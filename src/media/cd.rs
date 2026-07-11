use std::{
    fs, io,
    path::{Path, PathBuf},
    process::{Command, Stdio},
    thread,
    time::{Duration, Instant},
};

use anyhow::{bail, Context};
use base64::{engine::general_purpose::STANDARD, Engine};
use crossbeam_channel::Sender;
use regex::Regex;
use serde::Deserialize;
use sha1::{Digest, Sha1};

use crate::model::{
    CancelToken, CdDisc, CdDrive, CdTocTrack, ProgressInfo, TrackSource, UiEvent, WorkPhase,
};

use super::{probe, tools::sidecar};

#[cfg(target_os = "linux")]
pub fn enumerate_drives() -> anyhow::Result<Vec<CdDrive>> {
    let mut enumerator = udev::Enumerator::new()?;
    enumerator.match_subsystem("block")?;
    let mut drives = Vec::new();
    for device in enumerator.scan_devices()? {
        if device.property_value("ID_CDROM").is_none() {
            continue;
        }
        let Some(devnode) = device.devnode() else {
            continue;
        };
        let vendor = device
            .property_value("ID_VENDOR")
            .and_then(|value| value.to_str())
            .unwrap_or("");
        let model = device
            .property_value("ID_MODEL")
            .and_then(|value| value.to_str())
            .unwrap_or("Optical drive")
            .replace('_', " ");
        drives.push(CdDrive {
            device: devnode.to_path_buf(),
            name: format!("{vendor} {model}").trim().to_owned(),
            audio_media: device.property_value("ID_CDROM_MEDIA_AUDIO").is_some(),
            audio_tracks: device
                .property_value("ID_CDROM_MEDIA_TRACK_COUNT_AUDIO")
                .and_then(|value| value.to_str())
                .and_then(|value| value.parse().ok()),
        });
    }
    drives.sort_by(|a, b| a.device.cmp(&b.device));
    Ok(drives)
}

#[cfg(not(target_os = "linux"))]
#[cfg(not(any(target_os = "macos", target_os = "windows")))]
pub fn enumerate_drives() -> anyhow::Result<Vec<CdDrive>> {
    Ok(Vec::new())
}

#[cfg(any(target_os = "windows", test))]
#[derive(Debug, Deserialize)]
#[serde(rename_all = "PascalCase")]
struct WindowsCdDrive {
    drive: Option<String>,
    name: Option<String>,
    media_loaded: Option<bool>,
}

#[cfg(any(target_os = "windows", test))]
fn parse_windows_drives(json: &str) -> anyhow::Result<Vec<CdDrive>> {
    let drives: Vec<WindowsCdDrive> = serde_json::from_str(json)?;
    Ok(drives
        .into_iter()
        .filter_map(|drive| {
            let device = drive.drive?;
            Some(CdDrive {
                device: PathBuf::from(device),
                name: drive.name.unwrap_or_else(|| "Optical drive".into()),
                audio_media: drive.media_loaded.unwrap_or(false),
                audio_tracks: None,
            })
        })
        .collect())
}

#[cfg(target_os = "windows")]
pub fn enumerate_drives() -> anyhow::Result<Vec<CdDrive>> {
    let output = Command::new("powershell.exe")
        .args([
            "-NoProfile",
            "-NonInteractive",
            "-Command",
            "$d=@(Get-CimInstance Win32_CDROMDrive | Select-Object Drive,Name,MediaLoaded); ConvertTo-Json -Compress -InputObject $d",
        ])
        .output()
        .context("Could not ask Windows for its optical drives")?;
    if !output.status.success() {
        bail!(
            "Windows optical-drive detection failed: {}",
            String::from_utf8_lossy(&output.stderr).trim()
        );
    }
    parse_windows_drives(&String::from_utf8_lossy(&output.stdout))
}

#[cfg(any(target_os = "macos", test))]
fn parse_macos_drive_status(status: &str) -> Vec<CdDrive> {
    let lower = status.to_ascii_lowercase();
    if lower.contains("no optical drive") || lower.contains("not found") {
        return Vec::new();
    }
    vec![CdDrive {
        device: PathBuf::from("default"),
        name: "Default optical drive".into(),
        audio_media: lower.contains("type: audio") || lower.contains("audio cd"),
        audio_tracks: None,
    }]
}

#[cfg(target_os = "macos")]
pub fn enumerate_drives() -> anyhow::Result<Vec<CdDrive>> {
    let output = Command::new("drutil")
        .arg("status")
        .output()
        .context("Could not ask macOS for its optical drive")?;
    let status = format!(
        "{}\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    if !output.status.success() && status.to_ascii_lowercase().contains("no optical drive") {
        return Ok(Vec::new());
    }
    Ok(parse_macos_drive_status(&status))
}

fn cd_reader_path() -> PathBuf {
    let portable = sidecar("cd-paranoia");
    if portable.is_file() {
        portable
    } else {
        sidecar("cdparanoia")
    }
}

fn cd_reader_command(device: &Path) -> Command {
    let mut command = Command::new(cd_reader_path());
    if device != Path::new("default") {
        command.arg("-d").arg(device);
    }
    command
}

pub fn spawn_drive_monitor(tx: Sender<UiEvent>) {
    thread::spawn(move || {
        let mut previous = Vec::<(PathBuf, bool, Option<u32>)>::new();
        loop {
            if let Ok(drives) = enumerate_drives() {
                let snapshot = drives
                    .iter()
                    .map(|drive| (drive.device.clone(), drive.audio_media, drive.audio_tracks))
                    .collect::<Vec<_>>();
                if snapshot != previous {
                    previous = snapshot;
                    if tx.send(UiEvent::DrivesChanged(drives)).is_err() {
                        break;
                    }
                }
            }
            thread::sleep(Duration::from_secs(2));
        }
    });
}

pub fn spawn_read_disc(drive: CdDrive, tx: Sender<UiEvent>) {
    thread::spawn(move || match read_toc(&drive) {
        Ok(disc) => {
            if tx.send(UiEvent::DiscRead(Ok(disc.clone()))).is_err() {
                return;
            }
            let result = lookup_track_titles(&disc).map_err(|error| format!("{error:#}"));
            let _ = tx.send(UiEvent::DiscMetadata {
                device: drive.device.clone(),
                result,
            });
        }
        Err(error) => {
            let _ = tx.send(UiEvent::DiscRead(Err(format!("{error:#}"))));
        }
    });
}

pub fn spawn_lookup_titles(disc: CdDisc, tx: Sender<UiEvent>) {
    thread::spawn(move || {
        let device = disc.drive.device.clone();
        let result = lookup_track_titles(&disc).map_err(|error| format!("{error:#}"));
        let _ = tx.send(UiEvent::DiscMetadata { device, result });
    });
}

pub fn read_toc(drive: &CdDrive) -> anyhow::Result<CdDisc> {
    let output = cd_reader_command(&drive.device)
        .arg("-Q")
        .output()
        .with_context(|| "Could not start the bundled audio-CD reader")?;
    let text = format!(
        "{}\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    if !output.status.success() {
        if text.to_ascii_lowercase().contains("permission") {
            bail!(
                "Permission denied while opening {}. Add your user to the optical-drive group and sign in again.",
                drive.device.display()
            );
        }
        bail!("Could not read the disc table of contents: {}", text.trim());
    }
    let tracks = parse_toc(&text);
    if tracks.is_empty() {
        bail!("The inserted media has no readable audio tracks");
    }
    Ok(CdDisc {
        drive: drive.clone(),
        tracks,
    })
}

pub fn parse_toc(text: &str) -> Vec<CdTocTrack> {
    let line = Regex::new(r"(?m)^\s*(\d+)\.\s+(\d+)\s+\[[^\]]+\]\s+(\d+)\s+\[").unwrap();
    line.captures_iter(text)
        .filter_map(|captures| {
            let number = captures.get(1)?.as_str().parse().ok()?;
            let sectors: u64 = captures.get(2)?.as_str().parse().ok()?;
            let first_sector = captures.get(3)?.as_str().parse().ok()?;
            Some(CdTocTrack {
                number,
                first_sector,
                sectors,
                duration_secs: sectors as f64 / 75.0,
                title: None,
            })
        })
        .collect()
}

#[derive(Debug, Deserialize)]
struct MusicBrainzResponse {
    #[serde(default)]
    releases: Vec<MusicBrainzRelease>,
}

#[derive(Debug, Deserialize)]
struct MusicBrainzRelease {
    #[serde(default)]
    media: Vec<MusicBrainzMedium>,
}

#[derive(Debug, Deserialize)]
struct MusicBrainzMedium {
    #[serde(default)]
    discs: Vec<MusicBrainzDisc>,
    #[serde(default)]
    tracks: Vec<MusicBrainzTrack>,
}

#[derive(Debug, Deserialize)]
struct MusicBrainzDisc {
    id: String,
}

#[derive(Debug, Deserialize)]
struct MusicBrainzTrack {
    title: Option<String>,
    recording: Option<MusicBrainzRecording>,
}

#[derive(Debug, Deserialize)]
struct MusicBrainzRecording {
    title: Option<String>,
}

fn parse_musicbrainz_titles(
    json: &str,
    disc_id: &str,
    expected_tracks: usize,
) -> anyhow::Result<Vec<String>> {
    let response: MusicBrainzResponse = serde_json::from_str(json)?;
    let media = response
        .releases
        .iter()
        .flat_map(|release| &release.media)
        .filter(|medium| medium.tracks.len() == expected_tracks)
        .collect::<Vec<_>>();
    let medium = media
        .iter()
        .copied()
        .find(|medium| medium.discs.iter().any(|disc| disc.id == disc_id))
        .or_else(|| media.first().copied())
        .with_context(|| "MusicBrainz returned no matching CD track list")?;
    medium
        .tracks
        .iter()
        .enumerate()
        .map(|(index, track)| {
            track
                .recording
                .as_ref()
                .and_then(|recording| recording.title.as_deref())
                .or(track.title.as_deref())
                .map(str::trim)
                .filter(|title| !title.is_empty())
                .map(str::to_owned)
                .with_context(|| format!("MusicBrainz omitted the name of CD track {}", index + 1))
        })
        .collect()
}

fn musicbrainz_disc_identity(cd: &CdDisc) -> anyhow::Result<(String, String)> {
    let first = cd
        .tracks
        .first()
        .with_context(|| "Cannot identify an empty audio CD")?;
    let last = cd
        .tracks
        .last()
        .with_context(|| "Cannot identify an empty audio CD")?;
    let mut offsets = [0_u64; 100];
    offsets[0] = last.first_sector + last.sectors + 150;
    for track in &cd.tracks {
        let index = usize::try_from(track.number)?;
        if index >= offsets.len() {
            bail!("Audio CD track number {} is out of range", track.number);
        }
        offsets[index] = track.first_sector + 150;
    }
    let mut input = format!("{:02X}{:02X}", first.number, last.number);
    for offset in offsets {
        input.push_str(&format!("{offset:08X}"));
    }
    let digest = Sha1::digest(input.as_bytes());
    let id = STANDARD
        .encode(digest)
        .replace('+', ".")
        .replace('/', "_")
        .replace('=', "-");
    let toc = (first.number..=last.number).fold(
        format!("{} {} {}", first.number, last.number, offsets[0]),
        |mut toc, number| {
            toc.push_str(&format!(" {}", offsets[number as usize]));
            toc
        },
    );
    Ok((id, toc))
}

fn ca_bundle() -> Option<PathBuf> {
    let beside_executable = sidecar("ca-certificates.crt");
    let bundled = std::env::current_exe().ok().and_then(|executable| {
        executable
            .parent()?
            .parent()
            .map(|usr| usr.join("share/suture/ca-certificates.crt"))
    });
    [Some(beside_executable), bundled]
        .into_iter()
        .flatten()
        .chain([PathBuf::from("/etc/ssl/certs/ca-certificates.crt")])
        .find(|path| path.is_file())
}

fn musicbrainz_command(disc_id: &str, toc: &str) -> Command {
    let url = format!("https://musicbrainz.org/ws/2/discid/{disc_id}");
    let mut command = Command::new(sidecar("curl"));
    command.args([
        "--silent",
        "--show-error",
        "--fail",
        "--get",
        "--connect-timeout",
        "5",
        "--max-time",
        "20",
        "--retry",
        "2",
        "--retry-delay",
        "1",
        "--user-agent",
        "Suture/1.0.0 (https://github.com/Erik0318/Suture)",
        "--data-urlencode",
        "inc=recordings",
        "--data-urlencode",
        "fmt=json",
        "--data-urlencode",
        "cdstubs=no",
        "--data-urlencode",
    ]);
    command.arg(format!("toc={toc}"));
    if let Some(ca_bundle) = ca_bundle() {
        command.arg("--cacert").arg(ca_bundle);
    }
    command.arg(url);
    command
}

fn lookup_track_titles(disc: &CdDisc) -> anyhow::Result<Vec<String>> {
    let (disc_id, toc) = musicbrainz_disc_identity(disc)?;
    let output = musicbrainz_command(&disc_id, &toc)
        .output()
        .with_context(|| "Could not start the bundled MusicBrainz client")?;
    if !output.status.success() {
        let detail = String::from_utf8_lossy(&output.stderr);
        let detail = detail.trim();
        bail!(
            "MusicBrainz could not match this disc. Check the internet connection and retry{}",
            if detail.is_empty() {
                String::new()
            } else {
                format!(": {detail}")
            }
        );
    }
    parse_musicbrainz_titles(
        &String::from_utf8_lossy(&output.stdout),
        &disc_id,
        disc.tracks.len(),
    )
}

pub fn spawn_import(disc: CdDisc, cancel: CancelToken, tx: Sender<UiEvent>) {
    thread::spawn(move || match import_disc(&disc, &cancel, &tx) {
        Ok(tracks) => {
            let _ = tx.send(UiEvent::CdImported(tracks));
        }
        Err(_) if cancel.is_cancelled() => {
            let _ = tx.send(UiEvent::CdFailed("CD import cancelled".into()));
        }
        Err(error) => {
            let _ = tx.send(UiEvent::CdFailed(format!("{error:#}")));
        }
    });
}

pub fn spawn_export_tracks(
    disc: CdDisc,
    folder: PathBuf,
    cancel: CancelToken,
    tx: Sender<UiEvent>,
) {
    thread::spawn(move || match export_tracks(&disc, &folder, &cancel, &tx) {
        Ok(folder) => {
            let _ = tx.send(UiEvent::CdTracksExported(folder));
        }
        Err(_) if cancel.is_cancelled() => {
            let _ = tx.send(UiEvent::CdFailed("CD track export cancelled".into()));
        }
        Err(error) => {
            let _ = tx.send(UiEvent::CdFailed(format!("{error:#}")));
        }
    });
}

fn imported_bytes(folder: &Path) -> u64 {
    fs::read_dir(folder)
        .into_iter()
        .flatten()
        .filter_map(Result::ok)
        .filter(|entry| {
            entry
                .path()
                .extension()
                .and_then(|ext| ext.to_str())
                .is_some_and(|ext| ext.eq_ignore_ascii_case("wav"))
        })
        .filter_map(|entry| entry.metadata().ok())
        .map(|metadata| metadata.len().saturating_sub(44))
        .sum()
}

fn ripped_wav_files(folder: &Path) -> anyhow::Result<Vec<PathBuf>> {
    let mut files = fs::read_dir(folder)?
        .filter_map(Result::ok)
        .map(|entry| entry.path())
        .filter(|path| {
            path.extension()
                .and_then(|ext| ext.to_str())
                .is_some_and(|ext| ext.eq_ignore_ascii_case("wav"))
        })
        .collect::<Vec<_>>();
    files.sort();
    Ok(files)
}

fn rip_disc_to_folder(
    disc: &CdDisc,
    folder: &Path,
    cancel: &CancelToken,
    tx: &Sender<UiEvent>,
) -> anyhow::Result<Vec<PathBuf>> {
    let start = Instant::now();
    let total_bytes = disc.total_sectors().saturating_mul(2352).max(1);
    let mut child = cd_reader_command(&disc.drive.device)
        .arg("-B")
        .current_dir(folder)
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .context("Could not start audio-CD import")?;

    let status = loop {
        if cancel.is_cancelled() {
            let _ = child.kill();
            let _ = child.wait();
            bail!("CD import cancelled");
        }
        let bytes = imported_bytes(folder).min(total_bytes);
        let fraction = bytes as f32 / total_bytes as f32;
        let sectors = bytes / 2352;
        let mut cursor = 0_u64;
        let active_track = disc
            .tracks
            .iter()
            .position(|track| {
                cursor += track.sectors;
                sectors < cursor
            })
            .unwrap_or_else(|| disc.tracks.len().saturating_sub(1));
        let elapsed = start.elapsed().as_secs_f64();
        let eta = (fraction > 0.01).then(|| (elapsed / fraction as f64 - elapsed).max(0.0));
        let speed = if elapsed > 0.25 {
            Some(format!("{:.2}×", (bytes as f64 / 176_400.0) / elapsed))
        } else {
            None
        };
        let _ = tx.send(UiEvent::CdProgress(ProgressInfo {
            phase: WorkPhase::RippingDisc,
            fraction: Some(fraction),
            status: format!(
                "Reading track {} of {}",
                active_track + 1,
                disc.tracks.len()
            ),
            detail: format!("{} of {} audio sectors", sectors, disc.total_sectors()),
            elapsed_secs: elapsed,
            eta_secs: eta,
            speed,
            active_track: Some(active_track),
            track_count: disc.tracks.len(),
        }));
        if let Some(status) = child.try_wait()? {
            break status;
        }
        thread::sleep(Duration::from_millis(200));
    };
    if !status.success() {
        bail!(
            "The audio-CD reader stopped with exit code {:?}. The disc may be damaged or may have been removed.",
            status.code()
        );
    }

    let files = ripped_wav_files(folder)?;
    if files.len() != disc.tracks.len() {
        bail!(
            "Expected {} ripped tracks but found {}. Incomplete temporary data was removed.",
            disc.tracks.len(),
            files.len()
        );
    }
    Ok(files)
}

fn import_disc(
    disc: &CdDisc,
    cancel: &CancelToken,
    tx: &Sender<UiEvent>,
) -> anyhow::Result<Vec<crate::model::Track>> {
    let temp = tempfile::Builder::new().prefix("suture-cd-").tempdir()?;
    let files = rip_disc_to_folder(disc, temp.path(), cancel, tx)?;
    let mut tracks = Vec::with_capacity(files.len());
    for (index, path) in files.into_iter().enumerate() {
        let mut track = probe::probe_audio(&path, index)
            .with_context(|| format!("Could not verify ripped track {}", index + 1))?;
        let number = disc.tracks[index].number;
        track.source = TrackSource::AudioCd {
            device: disc.drive.device.clone(),
            disc_track: number,
        };
        track.title = disc.tracks[index]
            .title
            .clone()
            .or_else(|| Some(format!("Track {number:02}")));
        track.track_number = Some(number);
        tracks.push(track);
    }
    let _kept = temp.keep();
    Ok(tracks)
}

fn separate_track_paths(disc: &CdDisc, folder: &Path) -> Vec<PathBuf> {
    disc.tracks
        .iter()
        .map(|track| {
            let name = track
                .title
                .as_deref()
                .map(safe_filename_component)
                .filter(|name| !name.is_empty())
                .map(|title| format!("{:02} {title}.wav", track.number))
                .unwrap_or_else(|| format!("Track {:02}.wav", track.number));
            folder.join(name)
        })
        .collect()
}

fn safe_filename_component(title: &str) -> String {
    let cleaned = title
        .chars()
        .map(|character| match character {
            '/' | '\\' | ':' | '*' | '?' | '"' | '<' | '>' | '|' => ' ',
            character if character.is_control() => ' ',
            character => character,
        })
        .take(120)
        .collect::<String>();
    cleaned
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
        .trim_matches(['.', ' '])
        .to_owned()
}

fn ensure_targets_available(targets: &[PathBuf]) -> anyhow::Result<()> {
    if let Some(existing) = targets.iter().find(|path| path.exists()) {
        bail!(
            "Refusing to overwrite existing CD track {}. Choose another folder or move that file first.",
            existing.display()
        );
    }
    Ok(())
}

fn copy_tracks_without_overwrite(files: &[PathBuf], targets: &[PathBuf]) -> anyhow::Result<()> {
    if files.len() != targets.len() {
        bail!("The number of ripped CD tracks changed before they could be saved");
    }
    let mut written = Vec::<PathBuf>::new();
    for (source, target) in files.iter().zip(targets) {
        let mut input = match fs::File::open(source) {
            Ok(input) => input,
            Err(error) => {
                for path in &written {
                    let _ = fs::remove_file(path);
                }
                return Err(error).with_context(|| {
                    format!("Could not reopen ripped CD track {}", source.display())
                });
            }
        };
        let mut output = match fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(target)
        {
            Ok(output) => output,
            Err(error) => {
                for path in &written {
                    let _ = fs::remove_file(path);
                }
                return Err(error).with_context(|| {
                    format!("Could not create separate CD track {}", target.display())
                });
            }
        };
        let copied = (|| -> anyhow::Result<()> {
            io::copy(&mut input, &mut output)?;
            output.sync_all()?;
            Ok(())
        })();
        if let Err(error) = copied {
            let _ = fs::remove_file(target);
            for path in &written {
                let _ = fs::remove_file(path);
            }
            return Err(error)
                .with_context(|| format!("Could not save separate CD track {}", target.display()));
        }
        written.push(target.clone());
    }
    Ok(())
}

fn export_tracks(
    disc: &CdDisc,
    folder: &Path,
    cancel: &CancelToken,
    tx: &Sender<UiEvent>,
) -> anyhow::Result<PathBuf> {
    if !folder.is_dir() {
        bail!("The selected CD export folder no longer exists");
    }
    let targets = separate_track_paths(disc, folder);
    ensure_targets_available(&targets)?;
    let staging = tempfile::Builder::new()
        .prefix(".suture-cd-")
        .tempdir_in(folder)
        .with_context(|| format!("Could not write to {}", folder.display()))?;
    let files = rip_disc_to_folder(disc, staging.path(), cancel, tx)?;
    for (index, path) in files.iter().enumerate() {
        probe::probe_audio(path, index)
            .with_context(|| format!("Could not verify ripped CD track {}", index + 1))?;
    }
    ensure_targets_available(&targets)?;
    copy_tracks_without_overwrite(&files, &targets)?;
    Ok(folder.to_path_buf())
}

pub fn cd_reader_available() -> bool {
    Command::new(cd_reader_path())
        .arg("--version")
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .is_ok_and(|status| status.success())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_cdparanoia_toc() {
        let toc = r#"
Table of contents (audio tracks only):
track        length               begin        copy pre ch
===========================================================
  1.    21770 [04:50.20]        0 [00:00.00]    no   no  2
  2.    13200 [02:56.00]    21770 [04:50.20]    no   no  2
TOTAL 34970 [07:46.20]    (audio only)
"#;
        let tracks = parse_toc(toc);
        assert_eq!(tracks.len(), 2);
        assert_eq!(tracks[0].number, 1);
        assert_eq!(tracks[1].first_sector, 21_770);
        assert!((tracks[0].duration_secs - 290.266).abs() < 0.01);
    }

    #[test]
    fn parses_windows_optical_drives() {
        let json = r#"[{"Drive":"D:","Name":"USB DVD Drive","MediaLoaded":true}]"#;
        let drives = parse_windows_drives(json).unwrap();
        assert_eq!(drives.len(), 1);
        assert_eq!(drives[0].device, PathBuf::from("D:"));
        assert!(drives[0].audio_media);
    }

    #[test]
    fn parses_macos_audio_disc_status() {
        let drives = parse_macos_drive_status("Type: Audio CD\nName: External DVD");
        assert_eq!(drives.len(), 1);
        assert!(drives[0].audio_media);
        assert_eq!(drives[0].device, PathBuf::from("default"));
    }

    #[test]
    fn creates_the_reference_musicbrainz_disc_id() {
        let offsets = [
            150, 9700, 25887, 39297, 53795, 63735, 77517, 94877, 107270, 123552, 135522, 148422,
            161197, 174790, 192022, 205545, 218010, 228700, 239590, 255470, 266932, 288750, 303602,
        ];
        let tracks = offsets
            .windows(2)
            .enumerate()
            .map(|(index, pair)| CdTocTrack {
                number: index as u32 + 1,
                first_sector: pair[0] - 150,
                sectors: pair[1] - pair[0],
                duration_secs: (pair[1] - pair[0]) as f64 / 75.0,
                title: None,
            })
            .collect();
        let disc = CdDisc {
            drive: CdDrive {
                device: "default".into(),
                name: "Test drive".into(),
                audio_media: true,
                audio_tracks: Some(22),
            },
            tracks,
        };
        let (id, toc) = musicbrainz_disc_identity(&disc).unwrap();
        assert_eq!(id, "xUp1F2NkfP8s8jaeFn_Av3jNEI4-");
        assert_eq!(
            toc,
            "1 22 303602 150 9700 25887 39297 53795 63735 77517 94877 107270 123552 135522 148422 161197 174790 192022 205545 218010 228700 239590 255470 266932 288750"
        );
    }

    #[test]
    fn sector_progress_is_bounded() {
        let total = 10_000_u64 * 2352;
        let read = 2_500_u64 * 2352;
        assert_eq!(read as f32 / total as f32, 0.25);
    }

    #[test]
    fn separate_cd_tracks_use_stable_names() {
        let disc = CdDisc {
            drive: CdDrive {
                device: "/dev/sr0".into(),
                name: "Test drive".into(),
                audio_media: true,
                audio_tracks: Some(2),
            },
            tracks: vec![
                CdTocTrack {
                    number: 1,
                    first_sector: 0,
                    sectors: 75,
                    duration_secs: 1.0,
                    title: None,
                },
                CdTocTrack {
                    number: 12,
                    first_sector: 75,
                    sectors: 75,
                    duration_secs: 1.0,
                    title: None,
                },
            ],
        };
        let folder = Path::new("/music/disc");
        assert_eq!(
            separate_track_paths(&disc, folder),
            vec![folder.join("Track 01.wav"), folder.join("Track 12.wav")]
        );
    }

    #[test]
    fn separate_cd_export_refuses_existing_tracks() {
        let temp = tempfile::tempdir().unwrap();
        let existing = temp.path().join("Track 01.wav");
        fs::write(&existing, b"existing audio").unwrap();
        let error = ensure_targets_available(&[existing]).unwrap_err();
        assert!(error.to_string().contains("Refusing to overwrite"));
    }

    #[test]
    fn separate_cd_copy_rolls_back_without_overwriting() {
        let temp = tempfile::tempdir().unwrap();
        let source_one = temp.path().join("source-one.wav");
        let source_two = temp.path().join("source-two.wav");
        let target_one = temp.path().join("Track 01.wav");
        let target_two = temp.path().join("Track 02.wav");
        fs::write(&source_one, b"first").unwrap();
        fs::write(&source_two, b"second").unwrap();
        fs::write(&target_two, b"keep me").unwrap();

        let result = copy_tracks_without_overwrite(
            &[source_one, source_two],
            &[target_one.clone(), target_two.clone()],
        );
        assert!(result.is_err());
        assert!(!target_one.exists());
        assert_eq!(fs::read(target_two).unwrap(), b"keep me");
    }

    #[test]
    fn musicbrainz_titles_follow_the_matching_disc() {
        let json = r#"{
          "releases": [{
            "media": [{
              "discs": [{"id": "correct-disc"}],
              "tracks": [
                {"title": "Fallback", "recording": {"title": "Психолирика"}},
                {"title": "Суицидальное диско", "recording": {}}
              ]
            }]
          }]
        }"#;
        assert_eq!(
            parse_musicbrainz_titles(json, "correct-disc", 2).unwrap(),
            vec!["Психолирика", "Суицидальное диско"]
        );
    }

    #[test]
    fn musicbrainz_titles_accept_a_toc_fuzzy_match() {
        let json = r#"{
          "releases": [{
            "media": [{
              "discs": [{"id": "a-different-pressing"}],
              "tracks": [
                {"title": "First song"},
                {"title": "Second song"}
              ]
            }]
          }]
        }"#;
        assert_eq!(
            parse_musicbrainz_titles(json, "unrecognized-disc-id", 2).unwrap(),
            vec!["First song", "Second song"]
        );
    }

    #[test]
    fn musicbrainz_request_includes_the_full_toc() {
        let toc = "1 2 30150 150 15150";
        let command = musicbrainz_command("test-disc", toc);
        let args = command
            .get_args()
            .map(|arg| arg.to_string_lossy().into_owned())
            .collect::<Vec<_>>();
        assert!(args
            .windows(2)
            .any(|pair| pair[0] == "--data-urlencode" && pair[1] == "inc=recordings"));
        assert!(args
            .windows(2)
            .any(|pair| pair[0] == "--data-urlencode" && pair[1] == "fmt=json"));
        assert!(args
            .windows(2)
            .any(|pair| pair[0] == "--data-urlencode" && pair[1] == "cdstubs=no"));
        assert!(args
            .windows(2)
            .any(|pair| pair[0] == "--data-urlencode" && pair[1] == format!("toc={toc}")));
        assert_eq!(
            args.last().map(String::as_str),
            Some("https://musicbrainz.org/ws/2/discid/test-disc")
        );
    }

    #[test]
    fn cd_titles_become_safe_export_filenames() {
        assert_eq!(
            safe_filename_component("Bonus track: Тишина / Live"),
            "Bonus track Тишина Live"
        );
    }
}
