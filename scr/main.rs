mod audio;
mod cli;
mod dsp;
mod engine;
mod export;
mod midi;
mod ringbuf;
mod types;

use anyhow::Result;
use clap::Parser;
use cli::{Cli, Commands};
use tracing_subscriber::{fmt, prelude::*, EnvFilter};

fn main() -> Result<()> {
    // Setup logging
    tracing_subscriber::registry()
        .with(fmt::layer())
        
.with(EnvFilter::from_default_env().add_directive(tracing::Level::INFO.into()))
        .init();

    let cli = Cli::parse();

    match cli.command {
        Commands::ListMidi => {
            midi::list_midi_outputs()?;
        }
        Commands::ListAudio => {
            audio::list_audio_inputs()?;
        }
        Commands::Run(config) => {
            engine::run_autosampler(config)?;
        }
    }

    Ok(())
}
