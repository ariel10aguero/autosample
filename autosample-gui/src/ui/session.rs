// src/ui/session.rs
use crate::state::AppState;
use eframe::egui;

pub fn show(ui: &mut egui::Ui, state: &mut AppState) {
    // NOTE: no ScrollArea here (sidebar owns scrolling)

    ui.group(|ui| {
        ui.label(egui::RichText::new("Note Range").strong().size(16.0));
        ui.add_space(5.0);

        ui.label(egui::RichText::new("Examples: C2..C6, C4,E4,G4, A4").weak());
        ui.text_edit_singleline(&mut state.config.notes);
    });

    ui.add_space(10.0);

    ui.group(|ui| {
        ui.label(egui::RichText::new("Velocity Layers").strong().size(16.0));
        ui.add_space(5.0);

        ui.label(egui::RichText::new("Examples: 127..1:16, 127,100,64").weak());
        ui.text_edit_singleline(&mut state.config.vel);
    });

    ui.add_space(10.0);

    ui.group(|ui| {
        ui.label(egui::RichText::new("Timing").strong().size(16.0));
        ui.add_space(5.0);

        ui.horizontal(|ui| {
            ui.label("Preroll:");
            ui.add(egui::DragValue::new(&mut state.config.preroll_ms).suffix(" ms"));
        });

        ui.horizontal(|ui| {
            ui.label("Hold:");
            ui.add(egui::DragValue::new(&mut state.config.hold_ms).suffix(" ms"));
        });

        ui.horizontal(|ui| {
            ui.label("Tail:");
            ui.add(egui::DragValue::new(&mut state.config.tail_ms).suffix(" ms"));
        });
    });

    ui.add_space(10.0);

    ui.group(|ui| {
        ui.label(egui::RichText::new("Round Robin").strong().size(16.0));
        ui.add_space(5.0);

        ui.horizontal(|ui| {
            ui.label("Takes:");
            ui.add(egui::DragValue::new(&mut state.config.round_robin).range(1..=10));
        });
    });

    ui.add_space(10.0);

    ui.group(|ui| {
        ui.label(egui::RichText::new("Output").strong().size(16.0));
        ui.add_space(5.0);

        ui.horizontal(|ui| {
            ui.label("Directory:");
            ui.text_edit_singleline(&mut state.config.output);
            if ui.button("📁").on_hover_text("Choose output directory").clicked() {
                if let Some(path) = rfd::FileDialog::new().pick_folder() {
                    state.config.output = path.display().to_string();
                }
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
                    ui.selectable_value(&mut state.config.format, "wav".to_string(), "WAV");
                    ui.selectable_value(&mut state.config.format, "mp3".to_string(), "MP3");
                    ui.selectable_value(&mut state.config.format, "both".to_string(), "Both");
                });
        });

        ui.checkbox(&mut state.config.resume, "Resume (skip existing files)");
    });
}
