use autosample_core::{
    AudioDeviceInfo, EngineStatus, LogLevel, MidiPortInfo, ProgressUpdate, RunConfig,
};
use crossbeam_channel::{Receiver, TryRecvError};
use std::time::Instant;

use crate::device_scan::{self, DeviceScanResult};

// ---------------------------------------------------------------------------
// Public state types
// ---------------------------------------------------------------------------

#[derive(Clone)]
pub struct AppState {
    // Devices
    pub midi_devices: Vec<MidiPortInfo>,
    pub audio_devices: Vec<AudioDeviceInfo>,
    pub selected_midi_idx: Option<usize>,
    pub selected_audio_idx: Option<usize>,

    // Config
    pub config: RunConfig,

    // Session state
    pub engine_status: EngineStatus,
    pub progress: ProgressState,
    pub logs: Vec<LogEntry>,
    pub input_meter_db: Option<f32>,
    pub audio_permission_state: AudioInputPermissionState,

    // Presets
    pub preset_name: String,

    // Device scan state
    pub device_scan_state: DeviceScanState,
    pub last_scan_completed_at: Option<Instant>,

    // Private channels / flags
    device_scan_rx: Option<Receiver<device_scan::DeviceScanOutcome>>,
    pending_midi_scan_error: Option<String>,
    audio_permission_recheck_requested: bool,
}

#[derive(Clone, Default)]
pub struct ProgressState {
    pub current_index: usize,
    pub total_samples: usize,
    pub current_note: u8,
    pub current_velocity: u8,
    pub current_rr: u32,
    pub samples_completed: usize,
    pub samples_failed: usize,
    pub samples_skipped: usize,
}

#[derive(Clone)]
pub struct LogEntry {
    pub timestamp: String,
    pub level: LogLevel,
    pub message: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum DeviceScanState {
    Idle,
    Scanning,
    Failed(String),
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum AudioInputPermissionState {
    Unknown,
    Checking,
    Granted,
    Denied(String),
}

// ---------------------------------------------------------------------------
// Default
// ---------------------------------------------------------------------------

impl Default for AppState {
    fn default() -> Self {
        let config = RunConfig::default();
        Self {
            midi_devices: Vec::new(),
            audio_devices: Vec::new(),
            selected_midi_idx: None,
            selected_audio_idx: None,
            preset_name: config.prefix.clone(),
            config,
            engine_status: EngineStatus::Idle,
            progress: ProgressState::default(),
            logs: Vec::new(),
            input_meter_db: None,
            audio_permission_state: AudioInputPermissionState::Unknown,
            device_scan_state: DeviceScanState::Idle,
            last_scan_completed_at: None,
            device_scan_rx: None,
            pending_midi_scan_error: None,
            audio_permission_recheck_requested: false,
        }
    }
}

// ---------------------------------------------------------------------------
// AppState impl
// ---------------------------------------------------------------------------

impl AppState {
    // -----------------------------------------------------------------------
    // Device scanning
    // -----------------------------------------------------------------------

    pub fn request_device_scan(&mut self) {
        if self.engine_status == EngineStatus::Running {
            self.add_log(
                LogLevel::Warning,
                "Device refresh ignored: sampling is currently running.".to_string(),
            );
            return;
        }
        if self.is_device_scan_running() {
            self.add_log(
                LogLevel::Info,
                "Device refresh already in progress.".to_string(),
            );
            return;
        }

        tracing::info!("Requesting device scan");
        let preferred_midi = self.config.midi_out.trim().to_string();
        self.pending_midi_scan_error = None;

        // MIDI scan on the app thread — uses the cached port list from midi.rs.
        match autosample_core::midi::get_midi_ports() {
            Ok(midi_devices) => {
                self.apply_midi_scan_result(midi_devices, &preferred_midi);
            }
            Err(e) => {
                let msg = format!("MIDI scan failed: {}", e);
                self.pending_midi_scan_error = Some(msg.clone());
                self.add_log(LogLevel::Warning, msg);
            }
        }

        // Audio scan runs in a background thread.
        self.device_scan_state = DeviceScanState::Scanning;
        self.device_scan_rx = Some(device_scan::spawn_device_scan());
        self.add_log(
            LogLevel::Info,
            "Refreshing device list (MIDI cached, audio in background)…".to_string(),
        );
    }

    /// Must be called every frame from `eframe::App::update`.
    pub fn poll_device_scan_result(&mut self) {
        let rx = match self.device_scan_rx.clone() {
            Some(r) => r,
            None => return,
        };

        match rx.try_recv() {
            Ok(outcome) => {
                self.device_scan_rx = None;
                self.last_scan_completed_at = Some(Instant::now());

                match outcome {
                    Ok(result) => {
                        self.apply_audio_scan_result(result);

                        if let Some(midi_err) = self.pending_midi_scan_error.take() {
                            self.device_scan_state = DeviceScanState::Failed(midi_err.clone());
                            self.add_log(
                                LogLevel::Warning,
                                format!("Device refresh completed with warnings: {}", midi_err),
                            );
                        } else {
                            self.device_scan_state = DeviceScanState::Idle;
                            self.add_log(
                                LogLevel::Info,
                                format!(
                                    "Device refresh complete: {} MIDI, {} audio.",
                                    self.midi_devices.len(),
                                    self.audio_devices.len()
                                ),
                            );
                        }
                    }
                    Err(audio_err) => {
                        let combined = if let Some(midi_err) = self.pending_midi_scan_error.take() {
                            format!("{}; {}", midi_err, audio_err)
                        } else {
                            audio_err
                        };
                        self.device_scan_state = DeviceScanState::Failed(combined.clone());
                        self.add_log(
                            LogLevel::Warning,
                            format!("Device refresh failed: {}", combined),
                        );
                    }
                }
            }

            Err(TryRecvError::Empty) => {}

            Err(TryRecvError::Disconnected) => {
                self.device_scan_rx = None;
                self.last_scan_completed_at = Some(Instant::now());

                let mut error = "Device refresh worker disconnected unexpectedly".to_string();
                if let Some(midi_err) = self.pending_midi_scan_error.take() {
                    error = format!("{}; {}", midi_err, error);
                }
                self.device_scan_state = DeviceScanState::Failed(error.clone());
                self.add_log(
                    LogLevel::Warning,
                    format!("Device refresh failed: {}", error),
                );
            }
        }
    }

    pub fn is_device_scan_running(&self) -> bool {
        matches!(self.device_scan_state, DeviceScanState::Scanning)
    }

    pub fn device_scan_error(&self) -> Option<&str> {
        match &self.device_scan_state {
            DeviceScanState::Failed(msg) => Some(msg.as_str()),
            _ => None,
        }
    }

    pub fn request_audio_permission_recheck(&mut self) {
        self.audio_permission_recheck_requested = true;
    }

    pub fn consume_audio_permission_recheck_requested(&mut self) -> bool {
        let was = self.audio_permission_recheck_requested;
        self.audio_permission_recheck_requested = false;
        was
    }

    // -----------------------------------------------------------------------
    // Internal scan helpers
    // -----------------------------------------------------------------------

    fn apply_audio_scan_result(&mut self, result: DeviceScanResult) {
        let preferred_audio = self.config.audio_in.trim().to_string();
        self.audio_devices = result.audio_devices;
        self.selected_audio_idx =
            Self::resolve_selection_index(&preferred_audio, &self.audio_devices, |d| {
                d.name.as_str()
            });

        if let Some(idx) = self.selected_audio_idx {
            if let Some(device) = self.audio_devices.get(idx) {
                self.config.audio_in = device.name.clone();
            }
        } else {
            self.config.audio_in.clear();
        }
    }

    fn apply_midi_scan_result(&mut self, midi_devices: Vec<MidiPortInfo>, preferred_midi: &str) {
        self.midi_devices = midi_devices;
        self.selected_midi_idx =
            Self::resolve_selection_index(preferred_midi, &self.midi_devices, |d| d.name.as_str());

        if let Some(idx) = self.selected_midi_idx {
            if let Some(device) = self.midi_devices.get(idx) {
                self.config.midi_out = device.name.clone();
            }
        } else {
            self.config.midi_out.clear();
        }
    }

    fn resolve_selection_index<T, F>(preferred: &str, devices: &[T], get_name: F) -> Option<usize>
    where
        F: Fn(&T) -> &str,
    {
        if devices.is_empty() {
            return None;
        }
        if preferred.is_empty() {
            return Some(0);
        }
        devices
            .iter()
            .position(|d| get_name(d) == preferred)
            .or(Some(0))
    }

    // -----------------------------------------------------------------------
    // Logging
    // -----------------------------------------------------------------------

    pub fn add_log(&mut self, level: LogLevel, message: String) {
        tracing::debug!("[{:?}] {}", level, message);
        self.logs.push(LogEntry {
            timestamp: chrono::Local::now().format("%H:%M:%S").to_string(),
            level,
            message,
        });
    }

    // -----------------------------------------------------------------------
    // Audio permission state
    // -----------------------------------------------------------------------

    pub fn set_audio_permission_checking(&mut self) {
        self.audio_permission_state = AudioInputPermissionState::Checking;
    }

    pub fn set_audio_permission_granted(&mut self) {
        self.audio_permission_state = AudioInputPermissionState::Granted;
    }

    pub fn set_audio_permission_denied(&mut self, reason: String) {
        self.audio_permission_state = AudioInputPermissionState::Denied(reason);
    }

    pub fn reset_audio_permission_state(&mut self) {
        self.audio_permission_state = AudioInputPermissionState::Unknown;
    }

    pub fn audio_permission_is_granted(&self) -> bool {
        matches!(
            self.audio_permission_state,
            AudioInputPermissionState::Granted
        )
    }

    // -----------------------------------------------------------------------
    // Engine events
    // -----------------------------------------------------------------------

    pub fn handle_engine_event(&mut self, event: ProgressUpdate) {
        match event {
            ProgressUpdate::Started { total_samples } => {
                self.progress.total_samples = total_samples;
                self.progress.current_index = 0;
                self.progress.samples_completed = 0;
                self.progress.samples_failed = 0;
                self.progress.samples_skipped = 0;
                self.engine_status = EngineStatus::Running;
            }
            ProgressUpdate::SampleStarted {
                index,
                note,
                velocity,
                rr,
                ..
            } => {
                self.progress.current_index = index;
                self.progress.current_note = note;
                self.progress.current_velocity = velocity;
                self.progress.current_rr = rr;
            }
            ProgressUpdate::SampleCompleted { .. } => {
                self.progress.samples_completed += 1;
            }
            ProgressUpdate::SampleSkipped { .. } => {
                self.progress.samples_skipped += 1;
            }
            ProgressUpdate::SampleFailed { .. } => {
                self.progress.samples_failed += 1;
            }
            ProgressUpdate::Completed { .. } => {
                self.engine_status = EngineStatus::Completed;
            }
            ProgressUpdate::Cancelled => {
                self.engine_status = EngineStatus::Idle;
            }
            ProgressUpdate::Log { level, message } => {
                self.add_log(level, message);
            }
        }
    }

    // -----------------------------------------------------------------------
    // Preset I/O
    // -----------------------------------------------------------------------

    pub fn save_preset(&self, path: &str) -> anyhow::Result<()> {
        let json = serde_json::to_string_pretty(&self.config)?;
        std::fs::write(path, json)?;
        Ok(())
    }

    pub fn load_preset(&mut self, path: &str) -> anyhow::Result<()> {
        let json = std::fs::read_to_string(path)?;
        self.config = serde_json::from_str(&json)?;

        self.selected_midi_idx = self
            .midi_devices
            .iter()
            .position(|d| d.name == self.config.midi_out);
        self.selected_audio_idx = self
            .audio_devices
            .iter()
            .position(|d| d.name == self.config.audio_in);

        self.preset_name = self.config.prefix.clone();
        Ok(())
    }

    // -----------------------------------------------------------------------
    // Project reset
    // -----------------------------------------------------------------------

    pub fn clear_project(&mut self) {
        self.logs.clear();
        self.progress = ProgressState::default();
        self.engine_status = EngineStatus::Idle;
        self.preset_name.clear();
        self.config.prefix.clear();
    }
}
