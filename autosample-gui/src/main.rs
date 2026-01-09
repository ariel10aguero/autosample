#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod app;
mod state;
mod ui;

use app::AutosampleApp;
use eframe::egui;

fn main() -> Result<(), eframe::Error> {
    tracing_subscriber::fmt::init();

    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_inner_size([1000.0, 700.0])
            .with_min_inner_size([800.0, 600.0])
            .with_icon(load_icon()),
        ..Default::default()
    };

    eframe::run_native(
        "Autosample",
        options,
        Box::new(|cc| Ok(Box::new(AutosampleApp::new(cc)))),
    )
}

fn load_icon() -> egui::IconData {
    // TODO: Load app icon
    egui::IconData {
        rgba: vec![],
        width: 0,
        height: 0,
    }
}