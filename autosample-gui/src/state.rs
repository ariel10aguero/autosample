use autosample_core::{
    AudioDeviceInfo, EngineStatus, LogLevel, MidiPortInfo, ProgressUpdate, RunConfig,
};

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
        }
    }
}

impl AppState {
    pub fn refresh_devices(&mut self) {
        if let Ok(midi) = autosample_core::midi::get_midi_ports() {
            self.midi_devices = midi;
        }
        if let Ok(audio) = autosample_core::audio::get_audio_devices() {
            self.audio_devices = audio;
        }
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

    pub fn clear_project(&mut self) {
        self.logs.clear();
        self.progress = ProgressState::default();
        self.engine_status = EngineStatus::Idle;
        self.preset_name.clear();
    }
}
