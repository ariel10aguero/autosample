use crate::types::MidiPortInfo;

pub fn get_midi_ports() -> Result<Vec<MidiPortInfo>> {
    let midi_out = MidiOutput::new("autosample-list")?;
    let ports = midi_out.ports();
    let mut result = Vec::new();

    for (idx, port) in ports.iter().enumerate() {
        let name = midi_out
            .port_name(port)
            .unwrap_or_else(|_| "Unknown".to_string());
        
        result.push(MidiPortInfo { index: idx, name });
    }

    Ok(result)
}