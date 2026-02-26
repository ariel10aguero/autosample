use autosample_core::{
    AudioDeviceInfo, EngineStatus, LogLevel, MidiPortInfo, ProgressUpdate, RunConfig,
};
use crossbeam_channel::{Receiver, TryRecvError};

use crate::device_scan::{self, DeviceScanResult};

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

    // Presets
    pub preset_name: String,

    // Device scan state
    pub device_scan_state: DeviceScanState,
    device_scan_rx: Option<Receiver<device_scan::DeviceScanOutcome>>,
    pending_midi_scan_error: Option<String>,
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

impl Default for AppState {
    fn default() -> Self {
        Self {
            midi_devices: Vec::new(),
            audio_devices: Vec::new(),
            selected_midi_idx: None,
            selected_audio_idx: None,
            config: RunConfig::default(),
            engine_status: EngineStatus::Idle,
            progress: ProgressState::default(),
            logs: Vec::new(),
            preset_name: String::new(),
            device_scan_state: DeviceScanState::Idle,
            device_scan_rx: None,
            pending_midi_scan_error: None,
        }
    }
}

impl AppState {
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

        let preferred_midi = self.config.midi_out.trim().to_string();
        self.pending_midi_scan_error = None;

        // Run MIDI scan on the app thread to avoid backend init issues in short-lived worker threads.
        match autosample_core::midi::get_midi_ports() {
            Ok(midi_devices) => {
                self.apply_midi_scan_result(midi_devices, &preferred_midi);
            }
            Err(error) => {
                let message = format!("MIDI scan failed: {}", error);
                self.pending_midi_scan_error = Some(message.clone());
                self.add_log(LogLevel::Warning, message);
            }
        }

        self.device_scan_state = DeviceScanState::Scanning;
        self.device_scan_rx = Some(device_scan::spawn_device_scan());
        self.add_log(
            LogLevel::Info,
            "Refreshing device list (MIDI now, audio in background)...".to_string(),
        );
    }

    pub fn poll_device_scan_result(&mut self) {
        let Some(rx) = self.device_scan_rx.clone() else {
            return;
        };

        match rx.try_recv() {
            Ok(outcome) => {
                self.device_scan_rx = None;
                match outcome {
                    Ok(result) => {
                        self.apply_audio_scan_result(result);
                        let midi_error = self.pending_midi_scan_error.take();
                        if let Some(error) = midi_error {
                            self.device_scan_state = DeviceScanState::Failed(error.clone());
                            self.add_log(
                                LogLevel::Warning,
                                format!(
                                    "Device refresh completed with warnings: {}",
                                    error
                                ),
                            );
                        } else {
                            self.device_scan_state = DeviceScanState::Idle;
                            self.add_log(
                                LogLevel::Info,
                                format!(
                                    "Device refresh complete: {} MIDI, {} audio",
                                    self.midi_devices.len(),
                                    self.audio_devices.len()
                                ),
                            );
                        }
                    }
                    Err(error) => {
                        let combined_error = if let Some(midi_error) =
                            self.pending_midi_scan_error.take()
                        {
                            format!("{}; {}", midi_error, error)
                        } else {
                            error
                        };
                        self.device_scan_state = DeviceScanState::Failed(combined_error.clone());
                        self.add_log(
                            LogLevel::Warning,
                            format!("Device refresh failed: {}", combined_error),
                        );
                    }
                }
            }
            Err(TryRecvError::Empty) => {}
            Err(TryRecvError::Disconnected) => {
                self.device_scan_rx = None;
                let mut error = "Device refresh worker disconnected".to_string();
                if let Some(midi_error) = self.pending_midi_scan_error.take() {
                    error = format!("{}; {}", midi_error, error);
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
            DeviceScanState::Failed(message) => Some(message.as_str()),
            _ => None,
        }
    }

    fn apply_audio_scan_result(&mut self, result: DeviceScanResult) {
        let preferred_audio = self.config.audio_in.trim().to_string();

        self.audio_devices = result.audio_devices;
        self.selected_audio_idx = Self::resolve_selection_index(
            &preferred_audio,
            &self.audio_devices,
            |device| device.name.as_str(),
        );

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
        self.selected_midi_idx = Self::resolve_selection_index(
            preferred_midi,
            &self.midi_devices,
            |device| device.name.as_str(),
        );

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
            .position(|device| get_name(device) == preferred)
            .or(Some(0))
    }

    pub fn add_log(&mut self, level: LogLevel, message: String) {
        self.logs.push(LogEntry {
            timestamp: chrono::Local::now().format("%H:%M:%S").to_string(),
            level,
            message,
        });
    }

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

        self.preset_name = std::path::Path::new(path)
            .file_stem()
            .and_then(|s| s.to_str())
            .unwrap_or_default()
            .to_string();

        Ok(())
    }

    pub fn clear_project(&mut self) {
        self.logs.clear();
        self.progress = ProgressState::default();
        self.engine_status = EngineStatus::Idle;
        self.preset_name.clear();
    }
}
