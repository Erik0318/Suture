use std::{
    collections::VecDeque,
    path::{Path, PathBuf},
    sync::{Arc, Mutex},
    thread,
    time::Instant,
};

use crossbeam_channel::{unbounded, Sender};
use walkdir::WalkDir;

use crate::model::{ProgressInfo, UiEvent, WorkPhase};

use super::{cover, probe, sort};

fn is_hidden(path: &Path) -> bool {
    path.components().any(|component| {
        component
            .as_os_str()
            .to_str()
            .is_some_and(|name| name.starts_with('.') && name != "." && name != "..")
    })
}

fn collect_files(inputs: &[PathBuf], recursive: bool) -> (Vec<PathBuf>, Option<PathBuf>) {
    let mut files = Vec::new();
    let mut primary_folder = None;
    for input in inputs {
        if input.is_file() {
            files.push(input.clone());
            primary_folder.get_or_insert_with(|| input.parent().unwrap_or(Path::new(".")).to_path_buf());
        } else if input.is_dir() {
            primary_folder.get_or_insert_with(|| input.clone());
            let depth = if recursive { usize::MAX } else { 1 };
            files.extend(
                WalkDir::new(input)
                    .min_depth(1)
                    .max_depth(depth)
                    .into_iter()
                    .filter_map(Result::ok)
                    .map(|entry| entry.into_path())
                    .filter(|path| path.is_file() && !is_hidden(path)),
            );
        }
    }
    (files, primary_folder)
}

pub fn spawn(inputs: Vec<PathBuf>, recursive: bool, tx: Sender<UiEvent>) {
    thread::spawn(move || {
        let start = Instant::now();
        let (files, primary_folder) = collect_files(&inputs, recursive);
        let audio_candidates: Vec<_> = files
            .into_iter()
            .filter(|path| !cover::is_image_content(path))
            .collect();
        if audio_candidates.is_empty() {
            let _ = tx.send(UiEvent::ScanFailed("No files were found to probe".into()));
            return;
        }

        let total = audio_candidates.len();
        let queue = Arc::new(Mutex::new(
            audio_candidates
                .into_iter()
                .enumerate()
                .collect::<VecDeque<_>>(),
        ));
        let (result_tx, result_rx) = unbounded();
        for _ in 0..total.min(4) {
            let queue = queue.clone();
            let result_tx = result_tx.clone();
            thread::spawn(move || loop {
                let next = queue.lock().ok().and_then(|mut queue| queue.pop_front());
                let Some((index, path)) = next else { break };
                let result = probe::probe_audio(&path, index);
                let _ = result_tx.send((path, result));
            });
        }
        drop(result_tx);

        let mut tracks = Vec::new();
        let mut warnings = Vec::new();
        for (done, (path, result)) in result_rx.iter().enumerate() {
            match result {
                Ok(track) => tracks.push(track),
                Err(error) => warnings.push(format!("Skipped {}: {error}", path.display())),
            }
            let completed = done + 1;
            let elapsed = start.elapsed().as_secs_f64();
            let fraction = completed as f32 / total as f32;
            let eta = (elapsed / fraction as f64 - elapsed).max(0.0);
            let _ = tx.send(UiEvent::ScanProgress(ProgressInfo {
                phase: WorkPhase::Probing,
                fraction: Some(fraction),
                status: format!("Probing {completed} of {total} files"),
                detail: path.display().to_string(),
                elapsed_secs: elapsed,
                eta_secs: Some(eta),
                speed: None,
                active_track: Some(done),
                track_count: total,
            }));
        }

        if tracks.is_empty() {
            let message = if warnings.is_empty() {
                "No usable audio streams were found".into()
            } else {
                warnings.join("\n")
            };
            let _ = tx.send(UiEvent::ScanFailed(message));
            return;
        }
        let partial_numbering = sort::sort_tracks(&mut tracks);
        if partial_numbering {
            warnings.push("Partial filename numbering detected; numbered tracks were placed first".into());
        }
        let suggested_cover = primary_folder.as_deref().and_then(|folder| {
            cover::discover(folder, tracks.first().map(|track| track.path.as_path()))
        });
        let _ = tx.send(UiEvent::ScanFinished {
            tracks,
            warnings,
            suggested_cover,
        });
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hidden_components_are_ignored() {
        assert!(is_hidden(Path::new("album/.cache/track.flac")));
        assert!(!is_hidden(Path::new("album/track.flac")));
    }
}
