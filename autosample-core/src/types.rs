use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum OutputOrganization {
    #[default]
    Flat,
    ByNote,
    ByNoteVelocity,
}

impl OutputOrganization {
    pub fn from_str(s: &str) -> anyhow::Result<Self> {
        match s.to_lowercase().as_str() {
            "flat" => Ok(OutputOrganization::Flat),
            "by_note" | "by-note" | "bynote" => Ok(OutputOrganization::ByNote),
            "by_note_velocity" | "by-note-velocity" | "bynotevelocity" => {
                Ok(OutputOrganization::ByNoteVelocity)
            }
            _ => anyhow::bail!(
                "Invalid output organization: {}. Use flat, by-note, or by-note-velocity",
                s
            ),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RunConfig {
    pub midi_out: String,
    pub audio_in: String,
    pub notes: String,
    pub vel: String,
    pub hold_ms: u32,
    pub tail_ms: u32,
    pub preroll_ms: u32,
    pub sr: u32,
    pub channels: u16,
    pub format: String,
    pub output: String,
    pub prefix: String,
    pub trim_threshold_db: Option<f32>,
    pub normalize: Option<String>,
    pub round_robin: u32,
    pub resume: bool,
    #[serde(default)]
    pub output_organization: OutputOrganization,
}

impl Default for RunConfig {
    fn default() -> Self {
        Self {
            midi_out: String::new(),
            audio_in: String::new(),
            notes: "C4..C5".to_string(),
            vel: "127,100,64".to_string(),
            hold_ms: 1000,
            tail_ms: 2000,
            preroll_ms: 100,
            sr: 48000,
            channels: 2,
            format: "wav".to_string(),
            output: "./output".to_string(),
            prefix: "sample".to_string(),
            trim_threshold_db: Some(-50.0),
            normalize: Some("peak".to_string()),
            round_robin: 1,
            resume: false,
            output_organization: OutputOrganization::Flat,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OutputFormat {
    Wav,
    Mp3,
    Both,
}

impl OutputFormat {
    pub fn from_str(s: &str) -> anyhow::Result<Self> {
        match s.to_lowercase().as_str() {
            "wav" => Ok(OutputFormat::Wav),
            "mp3" => Ok(OutputFormat::Mp3),
            "both" => Ok(OutputFormat::Both),
            _ => anyhow::bail!("Invalid format: {}. Use wav, mp3, or both", s),
        }
    }

    pub fn as_str(&self) -> &'static str {
        match self {
            OutputFormat::Wav => "wav",
            OutputFormat::Mp3 => "mp3",
            OutputFormat::Both => "both",
        }
    }
}

#[derive(Debug, Clone)]
pub struct SampleJob {
    pub note: u8,
    pub velocity: u8,
    pub rr_index: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SessionMetadata {
    pub timestamp: String,
    pub config: RunConfig,
    pub samples: Vec<SampleInfo>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SampleInfo {
    pub note: u8,
    pub velocity: u8,
    pub rr_index: u32,
    pub path: String,
    pub peak_db: f32,
}

/// Progress updates sent from engine to UI
#[derive(Debug, Clone)]
pub enum ProgressUpdate {
    Started {
        total_samples: usize,
    },
    SampleStarted {
        index: usize,
        total: usize,
        note: u8,
        velocity: u8,
        rr: u32,
    },
    SampleCompleted {
        index: usize,
        total: usize,
        path: String,
        peak_db: f32,
    },
    SampleSkipped {
        index: usize,
        total: usize,
        path: String,
    },
    SampleFailed {
        index: usize,
        total: usize,
        error: String,
    },
    Completed {
        samples_recorded: usize,
    },
    Cancelled,
    Log {
        level: LogLevel,
        message: String,
    },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LogLevel {
    Info,
    Warning,
    Error,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EngineStatus {
    Idle,
    Running,
    Paused,
    Completed,
    Error,
}

/// Device information for UI display
#[derive(Debug, Clone)]
pub struct AudioDeviceInfo {
    pub index: usize,
    pub name: String,
    pub sample_rate: u32,
    pub channels: u16,
}

#[derive(Debug, Clone)]
pub struct MidiPortInfo {
    pub index: usize,
    pub name: String,
}