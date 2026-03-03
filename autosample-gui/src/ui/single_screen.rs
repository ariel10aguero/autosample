use crate::state::{AppState, AudioInputPermissionState};
use crate::ui;
use crate::ui::progress::RunCommand;
use autosample_core::parse::{parse_notes, parse_velocities};
use eframe::egui;
use std::path::PathBuf;
use std::process::Command;

pub fn show(ctx: &egui::Context, state: &mut AppState) -> Option<RunCommand> {
    let mut cmd = None;

    // ── LEFT: Setup sidebar ────────────────────────────────────────────────
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
                    let response = ui.add(
                        egui::TextEdit::singleline(&mut state.preset_name)
                            .hint_text("Untitled"),
                    );
                    if response.changed() {
                        state.config.prefix = state.preset_name.clone();
                    }
                });

                ui.add_space(8.0);

                egui::ScrollArea::vertical()
                    .auto_shrink([false, false])
                    .show(ui, |ui| {
                        ui.add_space(12.0);

                        egui::CollapsingHeader::new("🔌 Devices")
                            .default_open(false)
                            .show(ui, |ui| {
                                ui::devices::show(ui, state);
                            });

                        ui.add_space(8.0);

                        egui::CollapsingHeader::new("🎹 Session")
                            .default_open(false)
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

    // ── CENTER: Run / Progress / Logs ──────────────────────────────────────
    egui::CentralPanel::default().show(ctx, |ui| {
        ui.heading("▶ Run");
        ui.add_space(8.0);

        // Permission warning banner at the top of the run panel
        show_permission_warning_banner(ui, state);

        let (ready, missing) = readiness_check(state);

        ui.group(|ui| {
            ui.label(egui::RichText::new("Checklist").strong().size(16.0));
            ui.add_space(4.0);

            if missing.is_empty() {
                ui.label(
                    egui::RichText::new("✅ Ready to run")
                        .color(egui::Color32::LIGHT_GREEN),
                );
            } else {
                for item in &missing {
                    ui.label(
                        egui::RichText::new(format!("• {}", item))
                            .color(egui::Color32::YELLOW),
                    );
                }
            }
        });

        ui.add_space(8.0);

        cmd = show_run_controls(ui, state, ready);

        // Logo (bottom-right corner)
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
                .uv(egui::Rect::from_min_max(
                    egui::pos2(0.12, 0.22),
                    egui::pos2(0.88, 0.80),
                ))
                .fit_to_exact_size(logo_size),
        );
    });

    cmd
}

// ---------------------------------------------------------------------------
// Permission warning banner (shown in the central run panel)
// ---------------------------------------------------------------------------

fn show_permission_warning_banner(ui: &mut egui::Ui, state: &mut AppState) {
    match &state.audio_permission_state.clone() {
        AudioInputPermissionState::Granted | AudioInputPermissionState::Checking => {
            // No banner needed when granted or currently checking
        }

        AudioInputPermissionState::Unknown => {
            egui::Frame::none()
                .fill(egui::Color32::from_rgb(60, 55, 20))
                .inner_margin(egui::Margin::same(8.0))
                .rounding(egui::Rounding::same(4.0))
                .show(ui, |ui| {
                    ui.horizontal(|ui| {
                        ui.label(
                            egui::RichText::new("⚠ Microphone permission has not been checked.")
                                .color(egui::Color32::YELLOW),
                        );
                        ui.add_space(8.0);
                        if ui.button("🎤 Request Permission").clicked() {
                            state.request_audio_permission_recheck();
                        }

                        #[cfg(target_os = "windows")]
                        if ui.button("⚙ Open Settings").clicked() {
                            let _ = std::process::Command::new("cmd")
                                .args(["/c", "start", "ms-settings:privacy-microphone"])
                                .spawn();
                        }
                    });
                });
            ui.add_space(6.0);
        }

        AudioInputPermissionState::Denied(reason) => {
            egui::Frame::none()
                .fill(egui::Color32::from_rgb(70, 20, 20))
                .inner_margin(egui::Margin::same(8.0))
                .rounding(egui::Rounding::same(4.0))
                .show(ui, |ui| {
                    ui.label(
                        egui::RichText::new("✖ Microphone access is blocked")
                            .color(egui::Color32::RED)
                            .strong(),
                    );
                    ui.label(
                        egui::RichText::new(reason)
                            .color(egui::Color32::from_rgb(255, 160, 160))
                            .small(),
                    );
                    ui.add_space(4.0);
                    ui.horizontal(|ui| {
                        if ui.button("🎤 Request Permission").clicked() {
                            state.request_audio_permission_recheck();
                        }

                        #[cfg(target_os = "windows")]
                        if ui.button("⚙ Open Windows Microphone Settings").clicked() {
                            let _ = std::process::Command::new("cmd")
                                .args([
                                    "/c",
                                    "start",
                                    "ms-settings:privacy-microphone",
                                ])
                                .spawn();
                        }
                    });
                });
            ui.add_space(6.0);
        }
    }
}

// ---------------------------------------------------------------------------
// Readiness check
// ---------------------------------------------------------------------------

fn readiness_check(state: &AppState) -> (bool, Vec<String>) {
    let mut missing = Vec::new();

    if state.config.midi_out.trim().is_empty() {
        missing.push("Select a MIDI output device".to_string());
    }
    if state.config.audio_in.trim().is_empty() {
        missing.push("Select an audio input device".to_string());
    }
    if !matches!(
        state.audio_permission_state,
        AudioInputPermissionState::Granted
    ) {
        missing.push(
            "Grant microphone permission (click \"Request Permission\" above)".to_string(),
        );
    }
    if state.config.output.trim().is_empty() {
        missing.push("Choose an output directory".to_string());
    }
    if state.config.notes.trim().is_empty() {
        missing.push("Enter a note range/list".to_string());
    } else if parse_notes(&state.config.notes)
        .map(|n| n.is_empty())
        .unwrap_or(true)
    {
        missing.push(
            "Enter a valid note range/list (e.g. C2..C6 or C4,E4,G4)".to_string(),
        );
    }
    if state.config.vel.trim().is_empty() {
        missing.push("Enter velocity layers".to_string());
    } else if parse_velocities(&state.config.vel)
        .map(|v| v.is_empty())
        .unwrap_or(true)
    {
        missing.push(
            "Enter valid velocity layers (e.g. 127,100,64)".to_string(),
        );
    }
    if state.is_device_scan_running() {
        missing.push("Wait for device refresh to complete".to_string());
    }

    (missing.is_empty(), missing)
}

// ---------------------------------------------------------------------------
// Run controls + progress + log
// ---------------------------------------------------------------------------

fn show_run_controls(
    ui: &mut egui::Ui,
    state: &mut AppState,
    ready: bool,
) -> Option<RunCommand> {
    use autosample_core::{EngineStatus, LogLevel};

    let mut cmd = None;

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
                let start_btn =
                    ui.add_enabled(ready, egui::Button::new("▶ Start"));
                if start_btn.clicked() {
                    cmd = Some(RunCommand::Start);
                }

                if state.engine_status == EngineStatus::Completed {
                    let open_btn = ui
                        .button("📂 Open Samples Folder")
                        .on_hover_text("Open output directory");
                    if open_btn.clicked() {
                        let output_dir = samples_output_dir(state);
                        if let Err(err) = open_directory(&output_dir) {
                            state.add_log(
                                LogLevel::Warning,
                                format!(
                                    "Could not open output directory '{}': {}",
                                    output_dir.display(),
                                    err
                                ),
                            );
                        }
                    }
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

    // Progress bar
    if state.progress.total_samples > 0 {
        ui.group(|ui| {
            ui.label(egui::RichText::new("Progress").strong().size(16.0));
            ui.add_space(5.0);

            let denom = state.progress.total_samples.max(1) as f32;
            let fraction = state.progress.current_index as f32 / denom;

            ui.add(
                egui::ProgressBar::new(fraction).text(format!(
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
                ui.label(format!(
                    "✅ Completed: {}",
                    state.progress.samples_completed
                ));
                ui.label(format!(
                    "⏭ Skipped: {}",
                    state.progress.samples_skipped
                ));
                ui.label(format!(
                    "❌ Failed: {}",
                    state.progress.samples_failed
                ));
            });
        });

        ui.add_space(10.0);
    }

    // Log panel
    const LOG_PANEL_MAX_HEIGHT: f32 = 280.0;
    const LOG_PANEL_MIN_HEIGHT: f32 = 140.0;
    const LOG_CHROME_HEIGHT: f32 = 56.0;

    let log_panel_height = ui
        .available_height()
        .clamp(LOG_PANEL_MIN_HEIGHT, LOG_PANEL_MAX_HEIGHT);

    ui.allocate_ui_with_layout(
        egui::vec2(ui.available_width(), log_panel_height),
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
                    .max_height((log_panel_height - LOG_CHROME_HEIGHT).max(48.0))
                    .stick_to_bottom(true)
                    .show(ui, |ui| {
                        for log in &state.logs {
                            use autosample_core::LogLevel;
                            let color = match log.level {
                                LogLevel::Info => egui::Color32::LIGHT_GRAY,
                                LogLevel::Warning => egui::Color32::YELLOW,
                                LogLevel::Error => egui::Color32::RED,
                            };
                            ui.horizontal(|ui| {
                                ui.label(
                                    egui::RichText::new(&log.timestamp)
                                        .color(egui::Color32::GRAY),
                                );
                                ui.label(
                                    egui::RichText::new(&log.message).color(color),
                                );
                            });
                        }
                    });
            });
        },
    );

    cmd
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

fn samples_output_dir(state: &AppState) -> PathBuf {
    let output = PathBuf::from(state.config.output.trim());
    let prefix = state.config.prefix.trim();
    if prefix.is_empty() {
        output
    } else {
        output.join(prefix)
    }
}

fn open_directory(path: &PathBuf) -> Result<(), String> {
    if !path.exists() {
        return Err("Directory does not exist yet".to_string());
    }

    #[cfg(target_os = "macos")]
    let mut command = {
        let mut cmd = Command::new("open");
        cmd.arg(path);
        cmd
    };

    #[cfg(target_os = "windows")]
    let mut command = {
        let mut cmd = Command::new("explorer");
        cmd.arg(path);
        cmd
    };

    #[cfg(all(unix, not(target_os = "macos")))]
    let mut command = {
        let mut cmd = Command::new("xdg-open");
        cmd.arg(path);
        cmd
    };

    command
        .spawn()
        .map(|_| ())
        .map_err(|e| format!("Failed to launch file browser: {}", e))
}