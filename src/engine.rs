use crate::audio::{find_audio_input, AudioCapture};
use crate::cli::RunConfig;
use crate::export::process_and_export;
use crate::midi::{find_midi_output, MidiController};
use crate::ringbuf::CaptureBuffer;
use crate::types::{
    format_filename, parse_notes, parse_velocities, SampleJob, SampleMetadata, SessionMetadata,
};
use anyhow::{Context, Result};
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::thread;
use std::time::{Duration, Instant};
use tracing::{error, info, warn};

pub fn run_autosampler(config: RunConfig) -> Result<()> {
    info!("Starting autosampler session");
    info!("Configuration: {:?}", config);

    // Parse notes and velocities
    let notes = parse_notes(&config.notes)?;
    let velocities = parse_velocities(&config.vel)?;

    info!(
        "Will sample {} notes × {} velocities × {} round-robins = {} total samples",
        notes.len(),
        velocities.len(),
        config.rr,
        notes.len() * velocities.len() * config.rr as usize
    );

    // Setup MIDI
    let midi_port = find_midi_output(&config.midi_out)?;
    let mut midi = MidiController::new(midi_port)?;

    // Setup audio
    let audio_device = find_audio_input(&config.audio_in)?;
    let audio_capture = AudioCapture::new(audio_device, config.sr, config.channels)?;

    // Setup output directory
    let output_dir = PathBuf::from(&config.output);
    let samples_dir = output_dir.join(&config.prefix).join("samples");
    fs::create_dir_all(&samples_dir)
        .with_context(|| format!("Failed to create output directory: {}", samples_dir.display()))?;

    // Setup CTRL-C handler
    let running = Arc::new(AtomicBool::new(true));
    let r = running.clone();
    ctrlc::set_handler(move || {
        warn!("Received interrupt signal, stopping...");
        r.store(false, Ordering::SeqCst);
    })?;

    // Generate job list
    let mut jobs = Vec::new();
    for note in &notes {
        for &velocity in &velocities {
            for rr in 1..=config.rr {
                jobs.push(SampleJob {
                    note: *note,
                    velocity,
                    rr_index: rr,
                });
            }
        }
    }

    let total_jobs = jobs.len();
    let mut completed = 0;
    let mut sample_metadata = Vec::new();

    // Main sampling loop
    for job in jobs {
        if !running.load(Ordering::SeqCst) {
            info!("Stopping early due to interrupt");
            break;
        }

        // Build output path
        let filename = format_filename(
            &config.prefix,
            job.note,
            job.velocity,
            job.rr_index,
            "wav",
        );
        let note_dir = samples_dir.join(format!("{:03}_{}", job.note.0, job.note.to_name()));
        let output_path = note_dir.join(&filename);

        // Skip if resume and file exists
        if config.resume && (output_path.exists() || output_path.with_extension("mp3").exists()) {
            info!(
                "[{}/{}] Skipping existing: {} vel{} rr{}",
                completed + 1,
                total_jobs,
                job.note,
                job.velocity,
                job.rr_index
            );
            completed += 1;
            continue;
        }

        info!(
            "[{}/{}] Sampling: {} vel{} rr{}",
            completed + 1,
            total_jobs,
            job.note,
            job.velocity,
            job.rr_index
        );

        // Capture sample
        match capture_sample(
            &mut midi,
            &audio_capture,
            &job,
            &config,
            &output_path,
        ) {
            Ok(metadata) => {
                sample_metadata.push(metadata);
                completed += 1;
            }
            Err(e) => {
                error!("Failed to capture sample: {}", e);
                if !running.load(Ordering::SeqCst) {
                    break;
                }
            }
        }
    }

    // Cleanup
    info!("Sending all notes off...");
    midi.all_notes_off()?;

    // Write session metadata
    let session = SessionMetadata {
        timestamp: chrono::Utc::now(),
        config: config.clone(),
        samples: sample_metadata,
    };

    let session_path = output_dir.join(&config.prefix).join("session.json");
    let session_json = serde_json::to_string_pretty(&session)?;
    fs::write(&session_path, session_json)?;

    info!("Session complete! Captured {} samples", completed);
    info!("Session metadata: {}", session_path.display());

    Ok(())
}

fn capture_sample(
    midi: &mut MidiController,
    audio_capture: &AudioCapture,
    job: &SampleJob,
    config: &RunConfig,
    output_path: &Path,
) -> Result<SampleMetadata> {
    let total_duration_ms = config.preroll_ms + config.hold_ms + config.tail_ms;
    let mut buffer = CaptureBuffer::new(
        config.channels as usize,
        config.sr,
        total_duration_ms + 500, // Add margin
    );

    let receiver = audio_capture.receiver();

    // Wait a bit for buffer to stabilize
    thread::sleep(Duration::from_millis(50));

    // Clear any buffered audio
    while receiver.try_recv().is_ok() {}

    let preroll_duration = Duration::from_millis(config.preroll_ms);
    let hold_duration = Duration::from_millis(config.hold_ms);
    let tail_duration = Duration::from_millis(config.tail_ms);

    // Start capturing
    let capture_start = Instant::now();

    // Collect preroll
    while capture_start.elapsed() < preroll_duration {
        if let Ok(data) = receiver.recv_timeout(Duration::from_millis(100)) {
            buffer.append(&data);
        }
    }

    // Send MIDI note on
    midi.note_on(job.note.0, job.velocity)?;
    let note_on_time = Instant::now();

    // Capture during hold
    while note_on_time.elapsed() < hold_duration {
        if let Ok(data) = receiver.recv_timeout(Duration::from_millis(100)) {
            buffer.append(&data);
        }
    }

    // Send MIDI note off
    midi.note_off(job.note.0)?;
    let note_off_time = Instant::now();

    // Capture tail
    while note_off_time.elapsed() < tail_duration {
        if let Ok(data) = receiver.recv_timeout(Duration::from_millis(100)) {
            buffer.append(&data);
        }
    }

    let duration_ms = buffer.duration_ms();
    info!("Captured {:.1} ms of audio", duration_ms);

    // Process and export
    let samples = buffer.into_data();
    let peak_db = process_and_export(
        samples,
        config.channels,
        config.sr,
        config.bit_depth,
        &config.format,
        output_path,
        config.trim_threshold_db,
        config.min_tail_ms,
        &config.normalize,
    )?;

    // Determine actual filename based on format
    let actual_filename = match config.format.as_str() {
        "wav" => output_path.with_extension("wav"),
        "mp3" => output_path.with_extension("mp3"),
        "both" => output_path.with_extension("wav"), // Primary format
        _ => output_path.to_path_buf(),
    };

    Ok(SampleMetadata {
        job: job.clone(),
        filename: actual_filename
            .file_name()
            .unwrap()
            .to_string_lossy()
            .to_string(),
        peak_db,
        duration_ms,
    })
}