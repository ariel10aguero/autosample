use crate::state::AppState;
use autosample_core::{EngineStatus, LogLevel};
use eframe::egui;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RunCommand {
    Start,
    Stop,
    ClearLogs,
}

pub fn show(ui: &mut egui::Ui, state: &mut AppState) -> Option<RunCommand> {
    let mut cmd = None;

    ui.heading("▶ Run Session");
    ui.add_space(10.0);

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

        if state.engine_status == EngineStatus::Idle || state.engine_status == EngineStatus::Completed {
            if ui.button("▶ Start").clicked() {
                cmd = Some(RunCommand::Start);
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
            .max_height(300.0)
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
