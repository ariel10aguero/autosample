use crate::types::MidiPortInfo;
use anyhow::Result;
use midir::{MidiOutput, MidiOutputConnection, MidiOutputPort};
use std::sync::{Mutex, OnceLock};
use std::thread;
use std::time::{Duration, Instant};
use tracing::{info, warn};

// ---------------------------------------------------------------------------
// Retry / backoff configuration
// ---------------------------------------------------------------------------

const MIDI_INIT_ATTEMPTS: usize = 10;
const MIDI_INIT_BACKOFF_MS: [u64; MIDI_INIT_ATTEMPTS] =
    [50, 100, 200, 350, 500, 700, 900, 1200, 1500, 1800];

// ---------------------------------------------------------------------------
// Global warmup client
//
// Holding one long-lived MidiOutput keeps the CoreMIDI session open so that
// subsequent new() / connect() calls do not have to wait for the macOS daemon
// to fully restart.  We only need it alive; we never send through it.
// ---------------------------------------------------------------------------

struct WarmupHolder {
    _client: MidiOutput,
}

// SAFETY: MidiOutput is not Send, but we only ever touch it from behind the
// Mutex, so the Mutex itself makes access sequential.
unsafe impl Send for WarmupHolder {}

static WARMUP_CLIENT: OnceLock<Mutex<Option<WarmupHolder>>> = OnceLock::new();

/// Call once (e.g. in App::new) to pre-warm the CoreMIDI session.
/// Subsequent calls are no-ops.
pub fn ensure_midi_warmup() {
    let cell = WARMUP_CLIENT.get_or_init(|| Mutex::new(None));
    let mut guard = cell.lock().unwrap();
    if guard.is_some() {
        return; // already warmed up
    }
    match MidiOutput::new("autosample-warmup") {
        Ok(client) => {
            info!("MIDI warmup client created; CoreMIDI session is now live.");
            *guard = Some(WarmupHolder { _client: client });
        }
        Err(err) => {
            warn!("MIDI warmup client could not be created: {:?}", err);
        }
    }
}

// ---------------------------------------------------------------------------
// Port-list cache
//
// Enumerating ports is cheap but every enumeration that creates-then-drops a
// MidiOutput stresses the CoreMIDI daemon.  We cache the result for a short
// window so that the startup scan and the start_session() call that follows
// immediately after can share one enumeration.
// ---------------------------------------------------------------------------

const PORT_CACHE_TTL: Duration = Duration::from_secs(3);

struct PortCache {
    ports: Vec<MidiPortInfo>,
    captured_at: Instant,
}

static PORT_CACHE: OnceLock<Mutex<Option<PortCache>>> = OnceLock::new();

fn port_cache_cell() -> &'static Mutex<Option<PortCache>> {
    PORT_CACHE.get_or_init(|| Mutex::new(None))
}

fn cached_ports() -> Option<Vec<MidiPortInfo>> {
    let guard = port_cache_cell().lock().unwrap();
    guard.as_ref().and_then(|c| {
        if c.captured_at.elapsed() < PORT_CACHE_TTL {
            Some(c.ports.clone())
        } else {
            None
        }
    })
}

fn store_port_cache(ports: Vec<MidiPortInfo>) {
    let mut guard = port_cache_cell().lock().unwrap();
    *guard = Some(PortCache {
        ports,
        captured_at: Instant::now(),
    });
}

pub fn invalidate_port_cache() {
    let mut guard = port_cache_cell().lock().unwrap();
    *guard = None;
}

// ---------------------------------------------------------------------------
// CoreMIDI init helpers
// ---------------------------------------------------------------------------

fn new_midi_output_with_retry(client_name: &str, context: &str) -> Result<MidiOutput> {
    let mut last_error = None;

    for (attempt_idx, &backoff_ms) in MIDI_INIT_BACKOFF_MS.iter().enumerate() {
        match MidiOutput::new(client_name) {
            Ok(midi_out) => {
                if attempt_idx > 0 {
                    info!(
                        "MIDI client created after {} attempt(s) ({} ms backoff total).",
                        attempt_idx + 1,
                        MIDI_INIT_BACKOFF_MS[..attempt_idx].iter().sum::<u64>()
                    );
                }
                return Ok(midi_out);
            }
            Err(err) => {
                warn!(
                    "MIDI init attempt {}/{} failed {}: {:?}",
                    attempt_idx + 1,
                    MIDI_INIT_ATTEMPTS,
                    context,
                    err
                );
                last_error = Some(err);

                if attempt_idx + 1 < MIDI_INIT_ATTEMPTS {
                    thread::sleep(Duration::from_millis(backoff_ms));
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

// ---------------------------------------------------------------------------
// Port resolution helpers
// ---------------------------------------------------------------------------

fn enumerate_ports(midi_out: &MidiOutput) -> Vec<MidiPortInfo> {
    midi_out
        .ports()
        .iter()
        .enumerate()
        .map(|(idx, port)| {
            let name = midi_out
                .port_name(port)
                .unwrap_or_else(|_| "Unknown".to_string());
            MidiPortInfo { index: idx, name }
        })
        .collect()
}

fn find_port_in_output(midi_out: &MidiOutput, name_or_id: &str) -> Result<MidiOutputPort> {
    let ports = midi_out.ports();

    // Try numeric index first
    if let Ok(idx) = name_or_id.parse::<usize>() {
        if let Some(port) = ports.get(idx) {
            return Ok(port.clone());
        }
    }

    // Then substring match on name
    for port in &ports {
        if let Ok(port_name) = midi_out.port_name(port) {
            if port_name.contains(name_or_id) {
                return Ok(port.clone());
            }
        }
    }

    let available_ports: Vec<String> = ports
        .iter()
        .enumerate()
        .map(|(idx, port)| {
            let name = midi_out
                .port_name(port)
                .unwrap_or_else(|_| "Unknown".to_string());
            format!("[{}] {}", idx, name)
        })
        .collect();

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

// ---------------------------------------------------------------------------
// Public API
// ---------------------------------------------------------------------------

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

/// Returns the current MIDI port list, using the short-lived cache when
/// available so that a startup scan followed immediately by start_session()
/// does not create two separate CoreMIDI clients.
pub fn get_midi_ports() -> Result<Vec<MidiPortInfo>> {
    if let Some(cached) = cached_ports() {
        info!("get_midi_ports: returning cached port list ({} ports).", cached.len());
        return Ok(cached);
    }

    let midi_out = new_midi_output_with_retry("autosample-list", "while enumerating MIDI outputs")?;
    let result = enumerate_ports(&midi_out);
    store_port_cache(result.clone());
    Ok(result)
}

pub fn collect_midi_diagnostics(requested_target: &str) -> Vec<String> {
    let mut lines = vec![
        format!("MIDI diagnostics target: '{}'", requested_target),
        format!("MIDI diagnostics platform: {}", std::env::consts::OS),
    ];

    match new_midi_output_with_retry("autosample-diag", "during diagnostics") {
        Ok(midi_out) => {
            let ports = enumerate_ports(&midi_out);
            lines.push(format!("MIDI diagnostics ports detected: {}", ports.len()));

            if ports.is_empty() {
                lines.push(
                    "MIDI diagnostics warning: backend reports zero MIDI output ports".to_string(),
                );
            } else {
                for info in &ports {
                    lines.push(format!(
                        "MIDI diagnostics port [{}]: {}",
                        info.index, info.name
                    ));
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

/// Resolve a port by name/id without establishing a persistent connection.
pub fn find_midi_port(name_or_id: &str) -> Result<MidiOutputPort> {
    let midi_out = new_midi_output_with_retry(
        "autosample-find",
        &format!("while resolving output '{}'", name_or_id),
    )?;
    find_port_in_output(&midi_out, name_or_id)
}

/// Connect to the named/indexed MIDI output.
///
/// Returns `(connection, resolved_port_name, all_available_port_strings)`.
///
/// A single `MidiOutput` client is created, used for both enumeration and
/// connection (midir consumes the client on `connect()`), so there is only
/// one CoreMIDI init cycle per call.
pub fn connect_midi_output_by_name(
    name_or_id: &str,
) -> Result<(MidiOutputConnection, String, Vec<String>)> {
    // Ensure the persistent warmup client exists so CoreMIDI is already live.
    ensure_midi_warmup();

    let midi_out = new_midi_output_with_retry(
        "autosample-connect",
        &format!("while connecting output '{}'", name_or_id),
    )?;

    let available_ports: Vec<String> = midi_out
        .ports()
        .iter()
        .enumerate()
        .map(|(idx, port)| {
            let name = midi_out
                .port_name(port)
                .unwrap_or_else(|_| "Unknown".to_string());
            format!("[{}] {}", idx, name)
        })
        .collect();

    let selected_port = find_port_in_output(&midi_out, name_or_id)?;
    let selected_name = midi_out
        .port_name(&selected_port)
        .unwrap_or_else(|_| "Unknown".to_string());

    info!("Connecting to MIDI port: {}", selected_name);

    // midir consumes `midi_out` here — only one CoreMIDI client total.
    let connection = midi_out
        .connect(&selected_port, "autosample")
        .map_err(|e| anyhow::anyhow!("Failed to connect to MIDI output: {:?}", e))?;

    // The port list is now fresh; update the cache so the GUI doesn't need to
    // re-enumerate immediately after connecting.
    let port_infos: Vec<MidiPortInfo> = available_ports
        .iter()
        .enumerate()
        .map(|(idx, s)| MidiPortInfo {
            index: idx,
            name: s
                .trim_start_matches(|c: char| c == '[' || c.is_ascii_digit() || c == ']' || c == ' ')
                .to_string(),
        })
        .collect();
    store_port_cache(port_infos);

    Ok((connection, selected_name, available_ports))
}

// ---------------------------------------------------------------------------
// MIDI message helpers
// ---------------------------------------------------------------------------

pub fn send_note_on(conn: &mut MidiOutputConnection, note: u8, velocity: u8) -> Result<()> {
    send_note_on_channel(conn, note, velocity, 0)
}

pub fn send_note_on_channel(
    conn: &mut MidiOutputConnection,
    note: u8,
    velocity: u8,
    channel: u8,
) -> Result<()> {
    let ch = channel & 0x0F;
    conn.send(&[0x90 | ch, note, velocity])?;
    Ok(())
}

pub fn send_note_off(conn: &mut MidiOutputConnection, note: u8) -> Result<()> {
    send_note_off_channel(conn, note, 0)
}

pub fn send_note_off_channel(conn: &mut MidiOutputConnection, note: u8, channel: u8) -> Result<()> {
    let ch = channel & 0x0F;
    conn.send(&[0x80 | ch, note, 0])?;
    Ok(())
}

pub fn send_all_notes_off(conn: &mut MidiOutputConnection) -> Result<()> {
    for note in 0u8..128 {
        let _ = send_note_off(conn, note);
    }
    thread::sleep(Duration::from_millis(10));
    Ok(())
}