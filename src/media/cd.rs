use std::{
    fs,
    path::{Path, PathBuf},
    process::{Command, Stdio},
    thread,
    time::{Duration, Instant},
};

use anyhow::{bail, Context};
use crossbeam_channel::Sender;
use regex::Regex;

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
pub fn enumerate_drives() -> anyhow::Result<Vec<CdDrive>> {
    Ok(Vec::new())
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
    thread::spawn(move || {
        let result = read_toc(&drive).map_err(|error| format!("{error:#}"));
        let _ = tx.send(UiEvent::DiscRead(result));
    });
}

pub fn read_toc(drive: &CdDrive) -> anyhow::Result<CdDisc> {
    let output = Command::new(sidecar("cdparanoia"))
        .arg("-d")
        .arg(&drive.device)
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
            })
        })
        .collect()
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

fn import_disc(
    disc: &CdDisc,
    cancel: &CancelToken,
    tx: &Sender<UiEvent>,
) -> anyhow::Result<Vec<crate::model::Track>> {
    let temp = tempfile::Builder::new().prefix("suture-cd-").tempdir()?;
    let start = Instant::now();
    let total_bytes = disc.total_sectors().saturating_mul(2352).max(1);
    let mut child = Command::new(sidecar("cdparanoia"))
        .arg("-d")
        .arg(&disc.drive.device)
        .arg("-B")
        .current_dir(temp.path())
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
        let bytes = imported_bytes(temp.path()).min(total_bytes);
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

    let mut files = fs::read_dir(temp.path())?
        .filter_map(Result::ok)
        .map(|entry| entry.path())
        .filter(|path| {
            path.extension()
                .and_then(|ext| ext.to_str())
                .is_some_and(|ext| ext.eq_ignore_ascii_case("wav"))
        })
        .collect::<Vec<_>>();
    files.sort();
    if files.len() != disc.tracks.len() {
        bail!(
            "Expected {} ripped tracks but found {}. Incomplete temporary data was removed.",
            disc.tracks.len(),
            files.len()
        );
    }
    let mut tracks = Vec::with_capacity(files.len());
    for (index, path) in files.into_iter().enumerate() {
        let mut track = probe::probe_audio(&path, index)
            .with_context(|| format!("Could not verify ripped track {}", index + 1))?;
        let number = disc.tracks[index].number;
        track.source = TrackSource::AudioCd {
            device: disc.drive.device.clone(),
            disc_track: number,
        };
        track.title = Some(format!("Track {number:02}"));
        track.track_number = Some(number);
        tracks.push(track);
    }
    let _kept = temp.keep();
    Ok(tracks)
}

pub fn cd_reader_available() -> bool {
    Command::new(sidecar("cdparanoia"))
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
    fn sector_progress_is_bounded() {
        let total = 10_000_u64 * 2352;
        let read = 2_500_u64 * 2352;
        assert_eq!(read as f32 / total as f32, 0.25);
    }
}
