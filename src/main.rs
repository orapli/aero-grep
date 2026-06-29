#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod app;
mod config;
mod grep;
mod history;
mod models;

fn main() -> eframe::Result<()> {
    let native_options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_inner_size([1280.0, 800.0])
            .with_title("aero-grep"),
        ..Default::default()
    };
    eframe::run_native(
        "aero-grep",
        native_options,
        Box::new(|cc| Ok(Box::new(app::GrepApp::new(cc)))),
    )
}
