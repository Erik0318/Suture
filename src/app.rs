use std::{
    collections::BTreeSet,
    fs,
    path::{Path, PathBuf},
    time::Duration,
};

use crossbeam_channel::{unbounded, Receiver, Sender};
use eframe::egui::{self, Align, Color32, Layout, RichText, Sense, TextureHandle};

use crate::{
    media::{cd, cover, export, scan, sort, tools},
    model::{
        duration_label, AudioFormat, CancelToken, CdDisc, CdDrive, CoverMode, ExportKind,
        ExportOptions, ProgressInfo, Track, UiEvent, VideoContainer,
    },
    settings::Settings,
    ui,
};

pub struct SutureApp {
    tx: Sender<UiEvent>,
    rx: Receiver<UiEvent>,
    tracks: Vec<Track>,
    selected: BTreeSet<usize>,
    dragging: Option<usize>,
    cover_path: Option<PathBuf>,
    cover_texture: Option<TextureHandle>,
    options: ExportOptions,
    settings: Settings,
    progress: Option<ProgressInfo>,
    cancel: Option<CancelToken>,
    warnings: Vec<String>,
    error: Option<String>,
    completed_output: Option<PathBuf>,
    busy: bool,
    drives: Vec<CdDrive>,
    selected_drive: usize,
    toc_requested: Option<PathBuf>,
    disc: Option<CdDisc>,
    cd_reader_available: bool,
    confirm_overwrite: bool,
    cd_temp_dirs: BTreeSet<PathBuf>,
}

impl SutureApp {
    pub fn new(cc: &eframe::CreationContext<'_>) -> Self {
        let settings = Settings::load();
        if let Some(dark) = settings.dark_theme {
            cc.egui_ctx.set_visuals(if dark {
                egui::Visuals::dark()
            } else {
                egui::Visuals::light()
            });
        }
        let options = ExportOptions {
            kind: settings.export_kind,
            audio_format: settings.audio_format,
            video_container: settings.video_container,
            video_audio_codec: settings.video_audio_codec,
            cover_mode: settings.cover_mode,
            fps: settings.fps,
            ..Default::default()
        };
        let (tx, rx) = unbounded();
        cd::spawn_drive_monitor(tx.clone());
        let warnings = tools::verify_media_tools();
        let cd_reader_available = cd::cd_reader_available();
        Self {
            tx,
            rx,
            tracks: Vec::new(),
            selected: BTreeSet::new(),
            dragging: None,
            cover_path: None,
            cover_texture: None,
            options,
            settings,
            progress: None,
            cancel: None,
            warnings,
            error: None,
            completed_output: None,
            busy: false,
            drives: Vec::new(),
            selected_drive: 0,
            toc_requested: None,
            disc: None,
            cd_reader_available,
            confirm_overwrite: false,
            cd_temp_dirs: BTreeSet::new(),
        }
    }

    fn start_scan(&mut self, inputs: Vec<PathBuf>) {
        if self.busy || inputs.is_empty() {
            return;
        }
        if let Some(folder) = inputs
            .iter()
            .find(|path| path.is_dir())
            .cloned()
            .or_else(|| {
                inputs
                    .first()
                    .and_then(|path| path.parent().map(Path::to_path_buf))
            })
        {
            self.settings.last_input_folder = Some(folder);
        }
        self.busy = true;
        self.error = None;
        self.completed_output = None;
        self.progress = Some(ProgressInfo {
            phase: crate::model::WorkPhase::Scanning,
            fraction: None,
            status: "Finding audio files".into(),
            detail: "Reading the selected input".into(),
            elapsed_secs: 0.0,
            eta_secs: None,
            speed: None,
            active_track: None,
            track_count: 1,
        });
        scan::spawn(inputs, self.settings.include_subfolders, self.tx.clone());
    }

    fn set_cover(&mut self, path: PathBuf, ctx: &egui::Context) {
        match cover::load_color_image(&path) {
            Ok(image) => {
                self.cover_texture = Some(ctx.load_texture(
                    format!("cover:{}", path.display()),
                    image,
                    egui::TextureOptions::LINEAR,
                ));
                self.cover_path = Some(path);
            }
            Err(error) => self.error = Some(format!("Could not use that cover: {error:#}")),
        }
    }

    fn handle_dropped_files(&mut self, ctx: &egui::Context) {
        let dropped = ctx.input(|input| input.raw.dropped_files.clone());
        if dropped.is_empty() || self.busy {
            return;
        }
        let paths = dropped
            .into_iter()
            .filter_map(|file| file.path)
            .collect::<Vec<_>>();
        if let Some(image) = paths.iter().find(|path| cover::looks_like_image(path)) {
            self.set_cover(image.clone(), ctx);
        }
        let media = paths
            .into_iter()
            .filter(|path| path.is_dir() || !cover::looks_like_image(path))
            .collect();
        self.start_scan(media);
    }

    fn process_events(&mut self, ctx: &egui::Context) {
        while let Ok(event) = self.rx.try_recv() {
            match event {
                UiEvent::ScanProgress(progress)
                | UiEvent::CdProgress(progress)
                | UiEvent::ExportProgress(progress) => self.progress = Some(progress),
                UiEvent::ScanFinished {
                    tracks,
                    warnings,
                    suggested_cover,
                } => {
                    self.tracks.extend(tracks);
                    sort::sort_tracks(&mut self.tracks);
                    self.warnings.extend(warnings);
                    if self.cover_path.is_none() {
                        if let Some(path) = suggested_cover {
                            self.set_cover(path, ctx);
                        }
                    }
                    self.busy = false;
                    self.progress = None;
                }
                UiEvent::ScanFailed(message)
                | UiEvent::CdFailed(message)
                | UiEvent::ExportFailed(message) => {
                    self.error = Some(message);
                    self.busy = false;
                    self.progress = None;
                    self.cancel = None;
                }
                UiEvent::DrivesChanged(drives) => {
                    self.drives = drives;
                    self.selected_drive =
                        self.selected_drive.min(self.drives.len().saturating_sub(1));
                    if let Some(drive) = self.drives.iter().find(|drive| drive.audio_media) {
                        if self.toc_requested.as_ref() != Some(&drive.device) && !self.busy {
                            self.toc_requested = Some(drive.device.clone());
                            cd::spawn_read_disc(drive.clone(), self.tx.clone());
                        }
                    } else {
                        self.disc = None;
                        self.toc_requested = None;
                    }
                }
                UiEvent::DiscRead(result) => match result {
                    Ok(disc) => self.disc = Some(disc),
                    Err(message) => {
                        self.warnings.push(message);
                        self.disc = None;
                    }
                },
                UiEvent::CdImported(tracks) => {
                    for track in &tracks {
                        if let Some(parent) = track.path.parent() {
                            self.cd_temp_dirs.insert(parent.to_path_buf());
                        }
                    }
                    self.tracks.extend(tracks);
                    self.busy = false;
                    self.progress = None;
                    self.cancel = None;
                }
                UiEvent::ExportFinished { output, warnings } => {
                    self.completed_output = Some(output);
                    self.warnings.extend(warnings);
                    self.busy = false;
                    self.progress = None;
                    self.cancel = None;
                }
            }
        }
    }

    fn choose_folder(&mut self) {
        let mut dialog = rfd::FileDialog::new();
        if let Some(folder) = &self.settings.last_input_folder {
            dialog = dialog.set_directory(folder);
        }
        if let Some(folder) = dialog.pick_folder() {
            self.start_scan(vec![folder]);
        }
    }

    fn choose_files(&mut self) {
        let mut dialog = rfd::FileDialog::new().add_filter(
            "Audio",
            &[
                "flac", "mp3", "wav", "aiff", "aif", "m4a", "aac", "ogg", "opus", "wma", "ape",
                "wv", "tta",
            ],
        );
        if let Some(folder) = &self.settings.last_input_folder {
            dialog = dialog.set_directory(folder);
        }
        if let Some(files) = dialog.pick_files() {
            self.start_scan(files);
        }
    }

    fn choose_cover(&mut self, ctx: &egui::Context) {
        if let Some(path) = rfd::FileDialog::new().pick_file() {
            self.set_cover(path, ctx);
        }
    }

    fn choose_output(&mut self) {
        let extension = export::default_extension(&self.options);
        let mut dialog = rfd::FileDialog::new()
            .add_filter("Output", &[extension])
            .set_file_name(format!("Suture.{extension}"));
        if let Some(folder) = &self.settings.last_output_folder {
            dialog = dialog.set_directory(folder);
        }
        if let Some(path) = dialog.save_file() {
            self.settings.last_output_folder = path.parent().map(Path::to_path_buf);
            self.options.output = Some(path);
        }
    }

    fn request_export(&mut self) {
        if self.options.output.is_none() {
            self.choose_output();
        }
        let Some(output) = self.options.output.as_ref() else {
            return;
        };
        if output.exists() {
            self.confirm_overwrite = true;
        } else {
            self.start_export(false);
        }
    }

    fn start_export(&mut self, replace: bool) {
        if self.busy || self.tracks.is_empty() {
            return;
        }
        if self.options.kind == ExportKind::Video && self.cover_path.is_none() {
            self.error = Some("Choose or detect a usable cover before video export".into());
            return;
        }
        if self.options.audio_format == AudioFormat::OriginalCopy {
            if let Err(reason) = export::stream_copy_eligible(&self.tracks) {
                self.error = Some(format!("Original stream copy is unavailable: {reason}"));
                return;
            }
        }
        let cancel = CancelToken::default();
        let mut options = self.options.clone();
        options.replace_existing = replace;
        export::spawn(
            self.tracks.clone(),
            self.cover_path.clone(),
            options,
            cancel.clone(),
            self.tx.clone(),
        );
        self.cancel = Some(cancel);
        self.busy = true;
        self.error = None;
        self.completed_output = None;
    }

    fn import_cd(&mut self) {
        let Some(disc) = self.disc.clone() else {
            return;
        };
        if self.busy {
            return;
        }
        let cancel = CancelToken::default();
        cd::spawn_import(disc, cancel.clone(), self.tx.clone());
        self.cancel = Some(cancel);
        self.busy = true;
        self.error = None;
    }

    fn remove_selected(&mut self) {
        self.tracks = self
            .tracks
            .drain(..)
            .enumerate()
            .filter_map(|(index, track)| (!self.selected.contains(&index)).then_some(track))
            .collect();
        self.selected.clear();
    }

    fn move_selected(&mut self, direction: i32) {
        if direction < 0 {
            for index in self.selected.iter().copied().collect::<Vec<_>>() {
                if index > 0 && !self.selected.contains(&(index - 1)) {
                    self.tracks.swap(index, index - 1);
                    self.selected.remove(&index);
                    self.selected.insert(index - 1);
                }
            }
        } else {
            for index in self.selected.iter().rev().copied().collect::<Vec<_>>() {
                if index + 1 < self.tracks.len() && !self.selected.contains(&(index + 1)) {
                    self.tracks.swap(index, index + 1);
                    self.selected.remove(&index);
                    self.selected.insert(index + 1);
                }
            }
        }
    }

    fn move_selected_to_edge(&mut self, top: bool) {
        let mut selected_tracks = Vec::new();
        let mut others = Vec::new();
        for (index, track) in self.tracks.drain(..).enumerate() {
            if self.selected.contains(&index) {
                selected_tracks.push(track);
            } else {
                others.push(track);
            }
        }
        let selected_len = selected_tracks.len();
        self.tracks = if top {
            selected_tracks.into_iter().chain(others).collect()
        } else {
            others.into_iter().chain(selected_tracks).collect()
        };
        self.selected.clear();
        if top {
            self.selected.extend(0..selected_len);
        } else {
            self.selected
                .extend(self.tracks.len().saturating_sub(selected_len)..self.tracks.len());
        }
    }

    fn show_top_bar(&mut self, ui: &mut egui::Ui, ctx: &egui::Context) {
        ui.horizontal(|ui| {
            ui.heading("Suture");
            ui.label(RichText::new("stitch tracks into one continuous piece").weak());
            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                ui.menu_button("Settings", |ui| {
                    ui.checkbox(
                        &mut self.settings.include_subfolders,
                        "Include subfolders when scanning",
                    );
                    ui.checkbox(&mut self.settings.reduced_motion, "Reduce motion");
                    ui.separator();
                    ui.label("Theme");
                    if ui
                        .selectable_label(self.settings.dark_theme.is_none(), "System (on restart)")
                        .clicked()
                    {
                        self.settings.dark_theme = None;
                    }
                    if ui
                        .selectable_label(self.settings.dark_theme == Some(false), "Light")
                        .clicked()
                    {
                        self.settings.dark_theme = Some(false);
                        ctx.set_visuals(egui::Visuals::light());
                    }
                    if ui
                        .selectable_label(self.settings.dark_theme == Some(true), "Dark")
                        .clicked()
                    {
                        self.settings.dark_theme = Some(true);
                        ctx.set_visuals(egui::Visuals::dark());
                    }
                });
                if ui
                    .add_enabled(!self.busy, egui::Button::new("Add cover"))
                    .clicked()
                {
                    self.choose_cover(ctx);
                }
                if ui
                    .add_enabled(!self.busy, egui::Button::new("Add files"))
                    .clicked()
                {
                    self.choose_files();
                }
                if ui
                    .add_enabled(!self.busy, egui::Button::new("Select folder"))
                    .clicked()
                {
                    self.choose_folder();
                }
            });
        });
    }

    fn show_cd_card(&mut self, ui: &mut egui::Ui) {
        if self.drives.is_empty() {
            return;
        }
        egui::Frame::group(ui.style()).show(ui, |ui| {
            ui.horizontal(|ui| {
                ui.label(RichText::new("Audio CD").strong());
                if self.drives.len() > 1 {
                    egui::ComboBox::from_id_salt("cd-drive")
                        .selected_text(&self.drives[self.selected_drive].name)
                        .show_ui(ui, |ui| {
                            for (index, drive) in self.drives.iter().enumerate() {
                                ui.selectable_value(&mut self.selected_drive, index, &drive.name);
                            }
                        });
                } else {
                    ui.label(format!(
                        "{} ({})",
                        self.drives[0].name,
                        self.drives[0].device.display()
                    ));
                }
                if let Some(disc) = &self.disc {
                    ui.label(format!(
                        "{} tracks • {}",
                        disc.tracks.len(),
                        duration_label(disc.total_duration())
                    ));
                    if ui
                        .add_enabled(!self.busy, egui::Button::new("Import CD"))
                        .clicked()
                    {
                        self.import_cd();
                    }
                } else if !self.cd_reader_available {
                    ui.colored_label(Color32::YELLOW, "CD reader is unavailable in this build");
                } else if ui
                    .add_enabled(!self.busy, egui::Button::new("Check disc"))
                    .clicked()
                {
                    let drive = self.drives[self.selected_drive].clone();
                    self.toc_requested = Some(drive.device.clone());
                    cd::spawn_read_disc(drive, self.tx.clone());
                }
            });
        });
    }

    fn show_cover(&mut self, ui: &mut egui::Ui, ctx: &egui::Context) {
        ui.vertical(|ui| {
            ui.label(RichText::new("Cover").strong());
            let size = egui::vec2(ui.available_width().min(280.0), 250.0);
            egui::Frame::group(ui.style()).show(ui, |ui| {
                ui.set_min_size(size);
                if let Some(texture) = &self.cover_texture {
                    let source = texture.size_vec2();
                    let scale = (size.x / source.x).min(size.y / source.y);
                    ui.centered_and_justified(|ui| {
                        ui.image((texture.id(), source * scale));
                    });
                } else {
                    ui.centered_and_justified(|ui| {
                        ui.label("Drop a cover here\nor choose one manually");
                    });
                }
            });
            ui.horizontal(|ui| {
                if ui
                    .add_enabled(!self.busy, egui::Button::new("Replace"))
                    .clicked()
                {
                    self.choose_cover(ctx);
                }
                if ui
                    .add_enabled(
                        !self.busy && self.cover_path.is_some(),
                        egui::Button::new("Remove"),
                    )
                    .clicked()
                {
                    self.cover_path = None;
                    self.cover_texture = None;
                }
            });
        });
    }

    fn show_track_list(&mut self, ui: &mut egui::Ui) {
        let total: f64 = self.tracks.iter().map(|track| track.duration_secs).sum();
        ui.horizontal(|ui| {
            ui.label(RichText::new("Track order").strong());
            ui.label(format!(
                "{} tracks • {}",
                self.tracks.len(),
                duration_label(total)
            ));
            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                if ui
                    .add_enabled(
                        !self.busy && !self.tracks.is_empty(),
                        egui::Button::new("Restore automatic order"),
                    )
                    .clicked()
                {
                    sort::sort_tracks(&mut self.tracks);
                    self.selected.clear();
                }
            });
        });
        ui.horizontal(|ui| {
            if ui
                .add_enabled(
                    !self.busy && !self.selected.is_empty(),
                    egui::Button::new("Top"),
                )
                .clicked()
            {
                self.move_selected_to_edge(true);
            }
            if ui
                .add_enabled(
                    !self.busy && !self.selected.is_empty(),
                    egui::Button::new("Up"),
                )
                .clicked()
            {
                self.move_selected(-1);
            }
            if ui
                .add_enabled(
                    !self.busy && !self.selected.is_empty(),
                    egui::Button::new("Down"),
                )
                .clicked()
            {
                self.move_selected(1);
            }
            if ui
                .add_enabled(
                    !self.busy && !self.selected.is_empty(),
                    egui::Button::new("Bottom"),
                )
                .clicked()
            {
                self.move_selected_to_edge(false);
            }
            if ui
                .add_enabled(
                    !self.busy && !self.selected.is_empty(),
                    egui::Button::new("Remove"),
                )
                .clicked()
            {
                self.remove_selected();
            }
            if ui
                .add_enabled(
                    !self.busy && !self.tracks.is_empty(),
                    egui::Button::new("Clear"),
                )
                .clicked()
            {
                self.tracks.clear();
                self.selected.clear();
            }
        });

        let pointer_down = ui.input(|input| input.pointer.primary_down());
        if !pointer_down {
            self.dragging = None;
        }
        let mut remove_one = None;
        egui::ScrollArea::vertical()
            .max_height(310.0)
            .show(ui, |ui| {
                for index in 0..self.tracks.len() {
                    let track = &self.tracks[index];
                    let label = track.label().to_owned();
                    let duration = track.duration_label();
                    let codec = track.codec.clone();
                    let sample = track
                        .sample_rate
                        .map(|rate| format!("{} kHz", rate as f32 / 1000.0))
                        .unwrap_or_else(|| "? kHz".into());
                    let quality = if track.lossless { "lossless" } else { "lossy" };
                    let details = format!(
                        "{}\n{} • {} ch • {}\n{}",
                        track.path.display(),
                        track.container,
                        track.channels.unwrap_or(0),
                        track
                            .bit_depth
                            .map(|bits| format!("{bits}-bit"))
                            .unwrap_or_else(|| "unknown depth".into()),
                        [track.artist.as_deref(), track.album.as_deref()]
                            .into_iter()
                            .flatten()
                            .collect::<Vec<_>>()
                            .join(" — ")
                    );
                    let row = ui
                        .horizontal(|ui| {
                            let handle = ui.add(egui::Label::new("⠿").sense(Sense::drag()));
                            if handle.drag_started() {
                                self.dragging = Some(index);
                            }
                            let selected = self.selected.contains(&index);
                            if ui
                                .selectable_label(selected, format!("{:02}", index + 1))
                                .clicked()
                            {
                                if !ui
                                    .input(|input| input.modifiers.command || input.modifiers.shift)
                                {
                                    self.selected.clear();
                                }
                                if !self.selected.insert(index) {
                                    self.selected.remove(&index);
                                }
                            }
                            ui.label(label);
                            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                                if ui
                                    .add_enabled(!self.busy, egui::Button::new("×").small())
                                    .on_hover_text("Remove this track")
                                    .clicked()
                                {
                                    remove_one = Some(index);
                                }
                                ui.label(quality);
                                ui.label(sample);
                                ui.label(codec);
                                ui.label(duration);
                            });
                        })
                        .response;
                    let row = row.on_hover_text(details);
                    if row.hovered() && pointer_down {
                        if let Some(from) = self.dragging {
                            if from != index {
                                let track = self.tracks.remove(from);
                                self.tracks.insert(index, track);
                                self.dragging = Some(index);
                                self.selected.clear();
                                self.selected.insert(index);
                            }
                        }
                    }
                    ui.separator();
                }
                if self.tracks.is_empty() {
                    ui.add_space(40.0);
                    ui.centered_and_justified(|ui| {
                        ui.label("Add a folder, files, or an audio CD to begin")
                    });
                }
            });
        if let Some(index) = remove_one {
            self.tracks.remove(index);
            self.selected.clear();
        }
    }

    fn show_export_panel(&mut self, ui: &mut egui::Ui) {
        ui.separator();
        ui.horizontal(|ui| {
            ui.selectable_value(&mut self.options.kind, ExportKind::Audio, "Audio");
            ui.selectable_value(&mut self.options.kind, ExportKind::Video, "Video");
            ui.separator();
            if self.options.kind == ExportKind::Audio {
                egui::ComboBox::from_id_salt("audio-format")
                    .selected_text(self.options.audio_format.label())
                    .show_ui(ui, |ui| {
                        for format in AudioFormat::ALL {
                            ui.selectable_value(
                                &mut self.options.audio_format,
                                format,
                                format.label(),
                            );
                        }
                    });
                ui.checkbox(&mut self.options.write_cue, "Write CUE sheet");
            } else {
                egui::ComboBox::from_id_salt("video-container")
                    .selected_text(match self.options.video_container {
                        VideoContainer::Mkv => "MKV",
                        VideoContainer::Mp4 => "MP4",
                    })
                    .show_ui(ui, |ui| {
                        ui.selectable_value(
                            &mut self.options.video_container,
                            VideoContainer::Mkv,
                            "MKV",
                        );
                        ui.selectable_value(
                            &mut self.options.video_container,
                            VideoContainer::Mp4,
                            "MP4",
                        );
                    });
                let allowed = export::compatible_video_codecs(self.options.video_container);
                if !allowed.contains(&self.options.video_audio_codec) {
                    self.options.video_audio_codec = allowed[0];
                }
                egui::ComboBox::from_id_salt("video-audio")
                    .selected_text(format!("{:?}", self.options.video_audio_codec))
                    .show_ui(ui, |ui| {
                        for codec in allowed {
                            ui.selectable_value(
                                &mut self.options.video_audio_codec,
                                *codec,
                                format!("{codec:?}"),
                            );
                        }
                    });
                egui::ComboBox::from_id_salt("cover-mode")
                    .selected_text(format!("{:?}", self.options.cover_mode))
                    .show_ui(ui, |ui| {
                        ui.selectable_value(&mut self.options.cover_mode, CoverMode::Fit, "Fit");
                        ui.selectable_value(&mut self.options.cover_mode, CoverMode::Fill, "Fill");
                        ui.selectable_value(
                            &mut self.options.cover_mode,
                            CoverMode::Original,
                            "Original",
                        );
                    });
                egui::ComboBox::from_id_salt("fps")
                    .selected_text(format!("{} fps", self.options.fps))
                    .show_ui(ui, |ui| {
                        for fps in [1, 2, 5] {
                            ui.selectable_value(&mut self.options.fps, fps, format!("{fps} fps"));
                        }
                    });
            }
        });
        if self.options.kind == ExportKind::Audio
            && self.options.audio_format == AudioFormat::Flac
            && self.tracks.iter().any(|track| !track.lossless)
        {
            ui.colored_label(Color32::YELLOW, "Lossless output prevents further lossy compression, but cannot restore information already removed from the source.");
        }
        ui.horizontal(|ui| {
            let output = self
                .options
                .output
                .as_ref()
                .map(|path| path.display().to_string())
                .unwrap_or_else(|| "No output selected".into());
            ui.label(output);
            if ui
                .add_enabled(!self.busy, egui::Button::new("Choose output"))
                .clicked()
            {
                self.choose_output();
            }
            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                let can_export = !self.busy
                    && !self.tracks.is_empty()
                    && (self.options.kind == ExportKind::Audio || self.cover_path.is_some());
                if ui
                    .add_enabled(
                        can_export,
                        egui::Button::new(RichText::new("Export").strong()),
                    )
                    .clicked()
                {
                    self.request_export();
                }
            });
        });
    }

    fn show_notices(&mut self, ui: &mut egui::Ui) {
        if !self.warnings.is_empty() {
            egui::CollapsingHeader::new(format!("Warnings ({})", self.warnings.len())).show(
                ui,
                |ui| {
                    for warning in &self.warnings {
                        ui.colored_label(Color32::YELLOW, warning);
                    }
                    if ui.button("Dismiss warnings").clicked() {
                        self.warnings.clear();
                    }
                },
            );
        }
        if let Some(output) = self.completed_output.clone() {
            egui::Frame::group(ui.style()).show(ui, |ui| {
                ui.horizontal(|ui| {
                    ui.label(RichText::new(format!("Finished: {}", output.display())).strong());
                    if ui.button("Open containing folder").clicked() {
                        if let Some(parent) = output.parent() {
                            if let Err(error) = open::that(parent) {
                                self.error = Some(format!("Could not open the folder: {error}"));
                            }
                        }
                    }
                });
            });
        }
    }

    fn show_dialogs(&mut self, ctx: &egui::Context) {
        if let Some(message) = self.error.clone() {
            egui::Window::new("Suture could not finish")
                .collapsible(false)
                .resizable(true)
                .show(ctx, |ui| {
                    ui.label(message);
                    if ui.button("Close").clicked() {
                        self.error = None;
                    }
                });
        }
        if self.confirm_overwrite {
            egui::Window::new("Replace existing file?")
                .collapsible(false)
                .resizable(false)
                .show(ctx, |ui| {
                    ui.label("The selected output already exists. It will only be replaced after the new export passes validation.");
                    ui.horizontal(|ui| {
                        if ui.button("Replace after validation").clicked() {
                            self.confirm_overwrite = false;
                            self.start_export(true);
                        }
                        if ui.button("Cancel").clicked() {
                            self.confirm_overwrite = false;
                        }
                    });
                });
        }
    }
}

impl eframe::App for SutureApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        self.process_events(ctx);
        self.handle_dropped_files(ctx);
        if self.busy || self.dragging.is_some() {
            ctx.request_repaint_after(Duration::from_millis(33));
        } else {
            ctx.request_repaint_after(Duration::from_secs(1));
        }

        egui::TopBottomPanel::top("top").show(ctx, |ui| {
            ui.add_space(8.0);
            self.show_top_bar(ui, ctx);
            ui.add_space(8.0);
        });
        egui::CentralPanel::default().show(ctx, |ui| {
            self.show_cd_card(ui);
            ui.add_space(8.0);
            ui.columns(2, |columns| {
                self.show_cover(&mut columns[0], ctx);
                self.show_track_list(&mut columns[1]);
            });
            self.show_export_panel(ui);
            if let Some(progress) = &self.progress {
                ui.add_space(8.0);
                ui::progress::show(ui, progress, self.settings.reduced_motion);
                if ui.button("Cancel").clicked() {
                    if let Some(cancel) = &self.cancel {
                        cancel.cancel();
                    }
                }
            }
            self.show_notices(ui);
        });
        self.show_dialogs(ctx);

        if ctx.input(|input| input.modifiers.alt && input.key_pressed(egui::Key::ArrowUp)) {
            self.move_selected(-1);
        }
        if ctx.input(|input| input.modifiers.alt && input.key_pressed(egui::Key::ArrowDown)) {
            self.move_selected(1);
        }
    }

    fn save(&mut self, _storage: &mut dyn eframe::Storage) {
        self.settings.export_kind = self.options.kind;
        self.settings.audio_format = self.options.audio_format;
        self.settings.video_container = self.options.video_container;
        self.settings.video_audio_codec = self.options.video_audio_codec;
        self.settings.cover_mode = self.options.cover_mode;
        self.settings.fps = self.options.fps;
        let _ = self.settings.save();
    }
}

impl Drop for SutureApp {
    fn drop(&mut self) {
        if let Some(cancel) = &self.cancel {
            cancel.cancel();
        }
        let _ = self.settings.save();
        for folder in &self.cd_temp_dirs {
            let _ = fs::remove_dir_all(folder);
        }
    }
}
