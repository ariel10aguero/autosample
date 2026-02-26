// src/ui/session.rs
use crate::state::AppState;
use autosample_core::OutputOrganization;
use eframe::egui;

pub fn show(ui: &mut egui::Ui, state: &mut AppState) {
    // NOTE: no ScrollArea here (sidebar owns scrolling)

    ui.group(|ui| {
        ui.label(egui::RichText::new("Note Range").strong().size(16.0));
        ui.add_space(5.0);

        ui.label(egui::RichText::new("Range: C2..C6 | List: C4,E4,G4 | Single: A4").weak());
        ui.text_edit_singleline(&mut state.config.notes);
    });

    ui.add_space(10.0);

    ui.group(|ui| {
        ui.label(egui::RichText::new("Velocity Layers").strong().size(16.0));
        ui.add_space(5.0);

        ui.label(
            egui::RichText::new("Step: 127..15:16 | List: 127,100,64 | Single: 100").weak(),
        );
        ui.text_edit_singleline(&mut state.config.vel);
    });

    ui.add_space(10.0);

    ui.group(|ui| {
        ui.label(egui::RichText::new("Timing").strong().size(16.0));
        ui.add_space(5.0);

        egui::Grid::new("timing_grid")
            .num_columns(2)
            .spacing([8.0, 6.0])
            .show(ui, |ui| {
                ui.label("Preroll:");
                ui.add(egui::DragValue::new(&mut state.config.preroll_ms).suffix(" ms"));
                ui.end_row();

                ui.label("Hold:");
                ui.add(egui::DragValue::new(&mut state.config.hold_ms).suffix(" ms"));
                ui.end_row();

                ui.label("Tail:");
                ui.add(egui::DragValue::new(&mut state.config.tail_ms).suffix(" ms"));
                ui.end_row();
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

}

pub fn show_output(ui: &mut egui::Ui, state: &mut AppState) {
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

        let name_preview = if state.preset_name.trim().is_empty() {
            "Untitled"
        } else {
            state.preset_name.as_str()
        };
        ui.label(
            egui::RichText::new(format!("File name prefix: {}", name_preview))
                .weak()
                .monospace(),
        );

        ui.label("Organization:");
        ui.horizontal(|ui| {
            ui.radio_value(
                &mut state.config.output_organization,
                OutputOrganization::Flat,
                "Flat",
            );
            ui.radio_value(
                &mut state.config.output_organization,
                OutputOrganization::ByNote,
                "By Note",
            );
        });

        let path_preview = match state.config.output_organization {
            OutputOrganization::Flat => "output/<preset_name>/<file>.wav",
            OutputOrganization::ByNote => "output/<preset_name>/<NoteName_Midi>/<file>.wav",
        };
        ui.label(egui::RichText::new(path_preview).weak().monospace());

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
