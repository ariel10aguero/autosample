use crate::audio::{find_audio_device, start_audio_capture};
use crate::dsp::{apply_fade, get_peak_db, normalize_audio, trim_silence};
use crate::export::{check_ffmpeg_available, convert_to_mp3, write_wav};
use crate::midi::{
    connect_midi_output_by_name, send_all_notes_off, send_note_off, send_note_off_channel,
    send_note_on, send_note_on_channel,
};
use crate::parse::{parse_notes, parse_velocities};
use crate::ringbuf::{consume_audio_packets, RingBuffer};
use crate::types::*;
use anyhow::{Context, Result};
use crossbeam_channel::{Receiver, Sender};
use hound::WavSpec;
use midir::MidiOutputConnection;
use std::fs;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::thread;
use std::time::Duration;
use tracing::warn;

pub struct AutosampleEngine {
    cancel_flag: Arc<AtomicBool>,
}

impl AutosampleEngine {
    pub fn new() -> Self {
        Self {
            cancel_flag: Arc::new(AtomicBool::new(false)),
        }
    }

    pub fn cancel(&self) {
        self.cancel_flag.store(true, Ordering::SeqCst);
    }

    pub fn run(
        &mut self,
        config: RunConfig,
        progress_tx: Sender<ProgressUpdate>,
    ) -> Result<SessionMetadata> {
        self.run_internal(config, progress_tx, None, None)
    }

    pub fn run_with_connected_midi(
        &mut self,
        config: RunConfig,
        progress_tx: Sender<ProgressUpdate>,
        midi_conn: MidiOutputConnection,
        connected_port_name: String,
        available_ports: Vec<String>,
    ) -> Result<SessionMetadata> {
        self.run_internal(
            config,
            progress_tx,
            Some((midi_conn, connected_port_name, available_ports)),
            None,
        )
    }

    pub fn run_with_connected_midi_and_cancel(
        &mut self,
        config: RunConfig,
        progress_tx: Sender<ProgressUpdate>,
        midi_conn: MidiOutputConnection,
        connected_port_name: String,
        available_ports: Vec<String>,
        external_cancel: Arc<AtomicBool>,
    ) -> Result<SessionMetadata> {
        self.run_internal(
            config,
            progress_tx,
            Some((midi_conn, connected_port_name, available_ports)),
            Some(external_cancel),
        )
    }

    fn run_internal(
        &mut self,
        config: RunConfig,
        progress_tx: Sender<ProgressUpdate>,
        preconnected_midi: Option<(MidiOutputConnection, String, Vec<String>)>,
        external_cancel: Option<Arc<AtomicBool>>,
    ) -> Result<SessionMetadata> {
        self.cancel_flag.store(false, Ordering::SeqCst);
        if let Some(external) = &external_cancel {
            external.store(false, Ordering::SeqCst);
        }

        let is_cancelled = || {
            self.cancel_flag.load(Ordering::SeqCst)
                || external_cancel
                    .as_ref()
                    .map(|flag| flag.load(Ordering::SeqCst))
                    .unwrap_or(false)
        };

        // Validate ffmpeg if needed
        let format = OutputFormat::from_str(&config.format)?;
        let needs_mp3 = matches!(format, OutputFormat::Mp3 | OutputFormat::Both);

        if needs_mp3 && !check_ffmpeg_available() {
            let msg = "ffmpeg not found. MP3 export will be disabled.".to_string();
            let _ = progress_tx.send(ProgressUpdate::Log {
                level: LogLevel::Warning,
                message: msg,
            });

            if format == OutputFormat::Mp3 {
                anyhow::bail!("MP3 format requested but ffmpeg is not installed");
            }
        }

        // Parse notes and velocities
        let notes = parse_notes(&config.notes)?;
        let velocities = parse_velocities(&config.vel)?;

        let _ = progress_tx.send(ProgressUpdate::Log {
            level: LogLevel::Info,
            message: format!("Notes: {:?}", notes),
        });
        let _ = progress_tx.send(ProgressUpdate::Log {
            level: LogLevel::Info,
            message: format!("Velocities: {:?}", velocities),
        });

        // Connect MIDI
        let (mut midi_conn, connected_port_name, available_ports) = if let Some(existing) =
            preconnected_midi
        {
            existing
        } else {
            let _ = progress_tx.send(ProgressUpdate::Log {
                level: LogLevel::Info,
                message: format!("Initializing MIDI output '{}'", config.midi_out),
            });
            connect_midi_output_by_name(&config.midi_out).with_context(|| {
                format!("MIDI output initialization/connection failed for '{}'", config.midi_out)
            })?
        };

        let _ = progress_tx.send(ProgressUpdate::Log {
            level: LogLevel::Info,
            message: format!("Connected to MIDI: {}", connected_port_name),
        });
        let _ = progress_tx.send(ProgressUpdate::Log {
            level: LogLevel::Info,
            message: format!("MIDI ports at connect time: {}", available_ports.join(", ")),
        });

        // Setup audio capture
        let audio_device = find_audio_device(&config.audio_in)?;
        let (audio_tx, audio_rx) = crossbeam_channel::unbounded();

        let audio_capture = start_audio_capture(audio_device, config.sr, config.channels, audio_tx)?;

        let sample_rate = audio_capture.config.sample_rate;
        let channels = audio_capture.config.channels;

        let _ = progress_tx.send(ProgressUpdate::Log {
            level: LogLevel::Info,
            message: format!(
                "Audio capture started: {} Hz, {} channels",
                sample_rate, channels
            ),
        });

        // Setup output directory
        let output_dir = prepare_output_dir(&config, &progress_tx)?;

        // Generate jobs
        let jobs = generate_jobs(&notes, &velocities, config.round_robin);
        let total_jobs = jobs.len();

        let _ = progress_tx.send(ProgressUpdate::Started {
            total_samples: total_jobs,
        });

        // Ring buffer setup
        let max_duration_ms = config.preroll_ms + config.hold_ms + config.tail_ms + 1000;
        let ring_size =
            (sample_rate as usize * channels as usize * max_duration_ms as usize) / 1000;
        let mut ring = RingBuffer::new(ring_size * 2);

        let mut session = SessionMetadata {
            config: config.clone(),
            samples: Vec::new(),
            timestamp: chrono::Utc::now().to_rfc3339(),
        };

        // Main sampling loop
        let mut warned_about_silent_input = false;
        for (idx, job) in jobs.iter().enumerate() {
            if is_cancelled() {
                let _ = progress_tx.send(ProgressUpdate::Cancelled);
                break;
            }

            let _ = progress_tx.send(ProgressUpdate::SampleStarted {
                index: idx + 1,
                total: total_jobs,
                note: job.note,
                velocity: job.velocity,
                rr: job.rr_index,
            });

            // Check if file exists and resume mode is enabled
            let wav_path = build_file_path(
                &output_dir,
                &config.prefix,
                config.output_organization,
                job,
                "wav",
                config.round_robin > 1,
            );
            if config.resume && wav_path.exists() {
                let _ = progress_tx.send(ProgressUpdate::SampleSkipped {
                    index: idx + 1,
                    total: total_jobs,
                    path: wav_path.to_string_lossy().to_string(),
                });
                continue;
            }

            // Consume any pending audio packets
            consume_audio_packets(&audio_rx, &mut ring);
            thread::sleep(Duration::from_millis(100));

            // Capture the sample
            match capture_sample(
                &mut midi_conn,
                &audio_rx,
                &mut ring,
                job,
                &config,
                sample_rate,
                channels,
                &is_cancelled,
            ) {
                Ok(samples) => {
                    // Process the audio
                    let processed = process_audio(&samples, &config, channels);
                    let peak_db = get_peak_db(&processed);

                    // Avoid exporting empty/silent captures. This usually means no audio
                    // signal is reaching the selected input device.
                    if peak_db <= -90.0 {
                        if !warned_about_silent_input {
                            warned_about_silent_input = true;
                            let _ = progress_tx.send(ProgressUpdate::Log {
                                level: LogLevel::Error,
                                message: format!(
                                    "No input signal detected on audio device '{}'. Check cabling, input gain/monitoring, and macOS microphone permission for this app/terminal.",
                                    config.audio_in
                                ),
                            });
                        }
                        let _ = progress_tx.send(ProgressUpdate::SampleFailed {
                            index: idx + 1,
                            total: total_jobs,
                            error: "Captured silence (-inf dB). Sample was not exported.".to_string(),
                        });
                        continue;
                    }

                    // Export
                    if let Err(e) = export_sample(
                        &output_dir,
                        &config,
                        job,
                        &processed,
                        sample_rate,
                        channels,
                        needs_mp3,
                        format,
                    ) {
                        let _ = progress_tx.send(ProgressUpdate::SampleFailed {
                            index: idx + 1,
                            total: total_jobs,
                            error: e.to_string(),
                        });
                    } else {
                        session.samples.push(SampleInfo {
                            note: job.note,
                            velocity: job.velocity,
                            rr_index: job.rr_index,
                            path: wav_path.to_string_lossy().to_string(),
                            peak_db,
                        });

                        let _ = progress_tx.send(ProgressUpdate::SampleCompleted {
                            index: idx + 1,
                            total: total_jobs,
                            path: wav_path.to_string_lossy().to_string(),
                            peak_db,
                        });
                    }
                }
                Err(e) => {
                    if is_cancelled() {
                        let _ = progress_tx.send(ProgressUpdate::Cancelled);
                        break;
                    }
                    let _ = progress_tx.send(ProgressUpdate::SampleFailed {
                        index: idx + 1,
                        total: total_jobs,
                        error: e.to_string(),
                    });
                }
            }

            thread::sleep(Duration::from_millis(200));
        }

        // Send all notes off
        let _ = send_all_notes_off(&mut midi_conn);

        // Write session metadata
        let session_path = output_dir.join("session.json");
        let session_json = serde_json::to_string_pretty(&session)?;
        fs::write(&session_path, session_json)?;

        let _ = progress_tx.send(ProgressUpdate::Completed {
            samples_recorded: session.samples.len(),
        });

        Ok(session)
    }
}

fn generate_jobs(notes: &[u8], velocities: &[u8], round_robin: u32) -> Vec<SampleJob> {
    let mut jobs = Vec::new();
    for &note in notes {
        for &velocity in velocities {
            for rr in 1..=round_robin {
                jobs.push(SampleJob {
                    note,
                    velocity,
                    rr_index: rr,
                });
            }
        }
    }
    jobs
}

fn capture_sample(
    midi_conn: &mut midir::MidiOutputConnection,
    receiver: &Receiver<Vec<f32>>,
    ring: &mut RingBuffer,
    job: &SampleJob,
    config: &RunConfig,
    sample_rate: u32,
    channels: u16,
    is_cancelled: &dyn Fn() -> bool,
) -> Result<Vec<f32>> {
    const SILENT_CAPTURE_DB: f32 = -90.0;

    let first_pass = capture_sample_with_trigger(
        midi_conn,
        receiver,
        ring,
        job,
        config,
        sample_rate,
        channels,
        is_cancelled,
        MidiTriggerMode::Channel1,
    )?;
    let first_peak = get_peak_db(&first_pass);
    if first_peak > SILENT_CAPTURE_DB {
        return Ok(first_pass);
    }
    warn!(
        "Captured silence on channel 1 for note {} vel {}; retrying on all MIDI channels",
        job.note, job.velocity
    );

    // Some synths are configured to listen on a non-default MIDI channel.
    // If the first pass is silent, retry by broadcasting on all channels.
    let fallback_pass = capture_sample_with_trigger(
        midi_conn,
        receiver,
        ring,
        job,
        config,
        sample_rate,
        channels,
        is_cancelled,
        MidiTriggerMode::AllChannels,
    )?;

    let fallback_peak = get_peak_db(&fallback_pass);
    if fallback_peak > first_peak {
        Ok(fallback_pass)
    } else {
        Ok(first_pass)
    }
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum MidiTriggerMode {
    Channel1,
    AllChannels,
}

fn send_note_on_with_mode(
    midi_conn: &mut midir::MidiOutputConnection,
    note: u8,
    velocity: u8,
    mode: MidiTriggerMode,
) -> Result<()> {
    match mode {
        MidiTriggerMode::Channel1 => send_note_on(midi_conn, note, velocity),
        MidiTriggerMode::AllChannels => {
            for channel in 0u8..16 {
                send_note_on_channel(midi_conn, note, velocity, channel)?;
            }
            Ok(())
        }
    }
}

fn send_note_off_with_mode(
    midi_conn: &mut midir::MidiOutputConnection,
    note: u8,
    mode: MidiTriggerMode,
) -> Result<()> {
    match mode {
        MidiTriggerMode::Channel1 => send_note_off(midi_conn, note),
        MidiTriggerMode::AllChannels => {
            for channel in 0u8..16 {
                send_note_off_channel(midi_conn, note, channel)?;
            }
            Ok(())
        }
    }
}

fn capture_sample_with_trigger(
    midi_conn: &mut midir::MidiOutputConnection,
    receiver: &Receiver<Vec<f32>>,
    ring: &mut RingBuffer,
    job: &SampleJob,
    config: &RunConfig,
    sample_rate: u32,
    channels: u16,
    is_cancelled: &dyn Fn() -> bool,
    trigger_mode: MidiTriggerMode,
) -> Result<Vec<f32>> {
    let preroll_samples =
        (sample_rate as usize * channels as usize * config.preroll_ms as usize) / 1000;
    let hold_samples =
        (sample_rate as usize * channels as usize * config.hold_ms as usize) / 1000;
    let tail_samples =
        (sample_rate as usize * channels as usize * config.tail_ms as usize) / 1000;
    let total_samples = preroll_samples + hold_samples + tail_samples;

    ring.clear();

    let preroll_start = std::time::Instant::now();
    while preroll_start.elapsed() < Duration::from_millis(config.preroll_ms as u64) {
        if is_cancelled() {
            anyhow::bail!("Capture cancelled");
        }
        consume_audio_packets(receiver, ring);
        thread::sleep(Duration::from_millis(1));
    }

    send_note_on_with_mode(midi_conn, job.note, job.velocity, trigger_mode)?;

    let hold_start = std::time::Instant::now();
    while hold_start.elapsed() < Duration::from_millis(config.hold_ms as u64) {
        if is_cancelled() {
            let _ = send_note_off_with_mode(midi_conn, job.note, trigger_mode);
            let _ = send_all_notes_off(midi_conn);
            anyhow::bail!("Capture cancelled");
        }
        consume_audio_packets(receiver, ring);
        thread::sleep(Duration::from_millis(1));
    }

    send_note_off_with_mode(midi_conn, job.note, trigger_mode)?;

    let tail_start = std::time::Instant::now();
    while tail_start.elapsed() < Duration::from_millis(config.tail_ms as u64) {
        if is_cancelled() {
            let _ = send_all_notes_off(midi_conn);
            anyhow::bail!("Capture cancelled");
        }
        consume_audio_packets(receiver, ring);
        thread::sleep(Duration::from_millis(1));
    }

    let samples = ring.get_last_samples(total_samples);
    Ok(samples)
}

fn process_audio(samples: &[f32], config: &RunConfig, channels: u16) -> Vec<f32> {
    let mut processed = samples.to_vec();

    if let Some(threshold_db) = config.trim_threshold_db {
        let min_tail_samples = (config.sr as usize * channels as usize * 100) / 1000;
        let (trimmed, _, _) =
            trim_silence(&processed, threshold_db, channels as usize, min_tail_samples);
        processed = trimmed;
    }

    if let Some(ref norm_mode) = config.normalize {
        processed = normalize_audio(&processed, norm_mode);
    }

    let fade_samples = (config.sr as usize * 5) / 1000;
    apply_fade(&mut processed, channels as usize, fade_samples);

    processed
}

fn export_sample(
    output_dir: &PathBuf,
    config: &RunConfig,
    job: &SampleJob,
    samples: &[f32],
    sample_rate: u32,
    channels: u16,
    needs_mp3: bool,
    format: OutputFormat,
) -> Result<()> {
    let wav_path = build_file_path(
        output_dir,
        &config.prefix,
        config.output_organization,
        job,
        "wav",
        config.round_robin > 1,
    );

    if let Some(parent) = wav_path.parent() {
        fs::create_dir_all(parent)?;
    }

    let spec = WavSpec {
        channels,
        sample_rate,
        bits_per_sample: 24,
        sample_format: hound::SampleFormat::Int,
    };

    write_wav(&wav_path, samples, spec)?;

    if needs_mp3 {
        let mp3_path = build_file_path(
            output_dir,
            &config.prefix,
            config.output_organization,
            job,
            "mp3",
            config.round_robin > 1,
        );
        let _ = convert_to_mp3(&wav_path, &mp3_path);

        if format == OutputFormat::Mp3 {
            let _ = fs::remove_file(&wav_path);
        }
    }

    Ok(())
}

fn build_file_path(
    output_dir: &PathBuf,
    prefix: &str,
    organization: OutputOrganization,
    job: &SampleJob,
    extension: &str,
    include_round_robin_suffix: bool,
) -> PathBuf {
    let sample_dir = build_sample_dir(output_dir, organization, job);
    let filename = build_sample_filename(prefix, job, extension, include_round_robin_suffix);
    sample_dir.join(filename)
}

fn build_sample_dir(
    output_dir: &PathBuf,
    organization: OutputOrganization,
    job: &SampleJob,
) -> PathBuf {
    let note_name = midi_note_to_name(job.note);
    let note_dir = note_name;

    match organization {
        OutputOrganization::Flat => output_dir.clone(),
        OutputOrganization::ByNote => output_dir.join(note_dir),
    }
}

fn build_sample_filename(
    prefix: &str,
    job: &SampleJob,
    extension: &str,
    include_round_robin_suffix: bool,
) -> String {
    let note_name = midi_note_to_name(job.note);
    if include_round_robin_suffix {
        format!(
            "{}_{}_vel{:03}_rr{:02}.{}",
            prefix, note_name, job.velocity, job.rr_index, extension
        )
    } else {
        format!(
            "{}_{}_vel{:03}.{}",
            prefix, note_name, job.velocity, extension
        )
    }
}

fn midi_note_to_name(note: u8) -> String {
    let names = [
        "C", "Cs", "D", "Ds", "E", "F", "Fs", "G", "Gs", "A", "As", "B",
    ];
    let octave = (note / 12) as i32 - 1;
    let pitch = note % 12;
    format!("{}{}", names[pitch as usize], octave)
}

fn prepare_output_dir(config: &RunConfig, progress_tx: &Sender<ProgressUpdate>) -> Result<PathBuf> {
    let base_output = resolve_base_output(&config.output);
    let preferred_dir = base_output.join(&config.prefix);
    match fs::create_dir_all(&preferred_dir) {
        Ok(_) => Ok(preferred_dir),
        Err(err) if base_output.is_relative() => {
            let fallback_base = user_writable_output_root();
            let fallback_dir = fallback_base.join(&config.prefix);
            fs::create_dir_all(&fallback_dir).with_context(|| {
                format!(
                    "Could not create preferred output dir '{}' or fallback dir '{}'",
                    preferred_dir.display(),
                    fallback_dir.display()
                )
            })?;
            let _ = progress_tx.send(ProgressUpdate::Log {
                level: LogLevel::Warning,
                message: format!(
                    "Could not create output path '{}' ({}). Using '{}' instead.",
                    preferred_dir.display(),
                    err,
                    fallback_dir.display()
                ),
            });
            Ok(fallback_dir)
        }
        Err(err) => Err(err).with_context(|| {
            format!(
                "Could not create output directory '{}'",
                preferred_dir.display()
            )
        }),
    }
}

fn resolve_base_output(config_output: &str) -> PathBuf {
    let trimmed = config_output.trim();
    if trimmed.is_empty() {
        PathBuf::from("./output")
    } else {
        PathBuf::from(trimmed)
    }
}

fn user_writable_output_root() -> PathBuf {
    #[cfg(target_os = "windows")]
    {
        if let Some(local_app_data) = std::env::var_os("LOCALAPPDATA") {
            return PathBuf::from(local_app_data).join("autosample").join("output");
        }
    }

    if let Some(home) = std::env::var_os("HOME") {
        return PathBuf::from(home).join("autosample-output");
    }

    std::env::temp_dir().join("autosample-output")
}