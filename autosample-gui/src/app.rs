use crate::state::{AppState, Tab};
use crate::ui;
use autosample_core::AutosampleEngine;
use crossbeam_channel::{unbounded, Receiver};
use eframe::egui;
use std::sync::Arc;
use std::thread;

pub struct AutosampleApp {
    state: AppState,
    engine: Option<Arc<AutosampleEngine>>,
    event_rx: Option<Receiver<autosample_core::EngineEvent>>,
}

impl AutosampleApp {
    pub fn new(cc: &eframe::CreationContext<'_>) -> Self {
        cc.egui_ctx.set_visuals(egui::Visuals::dark());

        let mut state = AppState::default();
        state.refresh_devices();

        Self {
            state,
            engine: None,
            event_rx: None,
        }
    }

    fn start_session(&mut self) {
        if self.engine.is_some() {
            return; // Already running
        }

        let (tx, rx) = unbounded();
        self.event_rx = Some(rx);

        let config = self.config.clone();
        let mut engine = AutosampleEngine::new();

        let handle = thread::spawn(move || {
            let _ = engine.run(config, tx);
        });

        self.engine = Some(Arc::new(AutosampleEngine::new()));
        self.state.engine_status = autosample_core::EngineStatus::Running;
    }

    fn stop_session(&mut self) {
        if let Some(engine) = &self.engine {
            engine.cancel();
        }
        self.engine = None;
        self.event_rx = None;
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
        if self.state.engine_status == autosample_core::EngineStatus::Running {
            ctx.request_repaint();
        }

        // Top menu bar
        egui::TopBottomPanel::top("menu_bar").show(ctx, |ui| {
            egui::menu::bar(ui, |ui| {
                ui.menu_button("File", |ui| {
                    if ui.button("Load Preset...").clicked() {
                        // TODO: File dialog
                    }
                    if ui.button("Save Preset...").clicked() {
                        // TODO: File dialog
                    }
                    ui.separator();
                    if ui.button("Quit").clicked() {
                        ctx.send_viewport_cmd(egui::ViewportCommand::Close);
                    }
                });

                ui.menu_button("Help", |ui| {
                    if ui.button("About").clicked() {
                        // TODO: About dialog
                    }
                });
            });
        });

        // Tab bar
        egui::TopBottomPanel::top("tab_bar").show(ctx, |ui| {
            ui.horizontal(|ui| {
                ui.selectable_value(&mut self.state.active_tab, Tab::Devices, "🔌 Devices");
                ui.selectable_value(&mut self.state.active_tab, Tab::Session, "🎹 Session");
                ui.selectable_value(&mut self.state.active_tab, Tab::Processing, "⚙ Processing");
                ui.selectable_value(&mut self.state.active_tab, Tab::Run, "▶ Run");
            });
        });

        // Main content area
        egui::CentralPanel::default().show(ctx, |ui| {
            match self.state.active_tab {
                Tab::Devices => ui::devices::show(ui, &mut self.state),
                Tab::Session => ui::session::show(ui, &mut self.state),
                Tab::Processing => ui::processing::show(ui, &mut self.state),
                Tab::Run => ui::progress::show(ui, &mut self.state, self),
            }
        });
    }
}