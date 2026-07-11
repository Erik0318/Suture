#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

fn main() -> eframe::Result<()> {
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
