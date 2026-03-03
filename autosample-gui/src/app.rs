use crate::state::{AppState, AudioInputPermissionState};
use crate::ui;
use autosample_core::audio::{find_audio_device, get_audio_devices, start_audio_capture};
use autosample_core::midi::ensure_midi_warmup;
use autosample_core::parse::{parse_notes, parse_velocities};
use autosample_core::{AutosampleEngine, EngineStatus, LogLevel, ProgressUpdate};
use crossbeam_channel::{unbounded, Receiver};
use eframe::egui;
use midir::MidiOutputConnection;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::thread;
use std::time::{Duration, Instant};

const POST_STOP_SETTLE_MS: u64 = 2000;
const METER_FLOOR_DB: f32 = -60.0;
const METER_SMOOTHING_ALPHA: f32 = 0.25;

// Result type sent back from the background audio permission probe thread.
type AudioProbeResult = Result<(), String>;

pub struct AutosampleApp {
    pub state: AppState,

    engine_running: Arc<AtomicBool>,
    engine_cancel: Option<Arc<AtomicBool>>,

    active_midi_conn: Option<MidiOutputConnection>,

    stop_requested: bool,
    restart_blocked_until: Option<Instant>,
    event_rx: Option<Receiver<ProgressUpdate>>,

    // Input level meter
    meter_capture: Option<autosample_core::audio::AudioCapture>,
    meter_rx: Option<Receiver<Vec<f32>>>,
    meter_config_key: Option<(String, u32, u16)>,

    // Deferred startup flags
    initial_scan_done: bool,
    startup_platform_permission_probe_done: bool,
    startup_audio_permission_probe_done: bool,

    // Background audio permission probe
    audio_probe_rx: Option<Receiver<AudioProbeResult>>,
}

impl AutosampleApp {
    pub fn new(cc: &eframe::CreationContext<'_>) -> Self {
        tracing::info!("AutosampleApp::new() called");

        cc.egui_ctx.set_visuals(egui::Visuals::dark());
        egui_extras::install_image_loaders(&cc.egui_ctx);

        // Warm up the MIDI backend in a background thread so the UI thread is
        // never blocked. On Windows/WinMM and macOS/CoreMIDI this call can
        // take a non-trivial amount of time on the first run.
        thread::spawn(|| {
            tracing::info!("MIDI warmup starting in background thread");
            ensure_midi_warmup();
            tracing::info!("MIDI warmup complete");
        });

        tracing::info!("AutosampleApp::new() complete, window should appear now");

        Self {
            state: AppState::default(),
            engine_running: Arc::new(AtomicBool::new(false)),
            engine_cancel: None,
            active_midi_conn: None,
            stop_requested: false,
            restart_blocked_until: None,
            event_rx: None,
            meter_capture: None,
            meter_rx: None,
            meter_config_key: None,
            initial_scan_done: false,
            startup_platform_permission_probe_done: false,
            startup_audio_permission_probe_done: false,
            audio_probe_rx: None,
        }
    }

    // -----------------------------------------------------------------------
    // Audio permission helpers
    // -----------------------------------------------------------------------

    fn audio_permission_platform_hint() -> &'static str {
        #[cfg(target_os = "macos")]
        {
            "Allow microphone access in System Settings → Privacy & Security → Microphone."
        }
        #[cfg(target_os = "windows")]
        {
            "Enable microphone access in Settings → Privacy & security → Microphone."
        }
        #[cfg(all(not(target_os = "macos"), not(target_os = "windows")))]
        {
            "Ensure your desktop audio service and sandbox policies allow microphone input."
        }
    }

    fn audio_permission_menu_status(&self) -> &'static str {
        match self.state.audio_permission_state {
            AudioInputPermissionState::Unknown => "Not checked yet",
            AudioInputPermissionState::Checking => "Checking…",
            AudioInputPermissionState::Granted => "Granted",
            AudioInputPermissionState::Denied(_) => "Blocked",
        }
    }

    /// Spawn a background thread that tries to open the audio device and
    /// returns the result through a channel.  The UI thread polls the channel
    /// every frame and never blocks.
    fn run_audio_permission_probe(&mut self, reason: &str) {
        let audio_in = self.state.config.audio_in.trim().to_string();
        if audio_in.is_empty() {
            tracing::debug!("Audio permission probe skipped: no audio device selected");
            return;
        }

        tracing::info!("Spawning audio permission probe: {}", reason);
        self.state.set_audio_permission_checking();

        let sr = self.state.config.sr;
        let channels = self.state.config.channels;
        let hint = Self::audio_permission_platform_hint();

        let (tx, rx) = unbounded::<AudioProbeResult>();
        self.audio_probe_rx = Some(rx);

        thread::spawn(move || {
            tracing::info!("Audio probe thread: opening device '{}'", audio_in);

            let device = match find_audio_device(&audio_in) {
                Ok(d) => d,
                Err(e) => {
                    let msg = format!("Could not open '{}': {}. {}", audio_in, e, hint);
                    tracing::warn!("{}", msg);
                    let _ = tx.send(Err(msg));
                    return;
                }
            };

            let (dummy_tx, _dummy_rx) = unbounded::<Vec<f32>>();
            match start_audio_capture(device, sr, channels, dummy_tx) {
                Ok(_capture) => {
                    tracing::info!("Audio probe thread: access confirmed");
                    let _ = tx.send(Ok(()));
                }
                Err(e) => {
                    let msg = format!("Stream could not start: {}. {}", e, hint);
                    tracing::warn!("{}", msg);
                    let _ = tx.send(Err(msg));
                }
            }
        });
    }

    /// Startup probe that checks microphone access with the first detected
    /// audio input device. This gives macOS a chance to show the consent prompt
    /// even before a specific device is selected in the UI.
    fn run_platform_startup_permission_probe(&mut self, reason: &str) {
        tracing::info!("Spawning platform startup audio probe: {}", reason);
        self.state.set_audio_permission_checking();

        let hint = Self::audio_permission_platform_hint();
        let sr = self.state.config.sr;
        let channels = self.state.config.channels;
        let (tx, rx) = unbounded::<AudioProbeResult>();
        self.audio_probe_rx = Some(rx);

        thread::spawn(move || {
            let devices = match get_audio_devices() {
                Ok(list) if !list.is_empty() => list,
                Ok(_) => {
                    let msg = format!("No audio input devices detected. {}", hint);
                    tracing::warn!("{}", msg);
                    let _ = tx.send(Err(msg));
                    return;
                }
                Err(e) => {
                    let msg = format!("Audio device enumeration failed: {}. {}", e, hint);
                    tracing::warn!("{}", msg);
                    let _ = tx.send(Err(msg));
                    return;
                }
            };

            let candidate = devices[0].name.clone();
            tracing::info!(
                "Platform startup probe: trying first input device '{}'",
                candidate
            );
            let device = match find_audio_device(&candidate) {
                Ok(d) => d,
                Err(e) => {
                    let msg = format!("Could not open '{}': {}. {}", candidate, e, hint);
                    tracing::warn!("{}", msg);
                    let _ = tx.send(Err(msg));
                    return;
                }
            };

            let (dummy_tx, _dummy_rx) = unbounded::<Vec<f32>>();
            match start_audio_capture(device, sr, channels, dummy_tx) {
                Ok(_capture) => {
                    tracing::info!("Platform startup probe: access confirmed");
                    let _ = tx.send(Ok(()));
                }
                Err(e) => {
                    #[cfg(target_os = "windows")]
                    {
                        let _ = std::process::Command::new("explorer")
                            .arg("ms-settings:privacy-microphone")
                            .spawn();
                    }
                    let msg = format!("Stream could not start: {}. {}", e, hint);
                    tracing::warn!("{}", msg);
                    let _ = tx.send(Err(msg));
                }
            }
        });
    }

    /// Poll the audio probe background thread result.  Call every frame.
    fn poll_audio_probe(&mut self) {
        let rx = match &self.audio_probe_rx {
            Some(r) => r.clone(),
            None => return,
        };

        match rx.try_recv() {
            Ok(Ok(())) => {
                tracing::info!("Audio permission granted");
                self.state.set_audio_permission_granted();
                self.state
                    .add_log(LogLevel::Info, "Audio input access confirmed.".to_string());
                self.audio_probe_rx = None;
            }
            Ok(Err(reason)) => {
                tracing::warn!("Audio permission denied: {}", reason);
                self.state.set_audio_permission_denied(reason.clone());
                self.state.add_log(
                    LogLevel::Warning,
                    format!("Audio permission check failed: {}", reason),
                );
                self.audio_probe_rx = None;
            }
            Err(crossbeam_channel::TryRecvError::Empty) => {
                // Still running — keep polling next frame.
            }
            Err(crossbeam_channel::TryRecvError::Disconnected) => {
                tracing::warn!("Audio probe thread disconnected without sending a result");
                self.audio_probe_rx = None;
            }
        }
    }

    // -----------------------------------------------------------------------
    // Input level meter
    // -----------------------------------------------------------------------

    fn desired_meter_key(&self) -> Option<(String, u32, u16)> {
        let audio_in = self.state.config.audio_in.trim();
        if audio_in.is_empty() {
            None
        } else {
            Some((
                audio_in.to_string(),
                self.state.config.sr,
                self.state.config.channels,
            ))
        }
    }

    fn stop_input_meter(&mut self) {
        self.meter_capture = None;
        self.meter_rx = None;
        self.meter_config_key = None;
        self.state.input_meter_db = None;
    }

    fn start_input_meter(&mut self, audio_in: &str, sr: u32, channels: u16) {
        let device = match find_audio_device(audio_in) {
            Ok(d) => d,
            Err(e) => {
                tracing::warn!("Input meter could not open '{}': {}", audio_in, e);
                self.stop_input_meter();
                return;
            }
        };

        let (tx, rx) = unbounded();
        match start_audio_capture(device, sr, channels, tx) {
            Ok(capture) => {
                self.meter_capture = Some(capture);
                self.meter_rx = Some(rx);
                self.meter_config_key = Some((audio_in.to_string(), sr, channels));
                self.state.input_meter_db = None;
            }
            Err(e) => {
                tracing::warn!("Input meter stream failed: {}", e);
                self.stop_input_meter();
            }
        }
    }

    fn ensure_input_meter(&mut self) {
        if self.engine_running.load(Ordering::SeqCst) {
            self.stop_input_meter();
            return;
        }
        if !self.state.audio_permission_is_granted() {
            self.stop_input_meter();
            return;
        }

        let desired = self.desired_meter_key();
        if desired.is_none() {
            self.stop_input_meter();
            return;
        }
        if self.meter_config_key == desired {
            return;
        }

        self.stop_input_meter();
        if let Some((audio_in, sr, channels)) = desired {
            self.start_input_meter(&audio_in, sr, channels);
        }
    }

    fn poll_input_meter(&mut self) {
        let rx = match &self.meter_rx {
            Some(r) => r.clone(),
            None => {
                self.state.input_meter_db = None;
                return;
            }
        };

        let mut peak = 0.0f32;
        let mut saw_packet = false;
        while let Ok(packet) = rx.try_recv() {
            saw_packet = true;
            for s in packet {
                peak = peak.max(s.abs());
            }
        }

        if !saw_packet {
            return;
        }

        let instant_db = if peak <= 1e-7 {
            METER_FLOOR_DB
        } else {
            (20.0 * peak.log10()).clamp(METER_FLOOR_DB, 0.0)
        };

        let smoothed = match self.state.input_meter_db {
            Some(prev) => {
                (prev * (1.0 - METER_SMOOTHING_ALPHA)) + (instant_db * METER_SMOOTHING_ALPHA)
            }
            None => instant_db,
        };
        self.state.input_meter_db = Some(smoothed);
    }

    // -----------------------------------------------------------------------
    // Session control
    // -----------------------------------------------------------------------

    pub fn start_session(&mut self) {
        if self.engine_running.load(Ordering::SeqCst) {
            return;
        }
        if self.state.is_device_scan_running() {
            self.state.add_log(
                LogLevel::Warning,
                "Start blocked: device refresh is still running. \
                 Wait until scanning completes."
                    .to_string(),
            );
            return;
        }
        if !self.state.audio_permission_is_granted() {
            self.state.add_log(
                LogLevel::Error,
                format!(
                    "Start blocked: audio input permission is not granted. {}",
                    Self::audio_permission_platform_hint()
                ),
            );
            return;
        }
        if self.stop_requested {
            self.state.add_log(
                LogLevel::Warning,
                "Start blocked: stop is still in progress. \
                 Wait until teardown completes."
                    .to_string(),
            );
            return;
        }
        if let Some(blocked_until) = self.restart_blocked_until {
            if Instant::now() < blocked_until {
                let remaining = blocked_until.duration_since(Instant::now());
                self.state.add_log(
                    LogLevel::Warning,
                    format!(
                        "Start blocked: waiting {:.1}s for MIDI backend to settle after Stop.",
                        remaining.as_secs_f32()
                    ),
                );
                return;
            }
            self.restart_blocked_until = None;
        }

        // Validate note/velocity expressions.
        let notes = match parse_notes(&self.state.config.notes) {
            Ok(notes) if !notes.is_empty() => notes,
            Ok(_) => {
                self.state.add_log(
                    LogLevel::Error,
                    "Start failed: note selection resolved to zero notes.".to_string(),
                );
                return;
            }
            Err(e) => {
                self.state.add_log(
                    LogLevel::Error,
                    format!("Start failed: invalid note range/list: {}", e),
                );
                return;
            }
        };

        let velocities = match parse_velocities(&self.state.config.vel) {
            Ok(v) if !v.is_empty() => v,
            Ok(_) => {
                self.state.add_log(
                    LogLevel::Error,
                    "Start failed: velocity selection resolved to zero layers.".to_string(),
                );
                return;
            }
            Err(e) => {
                self.state.add_log(
                    LogLevel::Error,
                    format!("Start failed: invalid velocity layers: {}", e),
                );
                return;
            }
        };

        self.state.add_log(
            LogLevel::Info,
            format!(
                "Validated session: {} note(s), {} velocity layer(s).",
                notes.len(),
                velocities.len()
            ),
        );

        // Open MIDI connection.
        let midi_target = self.state.config.midi_out.clone();
        self.state.add_log(
            LogLevel::Info,
            format!("Opening MIDI connection for '{}'…", midi_target),
        );

        let (midi_conn, connected_port_name, available_ports) =
            match autosample_core::midi::connect_midi_output_by_name(&midi_target) {
                Ok(triple) => triple,
                Err(e) => {
                    self.state.add_log(
                        LogLevel::Error,
                        format!(
                            "Start failed: MIDI connection failed for '{}':\n{:#}",
                            midi_target, e
                        ),
                    );
                    return;
                }
            };

        self.state.add_log(
            LogLevel::Info,
            format!("MIDI connected: {}", connected_port_name),
        );

        // Release meter stream so the engine can own the audio device.
        self.stop_input_meter();

        // Launch engine thread.
        let (tx, rx) = unbounded();
        self.event_rx = Some(rx);
        self.engine_running.store(true, Ordering::SeqCst);

        let engine_cancel = Arc::new(AtomicBool::new(false));
        self.engine_cancel = Some(engine_cancel.clone());

        let config = self.state.config.clone();
        let running = self.engine_running.clone();
        let tx_errors = tx.clone();

        thread::spawn(move || {
            tracing::info!("Engine thread started");
            let mut engine = AutosampleEngine::new();
            if let Err(e) = engine.run_with_connected_midi_and_cancel(
                config,
                tx,
                midi_conn,
                connected_port_name,
                available_ports,
                engine_cancel,
            ) {
                let _ = tx_errors.send(ProgressUpdate::Log {
                    level: LogLevel::Error,
                    message: format!("Run failed:\n{:#}", e),
                });
                let _ = tx_errors.send(ProgressUpdate::Cancelled);
            }
            running.store(false, Ordering::SeqCst);
            tracing::info!("Engine thread finished");
        });

        self.state.engine_status = EngineStatus::Running;
    }

    pub fn stop_session(&mut self) {
        if let Some(cancel_flag) = &self.engine_cancel {
            cancel_flag.store(true, Ordering::SeqCst);
        }

        self.stop_requested = true;
        self.restart_blocked_until =
            Some(Instant::now() + Duration::from_millis(POST_STOP_SETTLE_MS));

        self.state.add_log(
            LogLevel::Info,
            "Stop requested: cancelling active sampling loop.".to_string(),
        );

        if !self.engine_running.load(Ordering::SeqCst) {
            self.stop_requested = false;
        }
    }

    // -----------------------------------------------------------------------
    // Emergency All-Notes-Off
    // -----------------------------------------------------------------------

    fn send_all_notes_off_best_effort(&mut self, reason: &str) {
        if self.engine_running.load(Ordering::SeqCst) {
            if let Some(flag) = &self.engine_cancel {
                flag.store(true, Ordering::SeqCst);
            }
            return;
        }

        let midi_target = self.state.config.midi_out.trim().to_string();
        if midi_target.is_empty() {
            return;
        }

        self.state.add_log(
            LogLevel::Info,
            format!("Sending All Notes Off for '{}' ({})", midi_target, reason),
        );

        if let Some(conn) = self.active_midi_conn.as_mut() {
            match autosample_core::midi::send_all_notes_off(conn) {
                Ok(_) => {
                    self.state.add_log(
                        LogLevel::Info,
                        format!("All Notes Off sent via active connection ({})", reason),
                    );
                    return;
                }
                Err(e) => {
                    self.state.add_log(
                        LogLevel::Warning,
                        format!("All Notes Off via active connection failed: {}", e),
                    );
                }
            }
        }

        match autosample_core::midi::connect_midi_output_by_name(&midi_target) {
            Ok((mut conn, port_name, _)) => {
                if let Err(e) = autosample_core::midi::send_all_notes_off(&mut conn) {
                    self.state.add_log(
                        LogLevel::Warning,
                        format!("All Notes Off failed on '{}': {}", port_name, e),
                    );
                } else {
                    self.state.add_log(
                        LogLevel::Info,
                        format!("All Notes Off sent to '{}'", port_name),
                    );
                }
                self.active_midi_conn = Some(conn);
            }
            Err(e) => {
                self.state.add_log(
                    LogLevel::Warning,
                    format!(
                        "Could not open MIDI output '{}' for All Notes Off: {:#}",
                        midi_target, e
                    ),
                );
            }
        }
    }
}

// ---------------------------------------------------------------------------
// Drop
// ---------------------------------------------------------------------------

impl Drop for AutosampleApp {
    fn drop(&mut self) {
        self.send_all_notes_off_best_effort("application shutdown");
    }
}

// ---------------------------------------------------------------------------
// eframe::App
// ---------------------------------------------------------------------------

impl eframe::App for AutosampleApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        // ── Deferred startup ────────────────────────────────────────────────
        // Trigger device scan after the very first frame so the window is
        // already visible and responsive before we touch MIDI/audio APIs.
        if !self.initial_scan_done {
            self.initial_scan_done = true;
            tracing::info!("First frame rendered — requesting device scan");
            self.state.request_device_scan();
            ctx.request_repaint();
        }

        // ── Poll background tasks ───────────────────────────────────────────
        self.state.poll_device_scan_result();
        self.poll_audio_probe();

        // ── Audio permission recheck ────────────────────────────────────────
        if self.state.consume_audio_permission_recheck_requested() {
            self.state.reset_audio_permission_state();
            self.startup_platform_permission_probe_done = false;
            self.startup_audio_permission_probe_done = false;
            self.state.add_log(
                LogLevel::Info,
                "Retrying audio permission check…".to_string(),
            );
        }

        // Platform-level preflight microphone check.
        if !self.startup_platform_permission_probe_done
            && !self.state.is_device_scan_running()
            && self.audio_probe_rx.is_none()
        {
            self.startup_platform_permission_probe_done = true;
            self.run_platform_startup_permission_probe("startup platform check");
        }

        // ── Startup audio permission probe ──────────────────────────────────
        // Only attempt once per session, after the device scan completes, and
        // only when there is no probe already in flight.
        if !self.startup_audio_permission_probe_done
            && !self.state.is_device_scan_running()
            && !self.state.config.audio_in.trim().is_empty()
            && self.audio_probe_rx.is_none()
        {
            self.startup_audio_permission_probe_done = true;
            self.run_audio_permission_probe("startup check");
        }

        // ── Input level meter ────────────────────────────────────────────────
        self.ensure_input_meter();
        self.poll_input_meter();

        // ── Engine completion detection ──────────────────────────────────────
        if !self.engine_running.load(Ordering::SeqCst) {
            if self.engine_cancel.is_some() {
                self.engine_cancel = None;
            }
            if self.stop_requested {
                self.stop_requested = false;
                self.state.add_log(
                    LogLevel::Info,
                    "Stop complete: sampling loop terminated.".to_string(),
                );
            }
        }

        // ── Drain engine event channel ───────────────────────────────────────
        if let Some(rx) = &self.event_rx {
            while let Ok(event) = rx.try_recv() {
                self.state.handle_engine_event(event);
            }
        }

        // ── Repaint scheduling ───────────────────────────────────────────────
        if self.state.engine_status == EngineStatus::Running {
            ctx.request_repaint();
        } else {
            if self.state.input_meter_db.is_some() {
                ctx.request_repaint_after(Duration::from_millis(60));
            }
            if self.state.is_device_scan_running() {
                ctx.request_repaint_after(Duration::from_millis(100));
            }
            if self.audio_probe_rx.is_some() {
                ctx.request_repaint_after(Duration::from_millis(100));
            }
        }

        // ── Menu bar ─────────────────────────────────────────────────────────
        egui::TopBottomPanel::top("menu_bar").show(ctx, |ui| {
            egui::menu::bar(ui, |ui| {
                ui.menu_button("File", |ui| {
                    if ui.button("Load Preset…").clicked() {
                        if let Some(path) = rfd::FileDialog::new()
                            .add_filter("JSON", &["json"])
                            .pick_file()
                        {
                            match self.state.load_preset(&path.display().to_string()) {
                                Ok(_) => self.state.add_log(
                                    LogLevel::Info,
                                    format!("Loaded preset from {}", path.display()),
                                ),
                                Err(e) => self.state.add_log(
                                    LogLevel::Error,
                                    format!("Failed to load preset: {}", e),
                                ),
                            }
                        }
                        ui.close_menu();
                    }

                    if ui.button("Save Preset…").clicked() {
                        if let Some(path) = rfd::FileDialog::new()
                            .add_filter("JSON", &["json"])
                            .save_file()
                        {
                            let mut path_str = path.display().to_string();
                            if !path_str.ends_with(".json") {
                                path_str.push_str(".json");
                            }
                            match self.state.save_preset(&path_str) {
                                Ok(_) => self.state.add_log(
                                    LogLevel::Info,
                                    format!("Saved preset to {}", path_str),
                                ),
                                Err(e) => self.state.add_log(
                                    LogLevel::Error,
                                    format!("Failed to save preset: {}", e),
                                ),
                            }
                        }
                        ui.close_menu();
                    }

                    ui.separator();
                    ui.label(egui::RichText::new("Audio Input Permission").strong());
                    ui.label(format!(
                        "Status: {}",
                        self.audio_permission_menu_status()
                    ));

                    if let AudioInputPermissionState::Denied(reason) =
                        &self.state.audio_permission_state
                    {
                        ui.label(
                            egui::RichText::new(reason).color(egui::Color32::YELLOW),
                        );
                    }

                    if ui.button("Check Audio Permission").clicked() {
                        self.state.request_audio_permission_recheck();
                        ui.close_menu();
                    }

                    ui.separator();

                    if ui.button("Quit").clicked() {
                        self.send_all_notes_off_best_effort("quit command");
                        ctx.send_viewport_cmd(egui::ViewportCommand::Close);
                    }
                });

                ui.menu_button("Help", |ui| {
                    if ui.button("About").clicked() {
                        ui.close_menu();
                    }
                });
            });
        });

        // ── Main UI ──────────────────────────────────────────────────────────
        if let Some(cmd) = ui::single_screen::show(ctx, &mut self.state) {
            match cmd {
                ui::progress::RunCommand::Start => self.start_session(),
                ui::progress::RunCommand::Stop => self.stop_session(),
                ui::progress::RunCommand::ClearLogs => self.state.logs.clear(),
                ui::progress::RunCommand::ClearProject => {
                    if self.engine_running.load(Ordering::SeqCst) || self.stop_requested {
                        self.state.add_log(
                            LogLevel::Warning,
                            "Clear Project blocked while run teardown is in progress."
                                .to_string(),
                        );
                    } else {
                        self.state.clear_project();
                    }
                }
            }
        }
    }
}