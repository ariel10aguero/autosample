#![cfg_attr(
    all(not(debug_assertions), not(feature = "console")),
    windows_subsystem = "windows"
)]

mod app;
mod device_scan;
mod state;
mod ui;

use app::AutosampleApp;
use eframe::egui;

fn main() -> Result<(), eframe::Error> {
    init_logging();

    tracing::info!("Autosample GUI starting");

    let mut viewport = egui::ViewportBuilder::default()
        .with_inner_size([1000.0, 700.0])
        .with_min_inner_size([800.0, 600.0])
        .with_title("Autosample");

    match eframe::icon_data::from_png_bytes(include_bytes!("../assets/logo.png")) {
        Ok(icon) => {
            viewport = viewport.with_icon(icon);
        }
        Err(e) => {
            tracing::warn!("Could not load app icon: {}", e);
        }
    }

    let options = eframe::NativeOptions {
        viewport,
        ..Default::default()
    };

    tracing::info!("Launching eframe window");

    eframe::run_native(
        "Autosample",
        options,
        Box::new(|cc| {
            tracing::info!("eframe creation context ready, building app");
            Ok(Box::new(AutosampleApp::new(cc)))
        }),
    )
}

fn init_logging() {
    #[cfg(target_os = "windows")]
    {
        use std::fs::OpenOptions;

        let log_path = std::env::current_exe()
            .ok()
            .and_then(|p| p.parent().map(|p| p.join("autosample.log")))
            .unwrap_or_else(|| std::path::PathBuf::from("autosample.log"));

        if let Ok(file) = OpenOptions::new()
            .create(true)
            .append(true)
            .open(&log_path)
        {
            tracing_subscriber::fmt()
                .with_writer(std::sync::Mutex::new(file))
                .with_ansi(false)
                .init();
            return;
        }
    }

    tracing_subscriber::fmt::init();
}