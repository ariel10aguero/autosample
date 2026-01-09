use autosample_core::RunConfig;
use clap::{Parser, Subcommand};

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

impl clap::Args for RunConfig {
    fn augment_args(cmd: clap::Command) -> clap::Command {
        cmd
            .arg(clap::Arg::new("midi_out")
                .long("midi-out")
                .value_name("DEVICE")
                .help("MIDI output device name or index")
                .required(true))
            .arg(clap::Arg::new("audio_in")
                .long("audio-in")
                .value_name("DEVICE")
                .help("Audio input device name or index")
                .required(true))
            .arg(clap::Arg::new("notes")
                .long("notes")
                .value_name("RANGE")
                .help("Note range (e.g., C2..C6, C4,E4,G4)")
                .required(true))
            .arg(clap::Arg::new("vel")
                .long("vel")
                .value_name("RANGE")
                .help("Velocity layers (e.g., 127..1:8, 127,100,64)")
                .required(true))
            .arg(clap::Arg::new("hold_ms")
                .long("hold-ms")
                .value_name("MS")
                .help("Hold duration in milliseconds")
                .default_value("1000"))
            .arg(clap::Arg::new("tail_ms")
                .long("tail-ms")
                .value_name("MS")
                .help("Tail duration in milliseconds")
                .default_value("2000"))
            .arg(clap::Arg::new("preroll_ms")
                .long("preroll-ms")
                .value_name("MS")
                .help("Preroll duration in milliseconds")
                .default_value("100"))
            .arg(clap::Arg::new("sr")
                .long("sr")
                .value_name("HZ")
                .help("Sample rate")
                .default_value("48000"))
            .arg(clap::Arg::new("channels")
                .long("channels")
                .value_name("N")
                .help("Number of channels (1 or 2)")
                .default_value("2"))
            .arg(clap::Arg::new("format")
                .long("format")
                .value_name("FORMAT")
                .help("Output format (wav, mp3, both)")
                .default_value("wav"))
            .arg(clap::Arg::new("output")
                .long("output")
                .value_name("DIR")
                .help("Output directory")
                .required(true))
            .arg(clap::Arg::new("prefix")
                .long("prefix")
                .value_name("NAME")
                .help("File prefix/instrument name")
                .required(true))
            .arg(clap::Arg::new("trim_threshold_db")
                .long("trim-threshold-db")
                .value_name("DB")
                .help("Silence trim threshold in dB"))
            .arg(clap::Arg::new("normalize")
                .long("normalize")
                .value_name("MODE")
                .help("Normalization mode (peak, -1dB)"))
            .arg(clap::Arg::new("round_robin")
                .long("round-robin")
                .value_name("N")
                .help("Round robin count")
                .default_value("1"))
            .arg(clap::Arg::new("resume")
                .long("resume")
                .help("Skip existing files")
                .action(clap::ArgAction::SetTrue))
    }

    fn augment_args_for_update(cmd: clap::Command) -> clap::Command {
        Self::augment_args(cmd)
    }
}

impl clap::FromArgMatches for RunConfig {
    fn from_arg_matches(matches: &clap::ArgMatches) -> Result<Self, clap::Error> {
        Ok(RunConfig {
            midi_out: matches.get_one::<String>("midi_out").unwrap().clone(),
            audio_in: matches.get_one::<String>("audio_in").unwrap().clone(),
            notes: matches.get_one::<String>("notes").unwrap().clone(),
            vel: matches.get_one::<String>("vel").unwrap().clone(),
            hold_ms: matches.get_one::<String>("hold_ms").unwrap().parse().unwrap(),
            tail_ms: matches.get_one::<String>("tail_ms").unwrap().parse().unwrap(),
            preroll_ms: matches.get_one::<String>("preroll_ms").unwrap().parse().unwrap(),
            sr: matches.get_one::<String>("sr").unwrap().parse().unwrap(),
            channels: matches.get_one::<String>("channels").unwrap().parse().unwrap(),
            format: matches.get_one::<String>("format").unwrap().clone(),
            output: matches.get_one::<String>("output").unwrap().clone(),
            prefix: matches.get_one::<String>("prefix").unwrap().clone(),
            trim_threshold_db: matches.get_one::<String>("trim_threshold_db").map(|s| s.parse().unwrap()),
            normalize: matches.get_one::<String>("normalize").cloned(),
            round_robin: matches.get_one::<String>("round_robin").unwrap().parse().unwrap(),
            resume: matches.get_flag("resume"),
        })
    }

    fn update_from_arg_matches(&mut self, matches: &clap::ArgMatches) -> Result<(), clap::Error> {
        *self = Self::from_arg_matches(matches)?;
        Ok(())
    }
}