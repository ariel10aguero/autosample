// src/ui/devices.rs
use crate::state::AppState;
use autosample_core::EngineStatus;
use eframe::egui;

pub fn show(ui: &mut egui::Ui, state: &mut AppState) {
    // NOTE: no ScrollArea here (sidebar owns scrolling)
    let refresh_disabled =
        state.engine_status == EngineStatus::Running || state.is_device_scan_running();

    ui.group(|ui| {
        ui.label(egui::RichText::new("MIDI Output").strong().size(16.0));
        ui.add_space(6.0);

        ui.horizontal(|ui| {
            ui.label("Device:");

            egui::ComboBox::from_id_salt("midi_device_combo")
                .width(ui.available_width() - 40.0)
                .selected_text(
                    state
                        .selected_midi_idx
                        .and_then(|idx| state.midi_devices.get(idx))
                        .map(|d| d.name.as_str())
                        .unwrap_or("Select…"),
                )
                .show_ui(ui, |ui| {
                    for (idx, device) in state.midi_devices.iter().enumerate() {
                        ui.selectable_value(&mut state.selected_midi_idx, Some(idx), &device.name);
                    }
                });

            let refresh = ui
                .add_enabled(!refresh_disabled, egui::Button::new("🔄"))
                .on_hover_text("Refresh MIDI and audio devices");
            if refresh.clicked() {
                state.request_device_scan();
            }
        });

        // Update config
        if let Some(idx) = state.selected_midi_idx {
            if let Some(device) = state.midi_devices.get(idx) {
                state.config.midi_out = device.name.clone();
            }
        }
    });

    ui.add_space(10.0);

    ui.group(|ui| {
        ui.label(egui::RichText::new("Audio Input").strong().size(16.0));
        ui.add_space(6.0);

        ui.horizontal(|ui| {
            ui.label("Device:");

            egui::ComboBox::from_id_salt("audio_device_combo")
                .width(ui.available_width() - 40.0)
                .selected_text(
                    state
                        .selected_audio_idx
                        .and_then(|idx| state.audio_devices.get(idx))
                        .map(|d| format!("{} ({} Hz, {} ch)", d.name, d.sample_rate, d.channels))
                        .unwrap_or_else(|| "Select…".to_string()),
                )
                .show_ui(ui, |ui| {
                    for (idx, device) in state.audio_devices.iter().enumerate() {
                        let label = format!(
                            "{} ({} Hz, {} ch)",
                            device.name, device.sample_rate, device.channels
                        );
                        ui.selectable_value(&mut state.selected_audio_idx, Some(idx), label);
                    }
                });

            let refresh = ui
                .add_enabled(!refresh_disabled, egui::Button::new("🔄"))
                .on_hover_text("Refresh MIDI and audio devices");
            if refresh.clicked() {
                state.request_device_scan();
            }
        });

        // Update config
        if let Some(idx) = state.selected_audio_idx {
            if let Some(device) = state.audio_devices.get(idx) {
                state.config.audio_in = device.name.clone();
            }
        }
    });

    ui.add_space(10.0);

    if state.engine_status == EngineStatus::Running {
        ui.label(
            egui::RichText::new("Refresh unavailable while running")
                .color(egui::Color32::GRAY),
        );
    } else if state.is_device_scan_running() {
        ui.label(egui::RichText::new("Scanning devices...").color(egui::Color32::LIGHT_BLUE));
    } else if let Some(error) = state.device_scan_error() {
        ui.label(
            egui::RichText::new(format!("Device refresh failed: {}", error))
                .color(egui::Color32::YELLOW),
        );
    }

    ui.group(|ui| {
        ui.label(egui::RichText::new("Audio Settings").strong().size(16.0));
        ui.add_space(6.0);

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
}
