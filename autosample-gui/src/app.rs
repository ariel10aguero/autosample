use crate::state::{AppState, Tab};
use crate::ui;
use autosample_core::{AutosampleEngine, EngineStatus};
use crossbeam_channel::{unbounded, Receiver};
use eframe::egui;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
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
                        // TODO: File dialog
                        ui.close_menu();
                    }
                    if ui.button("Save Preset...").clicked() {
                        // TODO: File dialog
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

        // Tab bar
        egui::TopBottomPanel::top("tab_bar").show(ctx, |ui| {
            ui.horizontal(|ui| {
                ui.selectable_value(&mut self.state.active_tab, Tab::Devices, "🔌 Devices");
                ui.selectable_value(&mut self.state.active_tab, Tab::Session, "🎹 Session");
                ui.selectable_value(
                    &mut self.state.active_tab,
                    Tab::Processing,
                    "⚙ Processing",
                );
                ui.selectable_value(&mut self.state.active_tab, Tab::Run, "▶ Run");
            });
        });

        // Main content area
        egui::CentralPanel::default().show(ctx, |ui| {
            match self.state.active_tab {
                Tab::Devices => ui::devices::show(ui, &mut self.state),
                Tab::Session => ui::session::show(ui, &mut self.state),
                Tab::Processing => ui::processing::show(ui, &mut self.state),
                Tab::Run => {
                    if let Some(cmd) = ui::progress::show(ui, &mut self.state) {
                        match cmd {
                            ui::progress::RunCommand::Start => self.start_session(),
                            ui::progress::RunCommand::Stop => self.stop_session(),
                            ui::progress::RunCommand::ClearLogs => self.state.logs.clear(),
                        }
                    }
                }
            }
        });
    }
}