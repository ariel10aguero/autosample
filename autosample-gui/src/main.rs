#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod app;
mod device_scan;
mod state;
mod ui;

use app::AutosampleApp;
use eframe::egui;

fn main() -> Result<(), eframe::Error> {
    tracing_subscriber::fmt::init();

    let mut viewport = egui::ViewportBuilder::default()
        .with_inner_size([1000.0, 700.0])
        .with_min_inner_size([800.0, 600.0])
        .with_title("Autosample");

    if let Ok(icon) = eframe::icon_data::from_png_bytes(include_bytes!("../assets/logo.png")) {
        viewport = viewport.with_icon(icon);
    }

    let options = eframe::NativeOptions {
        viewport,
        ..Default::default()
    };

    eframe::run_native(
        "Autosample",
        options,
        Box::new(|cc| Ok(Box::new(AutosampleApp::new(cc)))),
    )
}
