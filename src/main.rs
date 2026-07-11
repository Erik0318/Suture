#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use std::process::{self, Command, Stdio};

fn self_test() -> Result<(), String> {
    let warnings = suture::media::tools::verify_media_tools();
    if !warnings.is_empty() {
        return Err(warnings.join("; "));
    }
    if !suture::media::cd::cd_reader_available() {
        return Err("The bundled audio-CD reader failed its startup check".into());
    }
    let curl = Command::new(suture::media::tools::sidecar("curl"))
        .arg("--version")
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .map_err(|error| format!("Could not start the bundled HTTPS client: {error}"))?;
    if !curl.success() {
        return Err("The bundled HTTPS client failed its startup check".into());
    }
    suture::media::cd::enumerate_drives()
        .map_err(|error| format!("Optical-drive discovery failed: {error:#}"))?;
    Ok(())
}

fn run_gui() -> eframe::Result<()> {
    let options = eframe::NativeOptions {
        viewport: eframe::egui::ViewportBuilder::default()
            .with_inner_size([1080.0, 720.0])
            .with_min_inner_size([620.0, 480.0])
            .with_resizable(true)
            .with_decorations(true),
        ..Default::default()
    };

    eframe::run_native(
        "Suture",
        options,
        Box::new(|cc| Ok(Box::new(suture::app::SutureApp::new(cc)))),
    )
}

fn main() {
    if std::env::args_os().any(|argument| argument == "--self-test") {
        if let Err(error) = self_test() {
            eprintln!("Suture self-test failed: {error}");
            process::exit(1);
        }
        return;
    }
    if let Err(error) = run_gui() {
        eprintln!("Suture could not start: {error}");
        process::exit(1);
    }
}
