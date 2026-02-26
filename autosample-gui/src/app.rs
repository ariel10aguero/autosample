// src/app.rs
use crate::state::AppState;
use crate::ui;
use autosample_core::{AutosampleEngine, EngineStatus, LogLevel, ProgressUpdate};
use crossbeam_channel::{unbounded, Receiver};
use eframe::egui;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::thread;
use std::time::Duration;

pub struct AutosampleApp {
    pub state: AppState,
    engine_running: Arc<AtomicBool>,
    engine_cancel: Option<Arc<AtomicBool>>,
    event_rx: Option<Receiver<autosample_core::ProgressUpdate>>,
}

impl AutosampleApp {
    pub fn new(cc: &eframe::CreationContext<'_>) -> Self {
        cc.egui_ctx.set_visuals(egui::Visuals::dark());
        egui_extras::install_image_loaders(&cc.egui_ctx);

        let mut state = AppState::default();
        state.request_device_scan();

        Self {
            state,
            engine_running: Arc::new(AtomicBool::new(false)),
            engine_cancel: None,
            event_rx: None,
        }
    }

    pub fn start_session(&mut self) {
        if self.engine_running.load(Ordering::SeqCst) {
            return; // Already running
        }
        if self.state.is_device_scan_running() {
            self.state.add_log(
                LogLevel::Warning,
                "Start blocked: device refresh is still running. Wait until scanning completes."
                    .to_string(),
            );
            return;
        }

        let midi_target = self.state.config.midi_out.clone();
        self.state.add_log(
            LogLevel::Info,
            format!(
                "Preparing MIDI connection on app thread for '{}'",
                midi_target
            ),
        );
        let (midi_conn, connected_port_name, available_ports) =
            match autosample_core::midi::connect_midi_output_by_name(&midi_target) {
                Ok(connection) => connection,
                Err(error) => {
                    self.state.add_log(
                        LogLevel::Error,
                        format!(
                            "Start failed before engine launch:\nMIDI output initialization/connection failed for '{}': {:#}",
                            midi_target, error
                        ),
                    );
                    return;
                }
            };

        let (tx, rx) = unbounded();
        self.event_rx = Some(rx);
        self.engine_running.store(true, Ordering::SeqCst);
        let engine_cancel = Arc::new(AtomicBool::new(false));
        self.engine_cancel = Some(engine_cancel.clone());

        let config = self.state.config.clone();
        let running = self.engine_running.clone();
        let tx_for_errors = tx.clone();

        thread::spawn(move || {
            let mut engine = AutosampleEngine::new();
            if let Err(e) = engine.run_with_connected_midi_and_cancel(
                config,
                tx,
                midi_conn,
                connected_port_name,
                available_ports,
                engine_cancel,
            ) {
                let _ = tx_for_errors.send(ProgressUpdate::Log {
                    level: LogLevel::Error,
                    message: format!("Run failed:\n{:#}", e),
                });
                let _ = tx_for_errors.send(ProgressUpdate::Cancelled);
            }
            running.store(false, Ordering::SeqCst);
        });

        self.state.engine_status = EngineStatus::Running;
    }

    fn send_all_notes_off_best_effort(&mut self, reason: &str) {
        let midi_target = self.state.config.midi_out.trim().to_string();
        if midi_target.is_empty() {
            return;
        }

        self.state.add_log(
            LogLevel::Info,
            format!(
                "Sending emergency All Notes Off for '{}' ({})",
                midi_target, reason
            ),
        );

        match autosample_core::midi::connect_midi_output_by_name(&midi_target) {
            Ok((mut conn, connected_port_name, _)) => {
                if let Err(error) = autosample_core::midi::send_all_notes_off(&mut conn) {
                    self.state.add_log(
                        LogLevel::Warning,
                        format!(
                            "All Notes Off failed on '{}': {}",
                            connected_port_name, error
                        ),
                    );
                } else {
                    self.state.add_log(
                        LogLevel::Info,
                        format!("All Notes Off sent to '{}'", connected_port_name),
                    );
                }
            }
            Err(error) => {
                self.state.add_log(
                    LogLevel::Warning,
                    format!(
                        "Could not open MIDI output '{}' for emergency All Notes Off: {:#}",
                        midi_target, error
                    ),
                );
            }
        }
    }

    pub fn stop_session(&mut self) {
        self.send_all_notes_off_best_effort("stop button");
        if let Some(cancel_flag) = &self.engine_cancel {
            cancel_flag.store(true, Ordering::SeqCst);
        }
        self.state.add_log(
            LogLevel::Info,
            "Stop requested: cancelling active sampling loop.".to_string(),
        );
    }
}

impl Drop for AutosampleApp {
    fn drop(&mut self) {
        self.send_all_notes_off_best_effort("application shutdown");
    }
}

impl eframe::App for AutosampleApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        self.state.poll_device_scan_result();

        if !self.engine_running.load(Ordering::SeqCst) && self.engine_cancel.is_some() {
            self.engine_cancel = None;
        }

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
        if self.state.is_device_scan_running() {
            ctx.request_repaint_after(Duration::from_millis(100));
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
                        self.send_all_notes_off_best_effort("quit command");
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
