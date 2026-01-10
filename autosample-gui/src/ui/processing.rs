use crate::state::AppState;
use eframe::egui;

pub fn show(ui: &mut egui::Ui, state: &mut AppState) {
    ui.heading("⚙ Processing Options");
    ui.add_space(10.0);

    egui::ScrollArea::vertical().show(ui, |ui| {
        // Trimming
        ui.group(|ui| {
            ui.label(egui::RichText::new("Silence Trimming").strong().size(16.0));
            ui.add_space(5.0);

            let mut trim_enabled = state.config.trim_threshold_db.is_some();
            ui.checkbox(&mut trim_enabled, "Enable auto-trim");

            if trim_enabled {
                let mut threshold = state.config.trim_threshold_db.unwrap_or(-50.0);
                ui.horizontal(|ui| {
                    ui.label("Threshold:");
                    ui.add(egui::DragValue::new(&mut threshold).suffix(" dB").speed(0.5));
                });
                state.config.trim_threshold_db = Some(threshold);
            } else {
                state.config.trim_threshold_db = None;
            }
        });

        ui.add_space(10.0);

        // Normalization
        ui.group(|ui| {
            ui.label(egui::RichText::new("Normalization").strong().size(16.0));
            ui.add_space(5.0);

            let mut norm_enabled = state.config.normalize.is_some();
            ui.checkbox(&mut norm_enabled, "Enable normalization");

            if norm_enabled {
                let mut mode = state.config.normalize.clone().unwrap_or_else(|| "peak".to_string());
                ui.horizontal(|ui| {
                    ui.label("Mode:");
                    egui::ComboBox::from_id_salt("norm_mode")
                        .selected_text(&mode)
                        .show_ui(ui, |ui| {
                            ui.selectable_value(&mut mode, "peak".to_string(), "Peak (0 dBFS)");
                            ui.selectable_value(&mut mode, "-1dB".to_string(), "-1 dBFS");
                        });
                });
                state.config.normalize = Some(mode);
            } else {
                state.config.normalize = None;
            }
        });

        ui.add_space(10.0);

        // Fades (always applied, informational)
        ui.group(|ui| {
            ui.label(egui::RichText::new("Fades").strong().size(16.0));
            ui.add_space(5.0);
            ui.label("5ms fade in/out applied automatically to prevent clicks");
        });
    });
}