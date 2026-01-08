use anyhow::{Context, Result};
use midir::{MidiOutput, MidiOutputConnection, MidiOutputPort};
use std::thread;
use std::time::Duration;
use tracing::{info, warn};

pub fn list_midi_outputs() -> Result<()> {
    let midi_out = MidiOutput::new("autosample-lister")?;
    let ports = midi_out.ports();

    println!("\nAvailable MIDI Output Ports:");
    println!("{:-<60}", "");

    if ports.is_empty() {
        println!("No MIDI output ports found.");
        return Ok(());
    }

    for (i, port) in ports.iter().enumerate() {
        let name = midi_out
            .port_name(port)
            .unwrap_or_else(|_| "Unknown".to_string());
        println!("{:3}: {}", i, name);
    }
    println!();

    Ok(())
}

pub fn find_midi_output(identifier: &str) -> Result<MidiOutputPort> {
    let midi_out = MidiOutput::new("autosample-finder")?;
    let ports = midi_out.ports();

    // Try as index first
    if let Ok(index) = identifier.parse::<usize>() {
        if index < ports.len() {
            return Ok(ports[index].clone());
        }
    }

    // Try as name (case-insensitive substring match)
    let identifier_lower = identifier.to_lowercase();
    for port in &ports {
        if let Ok(name) = midi_out.port_name(port) {
            if name.to_lowercase().contains(&identifier_lower) {
                return Ok(port.clone());
            }
        }
    }

    anyhow::bail!("MIDI output port not found: {}", identifier)
}

pub struct MidiController {
    connection: MidiOutputConnection,
}

impl MidiController {
    pub fn new(port: MidiOutputPort) -> Result<Self> {
        let midi_out = MidiOutput::new("autosample")?;
        let port_name = midi_out
            .port_name(&port)
            .unwrap_or_else(|_| "Unknown".to_string());
        
        info!("Connecting to MIDI output: {}", port_name);
        
        let connection = midi_out
            .connect(&port, "autosample")
            .context("Failed to connect to MIDI output")?;

        Ok(Self { connection })
    }

    pub fn note_on(&mut self, note: u8, velocity: u8) -> Result<()> {
        let msg = [0x90, note, velocity];
        self.connection
            .send(&msg)
            .context("Failed to send NoteOn")?;
        Ok(())
    }

    pub fn note_off(&mut self, note: u8) -> Result<()> {
        let msg = [0x80, note, 0];
        self.connection
            .send(&msg)
            .context("Failed to send NoteOff")?;
        Ok(())
    }

    pub fn all_notes_off(&mut self) -> Result<()> {
        // Send All Notes Off (CC 123) on all channels
        for channel in 0..16 {
            let msg = [0xB0 | channel, 123, 0];
            let _ = self.connection.send(&msg);
        }
        thread::sleep(Duration::from_millis(50));
        Ok(())
    }
}

impl Drop for MidiController {
    fn drop(&mut self) {
        let _ = self.all_notes_off();
    }
}
