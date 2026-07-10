use std::path::PathBuf;
use std::sync::{atomic::AtomicBool, Arc};

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum TrackSource {
    Local,
    AudioCd { device: PathBuf, disc_track: u32 },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Track {
    pub path: PathBuf,
    pub source: TrackSource,
    pub display_name: String,
    pub title: Option<String>,
    pub artist: Option<String>,
    pub album: Option<String>,
    pub track_number: Option<u32>,
    pub disc_number: Option<u32>,
    pub codec: String,
    pub container: String,
    pub duration_secs: f64,
    pub sample_rate: Option<u32>,
    pub channels: Option<u32>,
    pub channel_layout: Option<String>,
    pub bit_depth: Option<u32>,
    pub bitrate: Option<u64>,
    pub lossless: bool,
    pub probe_error: Option<String>,
    pub original_index: usize,
}

impl Track {
    pub fn label(&self) -> &str {
        self.title.as_deref().unwrap_or(&self.display_name)
    }

    pub fn duration_label(&self) -> String {
        duration_label(self.duration_secs)
    }
}

pub fn duration_label(seconds: f64) -> String {
    if !seconds.is_finite() || seconds < 0.0 {
        return "--:--".into();
    }
    let total = seconds.round() as u64;
    let hours = total / 3600;
    let minutes = (total % 3600) / 60;
    let seconds = total % 60;
    if hours > 0 {
        format!("{hours}:{minutes:02}:{seconds:02}")
    } else {
        format!("{minutes}:{seconds:02}")
    }
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
pub enum ExportKind {
    Audio,
    Video,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
pub enum AudioFormat {
    Flac,
    Wav,
    Alac,
    Mp3,
    Aac,
    OggOpus,
    MkaFlac,
    MkaOpus,
    OriginalCopy,
}

impl AudioFormat {
    pub const ALL: [Self; 9] = [
        Self::Flac,
        Self::Wav,
        Self::Alac,
        Self::Mp3,
        Self::Aac,
        Self::OggOpus,
        Self::MkaFlac,
        Self::MkaOpus,
        Self::OriginalCopy,
    ];

    pub fn label(self) -> &'static str {
        match self {
            Self::Flac => "FLAC (lossless)",
            Self::Wav => "WAV PCM (lossless)",
            Self::Alac => "ALAC in M4A (lossless)",
            Self::Mp3 => "MP3 (lossy)",
            Self::Aac => "AAC in M4A (lossy)",
            Self::OggOpus => "Opus in OGG (lossy)",
            Self::MkaFlac => "FLAC in MKA (lossless)",
            Self::MkaOpus => "Opus in MKA (lossy)",
            Self::OriginalCopy => "Original stream copy",
        }
    }

    pub fn extension(self) -> &'static str {
        match self {
            Self::Flac => "flac",
            Self::Wav => "wav",
            Self::Alac | Self::Aac => "m4a",
            Self::Mp3 => "mp3",
            Self::OggOpus => "ogg",
            Self::MkaFlac | Self::MkaOpus | Self::OriginalCopy => "mka",
        }
    }

    pub fn is_lossy(self) -> bool {
        matches!(self, Self::Mp3 | Self::Aac | Self::OggOpus | Self::MkaOpus)
    }
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
pub enum VideoContainer {
    Mkv,
    Mp4,
}

impl VideoContainer {
    pub fn extension(self) -> &'static str {
        match self {
            Self::Mkv => "mkv",
            Self::Mp4 => "mp4",
        }
    }
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
pub enum VideoAudioCodec {
    Flac,
    Opus,
    Aac,
    Alac,
    Mp3,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
pub enum CoverMode {
    Fit,
    Fill,
    Original,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExportOptions {
    pub kind: ExportKind,
    pub audio_format: AudioFormat,
    pub video_container: VideoContainer,
    pub video_audio_codec: VideoAudioCodec,
    pub cover_mode: CoverMode,
    pub fps: u32,
    pub output: Option<PathBuf>,
    pub write_cue: bool,
    #[serde(skip)]
    pub replace_existing: bool,
}

impl Default for ExportOptions {
    fn default() -> Self {
        Self {
            kind: ExportKind::Audio,
            audio_format: AudioFormat::Flac,
            video_container: VideoContainer::Mkv,
            video_audio_codec: VideoAudioCodec::Flac,
            cover_mode: CoverMode::Fit,
            fps: 2,
            output: None,
            write_cue: false,
            replace_existing: false,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum WorkPhase {
    Scanning,
    Probing,
    ReadingDisc,
    RippingDisc,
    Preparing,
    Exporting,
    Validating,
}

#[derive(Debug, Clone)]
pub struct ProgressInfo {
    pub phase: WorkPhase,
    pub fraction: Option<f32>,
    pub status: String,
    pub detail: String,
    pub elapsed_secs: f64,
    pub eta_secs: Option<f64>,
    pub speed: Option<String>,
    pub active_track: Option<usize>,
    pub track_count: usize,
}

#[derive(Clone)]
pub struct CancelToken(pub Arc<AtomicBool>);

impl Default for CancelToken {
    fn default() -> Self {
        Self(Arc::new(AtomicBool::new(false)))
    }
}

impl CancelToken {
    pub fn cancel(&self) {
        self.0.store(true, std::sync::atomic::Ordering::Relaxed);
    }

    pub fn is_cancelled(&self) -> bool {
        self.0.load(std::sync::atomic::Ordering::Relaxed)
    }
}

#[derive(Debug, Clone)]
pub struct CdDrive {
    pub device: PathBuf,
    pub name: String,
    pub audio_media: bool,
}

#[derive(Debug, Clone)]
pub struct CdTocTrack {
    pub number: u32,
    pub first_sector: u64,
    pub sectors: u64,
    pub duration_secs: f64,
}

#[derive(Debug, Clone)]
pub struct CdDisc {
    pub drive: CdDrive,
    pub tracks: Vec<CdTocTrack>,
}

impl CdDisc {
    pub fn total_sectors(&self) -> u64 {
        self.tracks.iter().map(|t| t.sectors).sum()
    }

    pub fn total_duration(&self) -> f64 {
        self.tracks.iter().map(|t| t.duration_secs).sum()
    }
}

#[derive(Debug)]
pub enum UiEvent {
    ScanProgress(ProgressInfo),
    ScanFinished {
        tracks: Vec<Track>,
        warnings: Vec<String>,
        suggested_cover: Option<PathBuf>,
    },
    ScanFailed(String),
    DrivesChanged(Vec<CdDrive>),
    DiscRead(Result<CdDisc, String>),
    CdProgress(ProgressInfo),
    CdImported(Vec<Track>),
    CdFailed(String),
    ExportProgress(ProgressInfo),
    ExportFinished { output: PathBuf, warnings: Vec<String> },
    ExportFailed(String),
}
