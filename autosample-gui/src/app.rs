// src/app.rs
use crate::state::AppState;
use crate::ui;
use autosample_core::{AutosampleEngine, EngineStatus};
use crossbeam_channel::{unbounded, Receiver};
use eframe::egui;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::thread;

pub struct AutosampleApp {
    pub state: AppState,
    engine_running: Arc<AtomicBool>,
    event_rx: Option<Receiver<autosample_core::ProgressUpdate>>,
}

impl AutosampleApp {
    pub fn new(cc: &eframe::CreationContext<'_>) -> Self {
        cc.egui_ctx.set_visuals(egui::Visuals::dark());

        let mut state = AppState::default();
        state.refresh_devices();

        Self {
            state,
            engine_running: Arc::new(AtomicBool::new(false)),
            event_rx: None,
        }
    }

    pub fn start_session(&mut self) {
        if self.engine_running.load(Ordering::SeqCst) {
            return; // Already running
        }

        let (tx, rx) = unbounded();
        self.event_rx = Some(rx);
        self.engine_running.store(true, Ordering::SeqCst);

        let config = self.state.config.clone();
        let running = self.engine_running.clone();

        thread::spawn(move || {
            let mut engine = AutosampleEngine::new();
            let _ = engine.run(config, tx);
            running.store(false, Ordering::SeqCst);
        });

        self.state.engine_status = EngineStatus::Running;
    }

    pub fn stop_session(&mut self) {
        self.engine_running.store(false, Ordering::SeqCst);
        self.event_rx = None;
        self.state.engine_status = EngineStatus::Idle;
    }
}

impl eframe::App for AutosampleApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        // Process engine events
        if let Some(rx) = &self.event_rx {
            while let Ok(event) = rx.try_recv() {
                self.state.handle_engine_event(event);
            }
        }

        // Request continuous repaint while running
        if self.state.engine_status == EngineStatus::Running {
            ctx.request_repaint();
        }

        // Top menu bar
        egui::TopBottomPanel::top("menu_bar").show(ctx, |ui| {
            egui::menu::bar(ui, |ui| {
                ui.menu_button("File", |ui| {
                    if ui.button("Load Preset...").clicked() {
                        if let Some(path) = rfd::FileDialog::new()
                            .add_filter("JSON", &["json"])
                            .pick_file()
                        {
                            match self.state.load_preset(&path.display().to_string()) {
                                Ok(_) => {
                                    self.state.add_log(
                                        autosample_core::LogLevel::Info,
                                        format!("Loaded preset from {}", path.display()),
                                    );
                                }
                                Err(e) => {
                                    self.state.add_log(
                                        autosample_core::LogLevel::Error,
                                        format!("Failed to load preset: {}", e),
                                    );
                                }
                            }
                        }
                        ui.close_menu();
                    }
                    if ui.button("Save Preset...").clicked() {
                        if let Some(path) = rfd::FileDialog::new()
                            .add_filter("JSON", &["json"])
                            .save_file()
                        {
                            let mut path_str = path.display().to_string();
                            if !path_str.ends_with(".json") {
                                path_str.push_str(".json");
                            }
                            match self.state.save_preset(&path_str) {
                                Ok(_) => {
                                    self.state.add_log(
                                        autosample_core::LogLevel::Info,
                                        format!("Saved preset to {}", path_str),
                                    );
                                }
                                Err(e) => {
                                    self.state.add_log(
                                        autosample_core::LogLevel::Error,
                                        format!("Failed to save preset: {}", e),
                                    );
                                }
                            }
                        }
                        ui.close_menu();
                    }
                    ui.separator();
                    if ui.button("Quit").clicked() {
                        ctx.send_viewport_cmd(egui::ViewportCommand::Close);
                    }
                });

                ui.menu_button("Help", |ui| {
                    if ui.button("About").clicked() {
                        ui.close_menu();
                    }
                });
            });
        });

        // Single-screen UI
        if let Some(cmd) = ui::single_screen::show(ctx, &mut self.state) {
            match cmd {
                ui::progress::RunCommand::Start => self.start_session(),
                ui::progress::RunCommand::Stop => self.stop_session(),
                ui::progress::RunCommand::ClearLogs => self.state.logs.clear(),
                ui::progress::RunCommand::ClearProject => self.state.clear_project(),
            }
        }
    }
}
