use crate::state::AppState;
use eframe::egui;

pub fn show(ui: &mut egui::Ui, state: &mut AppState) {
    ui.heading("🎹 Session Configuration");
    ui.add_space(10.0);

    egui::ScrollArea::vertical().show(ui, |ui| {
        // Notes
        ui.group(|ui| {
            ui.label(egui::RichText::new("Note Range").strong().size(16.0));
            ui.add_space(5.0);

            ui.label("Examples: C2..C6, C4,E4,G4, A4");
            ui.text_edit_singleline(&mut state.config.notes);
        });

        ui.add_space(10.0);

        // Velocities
        ui.group(|ui| {
            ui.label(egui::RichText::new("Velocity Layers").strong().size(16.0));
            ui.add_space(5.0);

            ui.label("Examples: 127..1:16, 127,100,64, 127");
            ui.text_edit_singleline(&mut state.config.vel);
        });

        ui.add_space(10.0);

        // Timing
        ui.group(|ui| {
            ui.label(egui::RichText::new("Timing").strong().size(16.0));
            ui.add_space(5.0);

            ui.horizontal(|ui| {
                ui.label("Preroll:");
                ui.add(egui::DragValue::new(&mut state.config.preroll_ms).suffix(" ms"));
            });

            ui.horizontal(|ui| {
                ui.label("Hold Duration:");
                ui.add(egui::DragValue::new(&mut state.config.hold_ms).suffix(" ms"));
            });

            ui.horizontal(|ui| {
                ui.label("Tail Duration:");
                ui.add(egui::DragValue::new(&mut state.config.tail_ms).suffix(" ms"));
            });
        });

        ui.add_space(10.0);

        // Round Robin
        ui.group(|ui| {
            ui.label(egui::RichText::new("Round Robin").strong().size(16.0));
            ui.add_space(5.0);

            ui.horizontal(|ui| {
                ui.label("Takes per note/velocity:");
                ui.add(
                    egui::DragValue::new(&mut state.config.round_robin).clamp_range(1..=10),
                );
            });
        });

        ui.add_space(10.0);

        // Output
        ui.group(|ui| {
            ui.label(egui::RichText::new("Output").strong().size(16.0));
            ui.add_space(5.0);

            ui.horizontal(|ui| {
                ui.label("Output Directory:");
                ui.text_edit_singleline(&mut state.config.output);
                if ui.button("📁").clicked() {
                    // TODO: Directory picker
                }
            });

            ui.horizontal(|ui| {
                ui.label("Prefix:");
                ui.text_edit_singleline(&mut state.config.prefix);
            });

            ui.horizontal(|ui| {
                ui.label("Format:");
                egui::ComboBox::from_id_salt("format")
                    .selected_text(&state.config.format)
                    .show_ui(ui, |ui| {
                        ui.selectable_value(
                            &mut state.config.format,
                            "wav".to_string(),
                            "WAV",
                        );
                        ui.selectable_value(
                            &mut state.config.format,
                            "mp3".to_string(),
                            "MP3",
                        );
                        ui.selectable_value(
                            &mut state.config.format,
                            "both".to_string(),
                            "Both",
                        );
                    });
            });

            ui.checkbox(&mut state.config.resume, "Resume (skip existing files)");
        });
    });
}