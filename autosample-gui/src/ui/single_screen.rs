// src/ui/single_screen.rs
use crate::state::AppState;
use crate::ui;
use crate::ui::progress::RunCommand;
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
                ui.heading("Setup");
                ui.add_space(6.0);

                ui.horizontal(|ui| {
                    ui.label("Preset:");
                    ui.text_edit_singleline(&mut state.preset_name);
                });

                ui.add_space(8.0);

                // IMPORTANT: only one scroll area for the whole sidebar
                egui::ScrollArea::vertical()
                    .auto_shrink([false, false])
                    .show(ui, |ui| {
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
    });

    cmd
}

fn readiness_check(state: &AppState) -> (bool, Vec<&'static str>) {
    let mut missing = Vec::new();

    if state.config.midi_out.trim().is_empty() {
        missing.push("Select a MIDI output device");
    }
    if state.config.audio_in.trim().is_empty() {
        missing.push("Select an audio input device");
    }
    if state.config.output.trim().is_empty() {
        missing.push("Choose an output directory");
    }
    if state.config.notes.trim().is_empty() {
        missing.push("Enter a note range/list");
    }
    if state.config.vel.trim().is_empty() {
        missing.push("Enter velocity layers");
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
    ui.horizontal(|ui| {
        let status_text = match state.engine_status {
            EngineStatus::Idle => "⚪ Idle",
            EngineStatus::Running => "🟢 Running",
            EngineStatus::Paused => "🟡 Paused",
            EngineStatus::Completed => "✅ Completed",
            EngineStatus::Error => "🔴 Error",
        };

        ui.label(egui::RichText::new(status_text).size(18.0));
        ui.add_space(20.0);

        if state.engine_status == EngineStatus::Idle || state.engine_status == EngineStatus::Completed
        {
            let start_btn = ui.add_enabled(ready, egui::Button::new("▶ Start"));
            if start_btn.clicked() {
                cmd = Some(RunCommand::Start);
            }

            if !ready {
                ui.add_space(10.0);
                ui.label(
                    egui::RichText::new("Fix checklist items to enable Start")
                        .color(egui::Color32::GRAY),
                );
            }
        } else if state.engine_status == EngineStatus::Running {
            if ui.button("⏹ Stop").clicked() {
                cmd = Some(RunCommand::Stop);
            }
        }
    });

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

    // Log output
    ui.group(|ui| {
        ui.horizontal(|ui| {
            ui.label(egui::RichText::new("Log").strong().size(16.0));
            ui.add_space(10.0);

            if ui.button("Clear").clicked() {
                cmd = Some(RunCommand::ClearLogs);
            }
        });

        ui.add_space(5.0);

        egui::ScrollArea::vertical()
            .max_height(320.0)
            .stick_to_bottom(true)
            .show(ui, |ui| {
                for log in &state.logs {
                    let color = match log.level {
                        LogLevel::Info => egui::Color32::LIGHT_GRAY,
                        LogLevel::Warning => egui::Color32::YELLOW,
                        LogLevel::Error => egui::Color32::RED,
                    };

                    ui.horizontal(|ui| {
                        ui.label(egui::RichText::new(&log.timestamp).color(egui::Color32::GRAY));
                        ui.label(egui::RichText::new(&log.message).color(color));
                    });
                }
            });
    });

    cmd
}
