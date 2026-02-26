use autosample_core::{OutputOrganization, RunConfig};
use clap::{Args, Parser, Subcommand};

#[derive(Parser)]
#[command(name = "autosample")]
#[command(about = "Cross-platform CLI autosampler", long_about = None)]
pub struct Cli {
    #[command(subcommand)]
    pub command: Commands,
}

#[derive(Subcommand)]
pub enum Commands {
    /// List available MIDI output ports
    ListMidi,

    /// List available audio input devices
    ListAudio,

    /// Run autosampling session
    Run(RunConfigArgs),
}

/// Local CLI-facing args type (allowed to derive clap traits)
#[derive(Debug, Clone, Args)]
pub struct RunConfigArgs {
    /// MIDI output device name or index
    #[arg(long = "midi-out", value_name = "DEVICE", required = true)]
    pub midi_out: String,

    /// Audio input device name or index
    #[arg(long = "audio-in", value_name = "DEVICE", required = true)]
    pub audio_in: String,

    /// Note range (e.g., C2..C6, C4,E4,G4)
    #[arg(long, value_name = "RANGE", required = true)]
    pub notes: String,

    /// Velocity layers (e.g., 127..1:8, 127,100,64)
    #[arg(long, value_name = "RANGE", required = true)]
    pub vel: String,

    /// Hold duration in milliseconds
    #[arg(long = "hold-ms", value_name = "MS", default_value_t = 1000)]
    pub hold_ms: u32,

    /// Tail duration in milliseconds
    #[arg(long = "tail-ms", value_name = "MS", default_value_t = 2000)]
    pub tail_ms: u32,

    /// Preroll duration in milliseconds
    #[arg(long = "preroll-ms", value_name = "MS", default_value_t = 100)]
    pub preroll_ms: u32,

    /// Sample rate
    #[arg(long = "sr", value_name = "HZ", default_value_t = 48000)]
    pub sr: u32,

    /// Number of channels (1 or 2)
    #[arg(long, value_name = "N", default_value_t = 2)]
    pub channels: u16,

    /// Output format (wav, mp3, both)
    #[arg(long, value_name = "FORMAT", default_value = "wav")]
    pub format: String,

    /// Output directory
    #[arg(long, value_name = "DIR", required = true)]
    pub output: String,

    /// File prefix/instrument name
    #[arg(long, value_name = "NAME", required = true)]
    pub prefix: String,

    /// Silence trim threshold in dB
    #[arg(long = "trim-threshold-db", value_name = "DB")]
    pub trim_threshold_db: Option<f32>,

    /// Normalization mode (peak, -1dB)
    #[arg(long, value_name = "MODE")]
    pub normalize: Option<String>,

    /// Round robin count
    #[arg(long = "round-robin", value_name = "N", default_value_t = 1)]
    pub round_robin: u32,

    /// Skip existing files
    #[arg(long, action = clap::ArgAction::SetTrue)]
    pub resume: bool,

    /// Output directory organization (flat, by-note)
    #[arg(
        long = "output-organization",
        value_name = "MODE",
        default_value = "flat",
        value_parser = ["flat", "by-note"]
    )]
    pub output_organization: String,
}

impl From<RunConfigArgs> for RunConfig {
    fn from(a: RunConfigArgs) -> Self {
        RunConfig {
            midi_out: a.midi_out,
            audio_in: a.audio_in,
            notes: a.notes,
            vel: a.vel,
            hold_ms: a.hold_ms,
            tail_ms: a.tail_ms,
            preroll_ms: a.preroll_ms,
            sr: a.sr,
            channels: a.channels,
            format: a.format,
            output: a.output,
            prefix: a.prefix,
            trim_threshold_db: a.trim_threshold_db,
            normalize: a.normalize,
            round_robin: a.round_robin,
            resume: a.resume,
            output_organization: OutputOrganization::from_str(&a.output_organization)
                .unwrap_or_default(),
        }
    }
}
