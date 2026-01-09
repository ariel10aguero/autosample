mod cli;

use anyhow::Result;
use autosample_core::{audio, midi, AutosampleEngine, EngineEvent, LogLevel};
use clap::Parser;
use cli::{Cli, Commands};
use crossbeam_channel::unbounded;
use std::sync::Arc;
use std::thread;
use tracing::{error, info, warn};

fn main() -> Result<()> {
    tracing_subscriber::fmt::init();

    let cli = Cli::parse();

    match cli.command {
        Commands::ListMidi => {
            midi::list_midi_ports()?;
        }
        Commands::ListAudio => {
            audio::list_audio_devices()?;
        }
        Commands::Run(config) => {
            let (progress_tx, progress_rx) = unbounded();
            let mut engine = AutosampleEngine::new();

            // Setup Ctrl+C handler
            let engine_cancel = Arc::new(engine);
            let r = engine_cancel.clone();
            ctrlc::set_handler(move || {
                warn!("Received Ctrl+C, stopping...");
                r.cancel();
            })?;

            // Spawn engine thread
            let config_clone = config.clone();
            let engine_handle = thread::spawn(move || {
                let mut eng = AutosampleEngine::new();
                eng.run(config_clone, progress_tx)
            });

            // Process progress updates
            for event in progress_rx {
                match event {
                    EngineEvent::Started { total_samples } => {
                        info!("Starting session: {} samples to record", total_samples);
                    }
                    EngineEvent::SampleStarted { index, total, note, velocity, rr } => {
                        info!("[{}/{}] Recording note {}, vel {}, rr {}", index, total, note, velocity, rr);
                    }
                    EngineEvent::SampleCompleted { index, total, path, peak_db } => {
                        info!("[{}/{}] Completed: {} (peak: {:.1} dB)", index, total, path, peak_db);
                    }
                    EngineEvent::SampleSkipped { path, .. } => {
                        info!("Skipped: {}", path);
                    }
                    EngineEvent::SampleFailed { error, .. } => {
                        error!("Failed: {}", error);
                    }
                    EngineEvent::Completed { samples_recorded } => {
                        info!("Session complete! {} samples recorded", samples_recorded);
                        break;
                    }
                    EngineEvent::Cancelled => {
                        warn!("Session cancelled by user");
                        break;
                    }
                    EngineEvent::Log { level, message } => {
                        match level {
                            LogLevel::Info => info!("{}", message),
                            LogLevel::Warning => warn!("{}", message),
                            LogLevel::Error => error!("{}", message),
                        }
                    }
                }
            }

            // Wait for engine to finish
            let _ = engine_handle.join();
        }
    }

    Ok(())
}