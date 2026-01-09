use crate::types::MidiPortInfo;
use anyhow::Result;
use midir::{MidiOutput, MidiOutputConnection, MidiOutputPort};
use std::thread;
use std::time::Duration;
use tracing::info;

pub fn list_midi_ports() -> Result<()> {
    let midi_out = MidiOutput::new("autosample-list")?;
    let ports = midi_out.ports();

    println!("\nAvailable MIDI Output Ports:");
    println!("{}", "-".repeat(60));
    
    for (idx, port) in ports.iter().enumerate() {
        let name = midi_out
            .port_name(port)
            .unwrap_or_else(|_| "Unknown".to_string());
        println!("  {}: {}", idx, name);
    }

    if ports.is_empty() {
        println!("  (none found)");
    }
    
    println!();
    Ok(())
}

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

pub fn find_midi_port(name_or_id: &str) -> Result<MidiOutputPort> {
    let midi_out = MidiOutput::new("autosample-find")?;
    let ports = midi_out.ports();

    // Try parsing as index
    if let Ok(idx) = name_or_id.parse::<usize>() {
        if let Some(port) = ports.get(idx) {
            return Ok(port.clone());
        }
    }

    // Try finding by name
    for port in &ports {
        if let Ok(port_name) = midi_out.port_name(port) {
            if port_name.contains(name_or_id) {
                return Ok(port.clone());
            }
        }
    }

    anyhow::bail!("MIDI port not found: {}", name_or_id)
}

pub fn connect_midi_output(port: MidiOutputPort) -> Result<MidiOutputConnection> {
    let midi_out = MidiOutput::new("autosample")?;
    let port_name = midi_out
        .port_name(&port)
        .unwrap_or_else(|_| "Unknown".to_string());

    info!("Connecting to MIDI port: {}", port_name);

    let connection = midi_out
        .connect(&port, "autosample")
        .map_err(|e| anyhow::anyhow!("Failed to connect to MIDI output: {:?}", e))?;

    Ok(connection)
}

pub fn send_note_on(conn: &mut MidiOutputConnection, note: u8, velocity: u8) -> Result<()> {
    let msg = [0x90, note, velocity]; // Note On, channel 0
    conn.send(&msg)?;
    Ok(())
}

pub fn send_note_off(conn: &mut MidiOutputConnection, note: u8) -> Result<()> {
    let msg = [0x80, note, 0]; // Note Off, channel 0
    conn.send(&msg)?;
    Ok(())
}

pub fn send_all_notes_off(conn: &mut MidiOutputConnection) -> Result<()> {
    for note in 0..128 {
        let _ = send_note_off(conn, note);
    }
    thread::sleep(Duration::from_millis(10));
    Ok(())
}