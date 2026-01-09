use crate::state::AppState;
use eframe::egui;

pub fn show(ui: &mut egui::Ui, state: &mut AppState) {
    ui.heading("🔌 Device Configuration");
    ui.add_space(10.0);

    egui::ScrollArea::vertical().show(ui, |ui| {
        // MIDI Output
        ui.group(|ui| {
            ui.label(egui::RichText::new("MIDI Output").strong().size(16.0));
            ui.add_space(5.0);

            egui::ComboBox::from_label("MIDI Device")
                .selected_text(
                    state
                        .selected_midi_idx
                        .and_then(|idx| state.midi_devices.get(idx))
                        .map(|d| d.name.as_str())
                        .unwrap_or("Select device..."),
                )
                .show_ui(ui, |ui| {
                    for (idx, device) in state.midi_devices.iter().enumerate() {
                        ui.selectable_value(&mut state.selected_midi_idx, Some(idx), &device.name);
                    }
                });

            if ui.button("🔄 Refresh MIDI Devices").clicked() {
                if let Ok(devices) = autosample_core::midi::get_midi_ports() {
                    state.midi_devices = devices;
                }
            }

            // Update config
            if let Some(idx) = state.selected_midi_idx {
                if let Some(device) = state.midi_devices.get(idx) {
                    state.config.midi_out = device.name.clone();
                }
            }
        });

        ui.add_space(10.0);

        // Audio Input
        ui.group(|ui| {
            ui.label(egui::RichText::new("Audio Input").strong().size(16.0));
            ui.add_space(5.0);

            egui::ComboBox::from_label("Audio Device")
                .selected_text(
                    state
                        .selected_audio_idx
                        .and_then(|idx| state.audio_devices.get(idx))
                        .map(|d| format!("{} ({} Hz, {} ch)", d.name, d.sample_rate, d.channels))
                        .unwrap_or_else(|| "Select device...".to_string()),
                )
                .show_ui(ui, |ui| {
                    for (idx, device) in state.audio_devices.iter().enumerate() {
                        let label = format!("{} ({} Hz, {} ch)", device.name, device.sample_rate, device.channels);
                        ui.selectable_value(&mut state.selected_audio_idx, Some(idx), label);
                    }
                });

            if ui.button("🔄 Refresh Audio Devices").clicked() {
                if let Ok(devices) = autosample_core::audio::get_audio_devices() {
                    state.audio_devices = devices;
                }
            }

            // Update config
            if let Some(idx) = state.selected_audio_idx {
                if let Some(device) = state.audio_devices.get(idx) {
                    state.config.audio_in = device.name.clone();
                }
            }
        });

        ui.add_space(10.0);

        // Audio Settings
        ui.group(|ui| {
            ui.label(egui::RichText::new("Audio Settings").strong().size(16.0));
            ui.add_space(5.0);

            ui.horizontal(|ui| {
                ui.label("Sample Rate:");
                egui::ComboBox::from_id_salt("sample_rate")
                    .selected_text(format!("{} Hz", state.config.sr))
                    .show_ui(ui, |ui| {
                        for &sr in &[44100, 48000, 88200, 96000, 192000] {
                            ui.selectable_value(&mut state.config.sr, sr, format!("{} Hz", sr));
                        }
                    });
            });

            ui.horizontal(|ui| {
                ui.label("Channels:");
                ui.radio_value(&mut state.config.channels, 1, "Mono");
                ui.radio_value(&mut state.config.channels, 2, "Stereo");
            });
        });
    });
}