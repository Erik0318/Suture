use std::{
    ffi::OsString,
    fs,
    io::{BufRead, BufReader, Read},
    path::{Path, PathBuf},
    process::{Command, Stdio},
    sync::{Arc, Mutex},
    thread,
    time::{Duration, Instant},
};

use anyhow::{anyhow, bail, Context};
use crossbeam_channel::{unbounded, Sender};
use tempfile::TempDir;

use crate::model::{
    duration_label, AudioFormat, CancelToken, CoverMode, ExportKind, ExportOptions, ProgressInfo,
    Track, UiEvent, VideoAudioCodec, VideoContainer, WorkPhase,
};

use super::{probe, tools::sidecar};

struct Workspace {
    _temp: TempDir,
    inputs: Vec<PathBuf>,
    metadata: PathBuf,
    concat_manifest: PathBuf,
}

fn safe_extension(path: &Path) -> &str {
    path.extension()
        .and_then(|ext| ext.to_str())
        .filter(|ext| ext.chars().all(|c| c.is_ascii_alphanumeric()))
        .unwrap_or("bin")
}

fn link_or_copy(source: &Path, target: &Path) -> anyhow::Result<()> {
    #[cfg(unix)]
    {
        if std::os::unix::fs::symlink(source, target).is_ok() {
            return Ok(());
        }
    }
    fs::copy(source, target)?;
    Ok(())
}

fn escape_ffmetadata(value: &str) -> String {
    value
        .replace('\\', "\\\\")
        .replace('=', "\\=")
        .replace(';', "\\;")
        .replace('#', "\\#")
        .replace('\n', "\\n")
}

fn create_workspace(tracks: &[Track]) -> anyhow::Result<Workspace> {
    let temp = tempfile::Builder::new().prefix("suture-export-").tempdir()?;
    let mut inputs = Vec::with_capacity(tracks.len());
    let mut manifest = String::new();
    let mut metadata = String::from(";FFMETADATA1\n");
    let mut cursor_ms = 0_u64;

    for (index, track) in tracks.iter().enumerate() {
        let filename = format!("{:06}.{}", index + 1, safe_extension(&track.path));
        let target = temp.path().join(&filename);
        link_or_copy(&track.path, &target)
            .with_context(|| format!("Could not prepare {}", track.path.display()))?;
        manifest.push_str(&format!("file '{filename}'\n"));
        inputs.push(target);

        let end_ms = cursor_ms + (track.duration_secs.max(0.0) * 1000.0).round() as u64;
        metadata.push_str("[CHAPTER]\nTIMEBASE=1/1000\n");
        metadata.push_str(&format!("START={cursor_ms}\nEND={end_ms}\n"));
        metadata.push_str(&format!("title={}\n", escape_ffmetadata(track.label())));
        cursor_ms = end_ms;
    }

    let metadata_path = temp.path().join("chapters.ffmeta");
    let manifest_path = temp.path().join("concat.txt");
    fs::write(&metadata_path, metadata)?;
    fs::write(&manifest_path, manifest)?;
    Ok(Workspace {
        _temp: temp,
        inputs,
        metadata: metadata_path,
        concat_manifest: manifest_path,
    })
}

pub fn stream_copy_eligible(tracks: &[Track]) -> Result<(), String> {
    let Some(first) = tracks.first() else {
        return Err("No audio selected".into());
    };
    for track in &tracks[1..] {
        if track.codec != first.codec {
            return Err("The tracks use different audio codecs".into());
        }
        if track.sample_rate != first.sample_rate {
            return Err("The tracks use different sample rates".into());
        }
        if track.channels != first.channels || track.channel_layout != first.channel_layout {
            return Err("The tracks use different channel layouts".into());
        }
    }
    Ok(())
}

fn output_audio_args(format: AudioFormat) -> Vec<OsString> {
    let values: &[&str] = match format {
        AudioFormat::Flac | AudioFormat::MkaFlac => &["-c:a", "flac", "-compression_level", "8"],
        AudioFormat::Wav => &["-c:a", "pcm_s24le"],
        AudioFormat::Alac => &["-c:a", "alac"],
        AudioFormat::Mp3 => &["-c:a", "libmp3lame", "-q:a", "2"],
        AudioFormat::Aac => &["-c:a", "aac", "-b:a", "256k"],
        AudioFormat::OggOpus | AudioFormat::MkaOpus => {
            &["-c:a", "libopus", "-b:a", "192k", "-vbr", "on"]
        }
        AudioFormat::OriginalCopy => &["-c:a", "copy"],
    };
    values.iter().map(OsString::from).collect()
}

fn video_audio_args(codec: VideoAudioCodec) -> Vec<OsString> {
    let values: &[&str] = match codec {
        VideoAudioCodec::Flac => &["-c:a", "flac", "-compression_level", "8"],
        VideoAudioCodec::Opus => &["-c:a", "libopus", "-b:a", "192k"],
        VideoAudioCodec::Aac => &["-c:a", "aac", "-b:a", "256k"],
        VideoAudioCodec::Alac => &["-c:a", "alac"],
        VideoAudioCodec::Mp3 => &["-c:a", "libmp3lame", "-q:a", "2"],
    };
    values.iter().map(OsString::from).collect()
}

fn common_audio_parameters(tracks: &[Track]) -> anyhow::Result<(u32, String)> {
    let first = tracks.first().ok_or_else(|| anyhow!("No audio selected"))?;
    let first_channels = first.channels.unwrap_or(2);
    if tracks
        .iter()
        .any(|track| track.channels.unwrap_or(first_channels) != first_channels)
    {
        bail!("The selected tracks use different channel counts. Suture will not downmix them silently.");
    }
    let same_rate = tracks.iter().all(|track| track.sample_rate == first.sample_rate);
    let rate = if same_rate {
        first.sample_rate.unwrap_or(44_100)
    } else {
        44_100
    };
    let layout = first.channel_layout.clone().unwrap_or_else(|| match first_channels {
        1 => "mono".into(),
        2 => "stereo".into(),
        6 => "5.1".into(),
        8 => "7.1".into(),
        count => format!("{count}c"),
    });
    if !layout
        .chars()
        .all(|ch| ch.is_ascii_alphanumeric() || ".()_-".contains(ch))
    {
        bail!("Unsupported channel-layout name reported by ffprobe");
    }
    Ok((rate, layout))
}

fn audio_filter(indices: &[usize], rate: u32, layout: &str) -> String {
    let mut filters = String::new();
    for (slot, input) in indices.iter().enumerate() {
        filters.push_str(&format!(
            "[{input}:a:0]aresample={rate},aformat=sample_rates={rate}:channel_layouts={layout}[a{slot}];"
        ));
    }
    if indices.len() == 1 {
        filters.push_str("[a0]anull[aout]");
    } else {
        for slot in 0..indices.len() {
            filters.push_str(&format!("[a{slot}]"));
        }
        filters.push_str(&format!("concat=n={}:v=0:a=1[aout]", indices.len()));
    }
    filters
}

fn cover_filter(mode: CoverMode) -> &'static str {
    match mode {
        CoverMode::Fit => "scale=1920:1080:force_original_aspect_ratio=decrease,pad=1920:1080:(ow-iw)/2:(oh-ih)/2:black,setsar=1",
        CoverMode::Fill => "scale=1920:1080:force_original_aspect_ratio=increase,crop=1920:1080,setsar=1",
        CoverMode::Original => "scale=trunc(iw/2)*2:trunc(ih/2)*2,setsar=1",
    }
}

fn partial_output(final_output: &Path) -> anyhow::Result<PathBuf> {
    let parent = final_output
        .parent()
        .ok_or_else(|| anyhow!("The output has no parent directory"))?;
    let stem = final_output
        .file_stem()
        .and_then(|stem| stem.to_str())
        .unwrap_or("suture-output");
    let extension = final_output
        .extension()
        .and_then(|ext| ext.to_str())
        .ok_or_else(|| anyhow!("The output filename needs an extension"))?;
    Ok(parent.join(format!(
        ".{stem}.suture-partial-{}.{}",
        std::process::id(),
        extension
    )))
}

fn build_args(
    tracks: &[Track],
    cover: Option<&Path>,
    options: &ExportOptions,
    workspace: &Workspace,
    partial: &Path,
) -> anyhow::Result<Vec<OsString>> {
    let mut args: Vec<OsString> = ["-v", "error", "-y", "-progress", "pipe:1", "-nostats"]
        .into_iter()
        .map(OsString::from)
        .collect();

    if options.kind == ExportKind::Audio && options.audio_format == AudioFormat::OriginalCopy {
        stream_copy_eligible(tracks).map_err(|reason| anyhow!(reason))?;
        args.extend(["-f", "concat", "-safe", "1", "-i"].map(OsString::from));
        args.push(workspace.concat_manifest.as_os_str().to_owned());
        args.push("-i".into());
        args.push(workspace.metadata.as_os_str().to_owned());
        args.extend(
            ["-map", "0:a:0", "-map_metadata", "1", "-map_chapters", "1"]
                .map(OsString::from),
        );
        args.extend(output_audio_args(options.audio_format));
        args.push(partial.as_os_str().to_owned());
        return Ok(args);
    }

    let mut audio_indices = Vec::with_capacity(workspace.inputs.len());
    if options.kind == ExportKind::Video {
        let cover = cover.ok_or_else(|| anyhow!("A usable cover is required for video export"))?;
        args.extend(["-loop", "1", "-framerate"].map(OsString::from));
        args.push(options.fps.to_string().into());
        args.push("-i".into());
        args.push(cover.as_os_str().to_owned());
    }
    for input in &workspace.inputs {
        args.push("-i".into());
        args.push(input.as_os_str().to_owned());
        audio_indices.push(if options.kind == ExportKind::Video {
            audio_indices.len() + 1
        } else {
            audio_indices.len()
        });
    }
    let metadata_index = audio_indices.len() + usize::from(options.kind == ExportKind::Video);
    args.push("-i".into());
    args.push(workspace.metadata.as_os_str().to_owned());
    let (rate, layout) = common_audio_parameters(tracks)?;
    args.push("-filter_complex".into());
    args.push(audio_filter(&audio_indices, rate, &layout).into());

    if options.kind == ExportKind::Video {
        args.extend(["-map", "0:v:0", "-map", "[aout]", "-vf"].map(OsString::from));
        args.push(cover_filter(options.cover_mode).into());
        args.extend(
            ["-c:v", "libx264", "-tune", "stillimage", "-preset", "medium", "-crf", "30", "-pix_fmt", "yuv420p", "-r"]
                .map(OsString::from),
        );
        args.push(options.fps.to_string().into());
        args.push("-g".into());
        args.push((options.fps * 2).to_string().into());
        args.extend(video_audio_args(options.video_audio_codec));
        args.push("-shortest".into());
    } else {
        args.extend(["-map", "[aout]"].map(OsString::from));
        args.extend(output_audio_args(options.audio_format));
    }
    args.extend([
        "-map_metadata".into(),
        metadata_index.to_string().into(),
        "-map_chapters".into(),
        metadata_index.to_string().into(),
    ]);
    args.push(partial.as_os_str().to_owned());
    Ok(args)
}

fn parse_ffmpeg_time(value: &str) -> Option<f64> {
    let mut parts = value.trim().split(':');
    let hours: f64 = parts.next()?.parse().ok()?;
    let minutes: f64 = parts.next()?.parse().ok()?;
    let seconds: f64 = parts.next()?.parse().ok()?;
    Some(hours * 3600.0 + minutes * 60.0 + seconds)
}

fn write_cue(path: &Path, tracks: &[Track], output: &Path) -> anyhow::Result<()> {
    let mut cue = format!(
        "FILE \"{}\" WAVE\n",
        output.file_name().unwrap_or_default().to_string_lossy()
    );
    let mut cursor = 0.0_f64;
    for (index, track) in tracks.iter().enumerate() {
        let frames = (cursor * 75.0).round() as u64;
        let minutes = frames / (75 * 60);
        let seconds = (frames / 75) % 60;
        let frame = frames % 75;
        cue.push_str(&format!(
            "  TRACK {:02} AUDIO\n    TITLE \"{}\"\n    INDEX 01 {minutes:02}:{seconds:02}:{frame:02}\n",
            index + 1,
            track.label().replace('"', "'")
        ));
        cursor += track.duration_secs;
    }
    fs::write(path, cue)?;
    Ok(())
}

pub fn spawn(
    tracks: Vec<Track>,
    cover: Option<PathBuf>,
    options: ExportOptions,
    cancel: CancelToken,
    tx: Sender<UiEvent>,
) {
    thread::spawn(move || {
        let result = run_export(&tracks, cover.as_deref(), &options, &cancel, &tx);
        match result {
            Ok((output, warnings)) => {
                let _ = tx.send(UiEvent::ExportFinished { output, warnings });
            }
            Err(_error) if cancel.is_cancelled() => {
                let _ = tx.send(UiEvent::ExportFailed("Export cancelled".into()));
            }
            Err(error) => {
                let _ = tx.send(UiEvent::ExportFailed(format!("{error:#}")));
            }
        }
    });
}

fn run_export(
    tracks: &[Track],
    cover: Option<&Path>,
    options: &ExportOptions,
    cancel: &CancelToken,
    tx: &Sender<UiEvent>,
) -> anyhow::Result<(PathBuf, Vec<String>)> {
    let final_output = options.output.as_ref().ok_or_else(|| anyhow!("Choose an output file"))?;
    if tracks.is_empty() {
        bail!("No audio selected");
    }
    if final_output.exists() && !options.replace_existing {
        bail!("The output already exists. Confirm replacement in Suture before exporting.");
    }
    if let Some(parent) = final_output.parent() {
        fs::create_dir_all(parent)?;
    }
    let total_duration: f64 = tracks.iter().map(|track| track.duration_secs).sum();
    let start = Instant::now();
    let _ = tx.send(UiEvent::ExportProgress(ProgressInfo {
        phase: WorkPhase::Preparing,
        fraction: None,
        status: "Preparing safe temporary inputs".into(),
        detail: format!("{} tracks", tracks.len()),
        elapsed_secs: 0.0,
        eta_secs: None,
        speed: None,
        active_track: None,
        track_count: tracks.len(),
    }));

    let workspace = create_workspace(tracks)?;
    let partial = partial_output(final_output)?;
    let _ = fs::remove_file(&partial);
    let args = build_args(tracks, cover, options, &workspace, &partial)?;
    let mut child = Command::new(sidecar("ffmpeg"))
        .args(&args)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .context("Could not start the bundled FFmpeg")?;

    let stdout = child.stdout.take().ok_or_else(|| anyhow!("FFmpeg progress pipe was unavailable"))?;
    let stderr = child.stderr.take().ok_or_else(|| anyhow!("FFmpeg error pipe was unavailable"))?;
    let (progress_tx, progress_rx) = unbounded::<(Option<f64>, Option<String>)>();
    thread::spawn(move || {
        let mut current_time = None;
        let mut speed = None;
        for line in BufReader::new(stdout).lines().map_while(Result::ok) {
            if let Some(value) = line.strip_prefix("out_time=") {
                current_time = parse_ffmpeg_time(value);
            } else if let Some(value) = line.strip_prefix("speed=") {
                speed = Some(value.to_owned());
            } else if line == "progress=continue" || line == "progress=end" {
                let _ = progress_tx.send((current_time, speed.clone()));
            }
        }
    });
    let error_log = Arc::new(Mutex::new(String::new()));
    let error_log_writer = error_log.clone();
    thread::spawn(move || {
        let mut text = String::new();
        let _ = BufReader::new(stderr).take(512 * 1024).read_to_string(&mut text);
        if let Ok(mut target) = error_log_writer.lock() {
            *target = text;
        }
    });

    let status = loop {
        if cancel.is_cancelled() {
            let _ = child.kill();
            let _ = child.wait();
            let _ = fs::remove_file(&partial);
            bail!("Export cancelled");
        }
        while let Ok((encoded, speed)) = progress_rx.try_recv() {
            let encoded = encoded.unwrap_or(0.0).clamp(0.0, total_duration.max(0.0));
            let fraction = if total_duration > 0.0 {
                Some((encoded / total_duration) as f32)
            } else {
                None
            };
            let elapsed = start.elapsed().as_secs_f64();
            let eta = fraction.filter(|value| *value > 0.01).map(|value| {
                (elapsed / value as f64 - elapsed).max(0.0)
            });
            let active = tracks
                .iter()
                .scan(0.0, |cursor, track| {
                    *cursor += track.duration_secs;
                    Some(*cursor)
                })
                .position(|boundary| boundary >= encoded)
                .unwrap_or_else(|| tracks.len().saturating_sub(1));
            let _ = tx.send(UiEvent::ExportProgress(ProgressInfo {
                phase: WorkPhase::Exporting,
                fraction,
                status: format!(
                    "Stitching {} of {}{}",
                    duration_label(encoded),
                    duration_label(total_duration),
                    speed.as_deref().map(|value| format!(" at {value}")).unwrap_or_default()
                ),
                detail: tracks.get(active).map(|track| track.label().to_owned()).unwrap_or_default(),
                elapsed_secs: elapsed,
                eta_secs: eta,
                speed,
                active_track: Some(active),
                track_count: tracks.len(),
            }));
        }
        if let Some(status) = child.try_wait()? {
            break status;
        }
        thread::sleep(Duration::from_millis(100));
    };

    if !status.success() {
        let _ = fs::remove_file(&partial);
        let log = error_log.lock().map(|log| log.clone()).unwrap_or_default();
        bail!("FFmpeg failed (exit {:?}): {}", status.code(), log.trim());
    }
    if !partial.is_file() {
        bail!("FFmpeg reported success without producing an output file");
    }

    let _ = tx.send(UiEvent::ExportProgress(ProgressInfo {
        phase: WorkPhase::Validating,
        fraction: Some(0.98),
        status: "Checking duration, streams, and chapters".into(),
        detail: partial.display().to_string(),
        elapsed_secs: start.elapsed().as_secs_f64(),
        eta_secs: None,
        speed: None,
        active_track: Some(tracks.len().saturating_sub(1)),
        track_count: tracks.len(),
    }));
    let actual_duration = probe::probe_duration(&partial)?;
    let tolerance = (total_duration * 0.005).max(2.0);
    if (actual_duration - total_duration).abs() > tolerance {
        let _ = fs::remove_file(&partial);
        bail!(
            "Output validation failed: expected about {:.3}s, got {:.3}s",
            total_duration,
            actual_duration
        );
    }
    if options.replace_existing && final_output.exists() {
        #[cfg(not(unix))]
        fs::remove_file(final_output)?;
    }
    fs::rename(&partial, final_output)?;
    let mut warnings = Vec::new();
    if options.kind == ExportKind::Audio && options.audio_format == AudioFormat::Flac {
        if tracks.iter().any(|track| !track.lossless) {
            warnings.push("Lossless output prevents further lossy compression, but cannot restore information already removed from the source.".into());
        }
    }
    if options.write_cue && options.kind == ExportKind::Audio {
        let cue = final_output.with_extension("cue");
        if let Err(error) = write_cue(&cue, tracks, final_output) {
            warnings.push(format!("The audio was exported, but the CUE sheet failed: {error}"));
        }
    }
    Ok((final_output.clone(), warnings))
}

pub fn default_extension(options: &ExportOptions) -> &'static str {
    if options.kind == ExportKind::Audio {
        options.audio_format.extension()
    } else {
        options.video_container.extension()
    }
}

pub fn compatible_video_codecs(container: VideoContainer) -> &'static [VideoAudioCodec] {
    match container {
        VideoContainer::Mkv => &[
            VideoAudioCodec::Flac,
            VideoAudioCodec::Opus,
            VideoAudioCodec::Aac,
            VideoAudioCodec::Mp3,
        ],
        VideoContainer::Mp4 => &[VideoAudioCodec::Aac, VideoAudioCodec::Alac],
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn metadata_is_escaped() {
        assert_eq!(escape_ffmetadata("a=b;#c\\d"), "a\\=b\\;\\#c\\\\d");
    }

    #[test]
    fn ffmpeg_time_parser_accepts_fractional_seconds() {
        assert_eq!(parse_ffmpeg_time("01:02:03.500000"), Some(3723.5));
    }

    #[test]
    fn mp4_does_not_offer_flac_or_opus() {
        let codecs = compatible_video_codecs(VideoContainer::Mp4);
        assert!(!codecs.contains(&VideoAudioCodec::Flac));
        assert!(!codecs.contains(&VideoAudioCodec::Opus));
    }
}
