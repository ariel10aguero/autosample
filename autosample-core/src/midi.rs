use crate::types::MidiPortInfo;
use anyhow::Result;
use midir::{MidiOutput, MidiOutputConnection, MidiOutputPort};
use std::thread;
use std::time::Duration;
use tracing::info;

const MIDI_INIT_ATTEMPTS: usize = 3;
const MIDI_INIT_BACKOFF_MS: [u64; MIDI_INIT_ATTEMPTS] = [100, 250, 500];

fn new_midi_output_with_retry(client_name: &str, context: &str) -> Result<MidiOutput> {
    let mut last_error = None;

    for (attempt_idx, backoff_ms) in MIDI_INIT_BACKOFF_MS.iter().enumerate() {
        match MidiOutput::new(client_name) {
            Ok(midi_out) => return Ok(midi_out),
            Err(err) => {
                last_error = Some(err);

                if attempt_idx + 1 < MIDI_INIT_ATTEMPTS {
                    thread::sleep(Duration::from_millis(*backoff_ms));
                }
            }
        }
    }

    match last_error {
        Some(err) => anyhow::bail!(
            "MIDI support could not be initialized {} after {} attempts: {:?}",
            context,
            MIDI_INIT_ATTEMPTS,
            err
        ),
        None => anyhow::bail!(
            "MIDI support could not be initialized {}: unknown error",
            context
        ),
    }
}

fn find_port_in_output(midi_out: &MidiOutput, name_or_id: &str) -> Result<MidiOutputPort> {
    let ports = midi_out.ports();

    if let Ok(idx) = name_or_id.parse::<usize>() {
        if let Some(port) = ports.get(idx) {
            return Ok(port.clone());
        }
    }

    for port in &ports {
        if let Ok(port_name) = midi_out.port_name(port) {
            if port_name.contains(name_or_id) {
                return Ok(port.clone());
            }
        }
    }

    let available_ports = ports
        .iter()
        .enumerate()
        .map(|(idx, port)| {
            let name = midi_out
                .port_name(port)
                .unwrap_or_else(|_| "Unknown".to_string());
            format!("[{}] {}", idx, name)
        })
        .collect::<Vec<_>>();

    anyhow::bail!(
        "MIDI port not found: '{}'. Available ports: {}",
        name_or_id,
        if available_ports.is_empty() {
            "(none)".to_string()
        } else {
            available_ports.join(", ")
        }
    )
}

pub fn list_midi_ports() -> Result<()> {
    let midi_out = new_midi_output_with_retry("autosample-list", "while listing MIDI outputs")?;
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
    let midi_out = new_midi_output_with_retry("autosample-list", "while enumerating MIDI outputs")?;
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

pub fn collect_midi_diagnostics(requested_target: &str) -> Vec<String> {
    let mut lines = vec![
        format!("MIDI diagnostics target: '{}'", requested_target),
        format!("MIDI diagnostics platform: {}", std::env::consts::OS),
    ];

    match new_midi_output_with_retry("autosample-diag", "during diagnostics") {
        Ok(midi_out) => {
            let ports = midi_out.ports();
            lines.push(format!("MIDI diagnostics ports detected: {}", ports.len()));

            if ports.is_empty() {
                lines.push(
                    "MIDI diagnostics warning: backend reports zero MIDI output ports".to_string(),
                );
            } else {
                for (idx, port) in ports.iter().enumerate() {
                    let name = midi_out
                        .port_name(port)
                        .unwrap_or_else(|_| "Unknown".to_string());
                    lines.push(format!("MIDI diagnostics port [{}]: {}", idx, name));
                }
            }
        }
        Err(err) => {
            lines.push(format!(
                "MIDI diagnostics error: backend init failed: {}",
                err
            ));
        }
    }

    lines
}

pub fn find_midi_port(name_or_id: &str) -> Result<MidiOutputPort> {
    let midi_out = new_midi_output_with_retry(
        "autosample-find",
        &format!("while resolving output '{}'", name_or_id),
    )?;
    find_port_in_output(&midi_out, name_or_id)
}

pub fn connect_midi_output(port: MidiOutputPort) -> Result<MidiOutputConnection> {
    let midi_out =
        new_midi_output_with_retry("autosample", "for output connection")?;
    let port_name = midi_out
        .port_name(&port)
        .unwrap_or_else(|_| "Unknown".to_string());

    info!("Connecting to MIDI port: {}", port_name);

    let connection = midi_out
        .connect(&port, "autosample")
        .map_err(|e| anyhow::anyhow!("Failed to connect to MIDI output: {:?}", e))?;

    Ok(connection)
}

pub fn connect_midi_output_by_name(
    name_or_id: &str,
) -> Result<(MidiOutputConnection, String, Vec<String>)> {
    let midi_out = new_midi_output_with_retry(
        "autosample-connect",
        &format!("while connecting output '{}'", name_or_id),
    )?;
    let ports = midi_out.ports();

    let available_ports = ports
        .iter()
        .enumerate()
        .map(|(idx, port)| {
            let name = midi_out
                .port_name(port)
                .unwrap_or_else(|_| "Unknown".to_string());
            format!("[{}] {}", idx, name)
        })
        .collect::<Vec<_>>();

    let selected_port = find_port_in_output(&midi_out, name_or_id)?;
    let selected_name = midi_out
        .port_name(&selected_port)
        .unwrap_or_else(|_| "Unknown".to_string());

    info!("Connecting to MIDI port: {}", selected_name);

    let connection = midi_out
        .connect(&selected_port, "autosample")
        .map_err(|e| anyhow::anyhow!("Failed to connect to MIDI output: {:?}", e))?;

    Ok((connection, selected_name, available_ports))
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