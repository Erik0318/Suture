use std::{collections::HashMap, path::Path, process::Command};

use anyhow::{anyhow, Context};
use serde::Deserialize;

use crate::model::{Track, TrackSource};

use super::tools::sidecar;

#[derive(Debug, Deserialize)]
struct ProbeOutput {
    #[serde(default)]
    streams: Vec<ProbeStream>,
    format: Option<ProbeFormat>,
}

#[derive(Debug, Deserialize)]
struct ProbeStream {
    codec_type: Option<String>,
    codec_name: Option<String>,
    sample_rate: Option<String>,
    channels: Option<u32>,
    channel_layout: Option<String>,
    bits_per_sample: Option<u32>,
    bits_per_raw_sample: Option<String>,
    bit_rate: Option<String>,
    duration: Option<String>,
    #[serde(default)]
    tags: HashMap<String, String>,
}

#[derive(Debug, Deserialize)]
struct ProbeFormat {
    format_name: Option<String>,
    duration: Option<String>,
    bit_rate: Option<String>,
    #[serde(default)]
    tags: HashMap<String, String>,
}

fn tag<'a>(stream: &'a ProbeStream, format: &'a ProbeFormat, name: &str) -> Option<&'a str> {
    stream
        .tags
        .iter()
        .find(|(key, _)| key.eq_ignore_ascii_case(name))
        .map(|(_, value)| value.as_str())
        .or_else(|| {
            format
                .tags
                .iter()
                .find(|(key, _)| key.eq_ignore_ascii_case(name))
                .map(|(_, value)| value.as_str())
        })
}

fn parse_number(value: Option<&str>) -> Option<u32> {
    value?.split('/').next()?.trim().parse().ok()
}

fn parse_u32(value: Option<&str>) -> Option<u32> {
    value?.parse().ok()
}

fn parse_u64(value: Option<&str>) -> Option<u64> {
    value?.parse().ok()
}

fn parse_f64(value: Option<&str>) -> Option<f64> {
    value?.parse().ok()
}

pub fn probe_audio(path: &Path, original_index: usize) -> anyhow::Result<Track> {
    let output = Command::new(sidecar("ffprobe"))
        .args([
            "-v",
            "error",
            "-show_streams",
            "-show_format",
            "-of",
            "json",
        ])
        .arg(path)
        .output()
        .with_context(|| format!("Could not start ffprobe for {}", path.display()))?;

    if !output.status.success() {
        let message = String::from_utf8_lossy(&output.stderr).trim().to_owned();
        return Err(anyhow!(if message.is_empty() {
            "ffprobe rejected the file".to_owned()
        } else {
            message
        }));
    }

    let data: ProbeOutput = serde_json::from_slice(&output.stdout)
        .with_context(|| format!("Invalid ffprobe JSON for {}", path.display()))?;
    let stream = data
        .streams
        .iter()
        .find(|stream| stream.codec_type.as_deref() == Some("audio"))
        .ok_or_else(|| anyhow!("No audio stream"))?;
    let format = data
        .format
        .ok_or_else(|| anyhow!("Missing container information"))?;
    let codec = stream
        .codec_name
        .clone()
        .unwrap_or_else(|| "unknown".into());
    let duration_secs = parse_f64(stream.duration.as_deref())
        .or_else(|| parse_f64(format.duration.as_deref()))
        .unwrap_or(0.0);
    let lossless = matches!(
        codec.as_str(),
        "flac"
            | "alac"
            | "ape"
            | "wavpack"
            | "tta"
            | "pcm_s16le"
            | "pcm_s24le"
            | "pcm_s32le"
            | "pcm_s16be"
            | "pcm_s24be"
            | "pcm_f32le"
            | "pcm_f64le"
    );
    let display_name = path
        .file_name()
        .map(|name| name.to_string_lossy().into_owned())
        .unwrap_or_else(|| path.display().to_string());

    Ok(Track {
        path: path.to_path_buf(),
        source: TrackSource::Local,
        display_name,
        title: tag(stream, &format, "title").map(str::to_owned),
        artist: tag(stream, &format, "artist").map(str::to_owned),
        album: tag(stream, &format, "album").map(str::to_owned),
        track_number: parse_number(tag(stream, &format, "track")),
        disc_number: parse_number(tag(stream, &format, "disc")),
        codec,
        container: format.format_name.unwrap_or_else(|| "unknown".into()),
        duration_secs,
        sample_rate: parse_u32(stream.sample_rate.as_deref()),
        channels: stream.channels,
        channel_layout: stream.channel_layout.clone(),
        bit_depth: stream.bits_per_sample.or_else(|| {
            stream
                .bits_per_raw_sample
                .as_deref()
                .and_then(|value| value.parse().ok())
        }),
        bitrate: parse_u64(stream.bit_rate.as_deref())
            .or_else(|| parse_u64(format.bit_rate.as_deref())),
        lossless,
        probe_error: None,
        original_index,
    })
}

pub fn probe_duration(path: &Path) -> anyhow::Result<f64> {
    let output = Command::new(sidecar("ffprobe"))
        .args([
            "-v",
            "error",
            "-show_entries",
            "format=duration",
            "-of",
            "default=noprint_wrappers=1:nokey=1",
        ])
        .arg(path)
        .output()?;
    if !output.status.success() {
        return Err(anyhow!(String::from_utf8_lossy(&output.stderr)
            .trim()
            .to_owned()));
    }
    String::from_utf8_lossy(&output.stdout)
        .trim()
        .parse()
        .context("ffprobe did not return a duration")
}
