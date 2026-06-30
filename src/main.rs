#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod app;
mod config;
mod grep;
mod history;
mod models;

fn main() -> eframe::Result<()> {
    // Title-bar / taskbar icon. Built from the same artwork as the Windows
    // executable icon (assets/app.ico, embedded via winres) so the two match.
    let mut viewport = egui::ViewportBuilder::default()
        .with_inner_size([1280.0, 800.0])
        .with_title("aero-grep");
    if let Ok(icon) = eframe::icon_data::from_png_bytes(include_bytes!("../assets/app-icon.png")) {
        viewport = viewport.with_icon(std::sync::Arc::new(icon));
    }

    let native_options = eframe::NativeOptions {
        viewport,
        ..Default::default()
    };
    eframe::run_native(
        "aero-grep",
        native_options,
        Box::new(|cc| Ok(Box::new(app::GrepApp::new(cc)))),
    )
}
