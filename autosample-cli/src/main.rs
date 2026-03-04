mod cli;

use anyhow::Result;
use autosample_core::{audio, midi, AutosampleEngine, LogLevel, ProgressUpdate, RunConfig};
use clap::Parser;
use cli::{Cli, Commands};
use crossbeam_channel::unbounded;
use std::sync::atomic::{AtomicBool, Ordering};
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
        Commands::Run(args) => {
            // Convert CLI args into core config type
            let config: RunConfig = args.into();

            let (progress_tx, progress_rx) = unbounded();

            // Setup Ctrl+C handler
            let cancel_flag = Arc::new(AtomicBool::new(false));
            let r = cancel_flag.clone();
            ctrlc::set_handler(move || {
                warn!("Received Ctrl+C, stopping...");
                r.store(true, Ordering::SeqCst);
            })?;

            // Spawn engine thread
            let config_clone = config.clone();
            let cancel_clone = cancel_flag.clone();
            let engine_handle = thread::spawn(move || {
                let mut eng = AutosampleEngine::new();

                // Optional cancel monitor (kept from your code)
                let cancel_monitor = cancel_clone.clone();
                thread::spawn(move || loop {
                    if cancel_monitor.load(Ordering::SeqCst) {
                        break;
                    }
                    thread::sleep(std::time::Duration::from_millis(100));
                });

                eng.run(config_clone, progress_tx)
            });

            // Process progress updates
            for event in progress_rx {
                if cancel_flag.load(Ordering::SeqCst) {
                    break;
                }

                match event {
                    ProgressUpdate::Started {
                        total_samples,
                        output_dir,
                    } => {
                        info!(
                            "Starting session: {} samples to record (output: {})",
                            total_samples, output_dir
                        );
                    }
                    ProgressUpdate::SampleStarted {
                        index,
                        total,
                        note,
                        velocity,
                        rr,
                    } => {
                        info!(
                            "[{}/{}] Recording note {}, vel {}, rr {}",
                            index, total, note, velocity, rr
                        );
                    }
                    ProgressUpdate::SampleCompleted {
                        index,
                        total,
                        path,
                        peak_db,
                    } => {
                        info!(
                            "[{}/{}] Completed: {} (peak: {:.1} dB)",
                            index, total, path, peak_db
                        );
                    }
                    ProgressUpdate::SampleSkipped { path, .. } => {
                        info!("Skipped: {}", path);
                    }
                    ProgressUpdate::SampleFailed { error, .. } => {
                        error!("Failed: {}", error);
                    }
                    ProgressUpdate::Completed { samples_recorded } => {
                        info!("Session complete! {} samples recorded", samples_recorded);
                        break;
                    }
                    ProgressUpdate::Cancelled => {
                        warn!("Session cancelled by user");
                        break;
                    }
                    ProgressUpdate::Log { level, message } => match level {
                        LogLevel::Info => info!("{}", message),
                        LogLevel::Warning => warn!("{}", message),
                        LogLevel::Error => error!("{}", message),
                    },
                }
            }

            // Wait for engine to finish
            let _ = engine_handle.join();
        }
    }

    Ok(())
}
