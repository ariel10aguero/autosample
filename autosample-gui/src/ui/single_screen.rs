// src/ui/single_screen.rs
use crate::state::AppState;
use crate::ui;
use crate::ui::progress::RunCommand;
use autosample_core::parse::{parse_notes, parse_velocities};
use eframe::egui;

pub fn show(ctx: &egui::Context, state: &mut AppState) -> Option<RunCommand> {
    let mut cmd = None;

    // LEFT: Setup sidebar (single scroll only)
    egui::SidePanel::left("setup_sidebar")
        .resizable(true)
        .default_width(380.0)
        .min_width(320.0)
        .show(ctx, |ui| {
            ui.vertical(|ui| {
                ui.add_space(8.0);
                ui.horizontal(|ui| {
                    ui.add_space(8.0);
                    ui.heading("Setup");
                });
                ui.add_space(6.0);

                ui.horizontal(|ui| {
                    ui.add_space(8.0);
                    ui.label("Preset:");
                    ui.text_edit_singleline(&mut state.preset_name);
                });

                ui.add_space(8.0);

                // IMPORTANT: only one scroll area for the whole sidebar
                egui::ScrollArea::vertical()
                    .auto_shrink([false, false])
                    .show(ui, |ui| {
                        ui.add_space(12.0);

                        egui::CollapsingHeader::new("🔌 Devices")
                            .default_open(true)
                            .show(ui, |ui| {
                                ui::devices::show(ui, state);
                            });

                        ui.add_space(8.0);

                        egui::CollapsingHeader::new("🎹 Session")
                            .default_open(true)
                            .show(ui, |ui| {
                                ui::session::show(ui, state);
                            });

                        ui.add_space(8.0);

                        egui::CollapsingHeader::new("⚙ Processing")
                            .default_open(false)
                            .show(ui, |ui| {
                                ui::processing::show(ui, state);
                            });

                        ui.add_space(12.0);
                    });
            });
        });

    // CENTER: Run / Progress / Logs
    egui::CentralPanel::default().show(ctx, |ui| {
        ui.heading("▶ Run");
        ui.add_space(8.0);

        let (ready, missing) = readiness_check(state);

        ui.group(|ui| {
            ui.label(egui::RichText::new("Checklist").strong().size(16.0));
            ui.add_space(4.0);

            if missing.is_empty() {
                ui.label(
                    egui::RichText::new("✅ Ready to run").color(egui::Color32::LIGHT_GREEN),
                );
            } else {
                for item in missing {
                    ui.label(
                        egui::RichText::new(format!("• {}", item)).color(egui::Color32::YELLOW),
                    );
                }
            }
        });

        ui.add_space(8.0);

        cmd = show_run_with_start_gate(ui, state, ready);

        let logo_size = egui::vec2(104.0, 104.0);
        let logo_rect = egui::Rect::from_min_size(
            egui::pos2(
                ui.max_rect().right() - logo_size.x - 8.0,
                ui.max_rect().bottom() - logo_size.y,
            ),
            logo_size,
        );
        ui.put(
            logo_rect,
            egui::Image::new(egui::include_image!("../../assets/logo.png"))
                // Crop away black padding baked into the source image.
                .uv(egui::Rect::from_min_max(
                    egui::pos2(0.12, 0.22),
                    egui::pos2(0.88, 0.80),
                ))
                .fit_to_exact_size(logo_size),
        );
    });

    cmd
}

fn readiness_check(state: &AppState) -> (bool, Vec<String>) {
    let mut missing = Vec::new();

    if state.config.midi_out.trim().is_empty() {
        missing.push("Select a MIDI output device".to_string());
    }
    if state.config.audio_in.trim().is_empty() {
        missing.push("Select an audio input device".to_string());
    }
    if state.config.output.trim().is_empty() {
        missing.push("Choose an output directory".to_string());
    }
    if state.config.notes.trim().is_empty() {
        missing.push("Enter a note range/list".to_string());
    } else if parse_notes(&state.config.notes)
        .map(|notes| notes.is_empty())
        .unwrap_or(true)
    {
        missing.push("Enter a valid note range/list (for example: C2..C6 or C4,E4,G4)".to_string());
    }
    if state.config.vel.trim().is_empty() {
        missing.push("Enter velocity layers".to_string());
    } else if parse_velocities(&state.config.vel)
        .map(|vel| vel.is_empty())
        .unwrap_or(true)
    {
        missing.push("Enter valid velocity layers (for example: 127,100,64)".to_string());
    }
    if state.is_device_scan_running() {
        missing.push("Wait for device refresh to complete".to_string());
    }

    (missing.is_empty(), missing)
}

fn show_run_with_start_gate(
    ui: &mut egui::Ui,
    state: &mut AppState,
    ready: bool,
) -> Option<RunCommand> {
    use autosample_core::{EngineStatus, LogLevel};

    let mut cmd = None;

    // Status and controls
    ui.add_space(4.0);
    ui.vertical(|ui| {
        ui.horizontal(|ui| {
            let status_text = match state.engine_status {
                EngineStatus::Idle => "Idle",
                EngineStatus::Running => "🟢 Running",
                EngineStatus::Paused => "🟡 Paused",
                EngineStatus::Completed => "✅ Completed",
                EngineStatus::Error => "🔴 Error",
            };

            ui.label(egui::RichText::new(status_text).size(18.0));
            ui.add_space(20.0);

            if state.engine_status == EngineStatus::Idle
                || state.engine_status == EngineStatus::Completed
            {
                let start_btn = ui.add_enabled(ready, egui::Button::new("▶ Start"));
                if start_btn.clicked() {
                    cmd = Some(RunCommand::Start);
                }
            } else if state.engine_status == EngineStatus::Running {
                if ui.button("⏹ Stop").clicked() {
                    cmd = Some(RunCommand::Stop);
                }
            }
        });

        if (state.engine_status == EngineStatus::Idle
            || state.engine_status == EngineStatus::Completed)
            && !ready
        {
            ui.add_space(4.0);
            ui.label(
                egui::RichText::new("Fix checklist items to enable Start")
                    .color(egui::Color32::GRAY),
            );
        }
    });

    ui.add_space(10.0);

    ui::session::show_output(ui, state);
    ui.add_space(10.0);

    // Progress
    if state.progress.total_samples > 0 {
        ui.group(|ui| {
            ui.label(egui::RichText::new("Progress").strong().size(16.0));
            ui.add_space(5.0);

            let denom = state.progress.total_samples.max(1) as f32;
            let progress_fraction = state.progress.current_index as f32 / denom;

            ui.add(
                egui::ProgressBar::new(progress_fraction).text(format!(
                    "{}/{} samples",
                    state.progress.current_index, state.progress.total_samples
                )),
            );

            ui.add_space(10.0);

            ui.label(format!(
                "Current: Note {} | Vel {} | RR {}",
                state.progress.current_note,
                state.progress.current_velocity,
                state.progress.current_rr
            ));

            ui.horizontal(|ui| {
                ui.label(format!("✅ Completed: {}", state.progress.samples_completed));
                ui.label(format!("⏭ Skipped: {}", state.progress.samples_skipped));
                ui.label(format!("❌ Failed: {}", state.progress.samples_failed));
            });
        });

        ui.add_space(10.0);
    }

    const LOG_PANEL_HEIGHT: f32 = 280.0;
    ui.allocate_ui_with_layout(
        egui::vec2(ui.available_width(), LOG_PANEL_HEIGHT),
        egui::Layout::top_down(egui::Align::Min),
        |ui| {
            ui.group(|ui| {
                ui.horizontal(|ui| {
                    ui.label(egui::RichText::new("Log").strong().size(16.0));
                    ui.add_space(10.0);

                    if ui.button("Clear").clicked() {
                        cmd = Some(RunCommand::ClearLogs);
                    }

                    if ui.button("Clear Project").clicked() {
                        cmd = Some(RunCommand::ClearProject);
                    }
                });

                ui.add_space(5.0);

                egui::ScrollArea::vertical()
                    .max_height(LOG_PANEL_HEIGHT - 56.0)
                    .stick_to_bottom(true)
                    .show(ui, |ui| {
                        for log in &state.logs {
                            let color = match log.level {
                                LogLevel::Info => egui::Color32::LIGHT_GRAY,
                                LogLevel::Warning => egui::Color32::YELLOW,
                                LogLevel::Error => egui::Color32::RED,
                            };

                            ui.horizontal(|ui| {
                                ui.label(
                                    egui::RichText::new(&log.timestamp).color(egui::Color32::GRAY),
                                );
                                ui.label(egui::RichText::new(&log.message).color(color));
                            });
                        }
                    });
            });
        },
    );

    cmd
}
