use clap::{Parser, Subcommand};
use serde::{Deserialize, Serialize};

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
    Run(RunConfig),
}

#[derive(Parser, Clone, Debug, Serialize, Deserialize)]
pub struct RunConfig {
    /// MIDI output port (name or index)
    #[arg(long)]
    pub midi_out: String,

    /// Audio input device (name or index)
    #[arg(long)]
    pub audio_in: String,

    /// Note range or list (e.g., "C2..C6" or "C2,D2,E2")
    #[arg(long)]
    pub notes: String,

    /// Velocity range or list (e.g., "127..1:8" or "127,100,80")
    #[arg(long)]
    pub vel: String,

    /// Hold duration in milliseconds (NoteOn to NoteOff)
    #[arg(long, default_value = "1000")]
    pub hold_ms: u64,

    /// Tail duration in milliseconds (recording after NoteOff)
    #[arg(long, default_value = "2000")]
    pub tail_ms: u64,

    /// Preroll duration in milliseconds (recording before NoteOn)
    #[arg(long, default_value = "100")]
    pub preroll_ms: u64,

    /// Sample rate
    #[arg(long, default_value = "48000")]
    pub sr: u32,

    /// Number of channels (1 or 2)
    #[arg(long, default_value = "2")]
    pub channels: u16,

    /// Output format: wav, mp3, or both
    #[arg(long, default_value = "wav")]
    pub format: String,

    /// Output directory
    #[arg(long)]
    pub output: String,

    /// Instrument name/prefix
    #[arg(long)]
    pub prefix: String,

    /// Trim threshold in dBFS (e.g., -55)
    #[arg(long)]
    pub trim_threshold_db: Option<f32>,

    /// Normalization mode: off, peak, or -1db
    #[arg(long, default_value = "off")]
    pub normalize: String,

    /// Round-robin count (>=1)
    #[arg(long, default_value = "1")]
    pub rr: u32,

    /// Resume mode: skip existing files
    #[arg(long, default_value = "false")]
    pub resume: bool,

    /// Minimum tail duration in ms (when trimming)
    #[arg(long, default_value = "500")]
    pub min_tail_ms: u64,

    /// Bit depth for WAV output (16 or 24)
    #[arg(long, default_value = "24")]
    pub bit_depth: u16,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_cli_parsing() {
        let args = vec![
            "autosample",
            "run",
            "--midi-out",
            "Test",
            "--audio-in",
            "Test",
            "--notes",
            "C2..C4",
            "--vel",
            "127",
            "--output",
            "./out",
            "--prefix",
            "test",
        ];
        
        let cli = Cli::try_parse_from(args);
        assert!(cli.is_ok());
    }
}
