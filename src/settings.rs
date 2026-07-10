use std::{fs, path::PathBuf};

use directories::ProjectDirs;
use serde::{Deserialize, Serialize};

use crate::model::{AudioFormat, CoverMode, ExportKind, VideoAudioCodec, VideoContainer};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct Settings {
    pub last_input_folder: Option<PathBuf>,
    pub last_output_folder: Option<PathBuf>,
    pub export_kind: ExportKind,
    pub audio_format: AudioFormat,
    pub video_container: VideoContainer,
    pub video_audio_codec: VideoAudioCodec,
    pub cover_mode: CoverMode,
    pub fps: u32,
    pub include_subfolders: bool,
    pub dark_theme: Option<bool>,
    pub reduced_motion: bool,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            last_input_folder: None,
            last_output_folder: None,
            export_kind: ExportKind::Audio,
            audio_format: AudioFormat::Flac,
            video_container: VideoContainer::Mkv,
            video_audio_codec: VideoAudioCodec::Flac,
            cover_mode: CoverMode::Fit,
            fps: 2,
            include_subfolders: false,
            dark_theme: None,
            reduced_motion: false,
        }
    }
}

impl Settings {
    fn path() -> Option<PathBuf> {
        ProjectDirs::from("fun", "Twigoo Studio", "Suture")
            .map(|dirs| dirs.config_dir().join("settings.json"))
    }

    pub fn load() -> Self {
        let Some(path) = Self::path() else {
            return Self::default();
        };
        fs::read_to_string(path)
            .ok()
            .and_then(|text| serde_json::from_str(&text).ok())
            .unwrap_or_default()
    }

    pub fn save(&self) -> anyhow::Result<()> {
        let path = Self::path().ok_or_else(|| anyhow::anyhow!("No configuration directory"))?;
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)?;
        }
        let temp = path.with_extension("json.tmp");
        fs::write(&temp, serde_json::to_vec_pretty(self)?)?;
        fs::rename(temp, path)?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn settings_round_trip() {
        let mut settings = Settings::default();
        settings.include_subfolders = true;
        settings.audio_format = AudioFormat::MkaOpus;
        let json = serde_json::to_string(&settings).unwrap();
        let decoded: Settings = serde_json::from_str(&json).unwrap();
        assert!(decoded.include_subfolders);
        assert_eq!(decoded.audio_format, AudioFormat::MkaOpus);
    }
}

