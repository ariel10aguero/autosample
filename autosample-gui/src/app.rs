use crate::state::{AppState, AudioInputPermissionState};
use crate::ui;
use autosample_core::audio::{find_audio_device, start_audio_capture, AudioCapture};
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

// How long after a Stop we refuse to allow a new Start.  Gives CoreMIDI time
// to fully release the previous connection before we open a new one.
const POST_STOP_SETTLE_MS: u64 = 2000;
const METER_FLOOR_DB: f32 = -60.0;
const METER_SMOOTHING_ALPHA: f32 = 0.25;

pub struct AutosampleApp {
    pub state: AppState,

    engine_running: Arc<AtomicBool>,
    engine_cancel: Option<Arc<AtomicBool>>,

    /// Active MIDI connection that is being used (or was last used) by the
    /// engine.  Keeping it here means we can send All-Notes-Off through the
    /// *same* connection without opening a new CoreMIDI client.
    active_midi_conn: Option<MidiOutputConnection>,

    stop_requested: bool,
    restart_blocked_until: Option<Instant>,
    event_rx: Option<Receiver<ProgressUpdate>>,
    meter_capture: Option<AudioCapture>,
    meter_rx: Option<Receiver<Vec<f32>>>,
    meter_config_key: Option<(String, u32, u16)>,
    startup_audio_permission_probe_done: bool,
}

impl AutosampleApp {
    pub fn new(cc: &eframe::CreationContext<'_>) -> Self {
        cc.egui_ctx.set_visuals(egui::Visuals::dark());
        egui_extras::install_image_loaders(&cc.egui_ctx);

        // Pre-warm the CoreMIDI session as early as possible so the daemon
        // has time to fully initialize before the user clicks Start.
        ensure_midi_warmup();

        let mut state = AppState::default();
        // Scan happens after warmup so get_midi_ports() hits a live session.
        state.request_device_scan();

        Self {
            state,
            engine_running: Arc::new(AtomicBool::new(false)),
            engine_cancel: None,
            active_midi_conn: None,
            stop_requested: false,
            restart_blocked_until: None,
            event_rx: None,
            meter_capture: None,
            meter_rx: None,
            meter_config_key: None,
            startup_audio_permission_probe_done: false,
        }
    }

    fn audio_permission_platform_hint() -> &'static str {
        #[cfg(target_os = "macos")]
        {
            "Allow microphone access in System Settings -> Privacy & Security -> Microphone."
        }
        #[cfg(target_os = "windows")]
        {
            "Enable microphone access in Settings -> Privacy & security -> Microphone."
        }
        #[cfg(all(not(target_os = "macos"), not(target_os = "windows")))]
        {
            "Ensure your desktop audio service and app sandbox policies allow microphone input."
        }
    }

    fn audio_permission_menu_status(&self) -> &'static str {
        match self.state.audio_permission_state {
            AudioInputPermissionState::Unknown => "Not checked yet",
            AudioInputPermissionState::Checking => "Checking...",
            AudioInputPermissionState::Granted => "Granted",
            AudioInputPermissionState::Denied(_) => "Blocked",
        }
    }

    fn run_audio_permission_probe(&mut self, reason: &str) {
        let audio_in = self.state.config.audio_in.trim().to_string();
        if audio_in.is_empty() {
            return;
        }

        self.state.set_audio_permission_checking();
        let device = match find_audio_device(&audio_in) {
            Ok(device) => device,
            Err(err) => {
                let reason = format!(
                    "Could not open selected input device '{}': {}. {}",
                    audio_in,
                    err,
                    Self::audio_permission_platform_hint()
                );
                self.state.set_audio_permission_denied(reason.clone());
                self.state.add_log(
                    LogLevel::Warning,
                    format!("Audio permission check failed: {}", reason),
                );
                return;
            }
        };

        // Opening a short-lived stream here triggers first-run permission prompts
        // where the OS requires explicit microphone consent.
        let (tx, _rx) = unbounded();
        match start_audio_capture(
            device,
            self.state.config.sr,
            self.state.config.channels,
            tx,
        ) {
            Ok(_capture) => {
                self.state.set_audio_permission_granted();
                self.state.add_log(
                    LogLevel::Info,
                    format!("Audio input access confirmed ({}).", reason),
                );
            }
            Err(err) => {
                let reason = format!(
                    "Audio input stream could not start: {}. {}",
                    err,
                    Self::audio_permission_platform_hint()
                );
                self.state.set_audio_permission_denied(reason.clone());
                self.state.add_log(
                    LogLevel::Warning,
                    format!("Audio permission check failed: {}", reason),
                );
            }
        }
    }

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
            Ok(device) => device,
            Err(err) => {
                self.state.add_log(
                    LogLevel::Warning,
                    format!("Input meter could not open audio device '{}': {}", audio_in, err),
                );
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
            Err(err) => {
                self.state.add_log(
                    LogLevel::Warning,
                    format!("Input meter could not start stream: {}", err),
                );
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
        let Some(rx) = &self.meter_rx else {
            self.state.input_meter_db = None;
            return;
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
            Some(prev) => (prev * (1.0 - METER_SMOOTHING_ALPHA)) + (instant_db * METER_SMOOTHING_ALPHA),
            None => instant_db,
        };
        self.state.input_meter_db = Some(smoothed);
    }

    // -----------------------------------------------------------------------
    // Session control
    // -----------------------------------------------------------------------

    pub fn start_session(&mut self) {
        // --- guards --------------------------------------------------------
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

        // Validate note/velocity expressions before touching MIDI backend.
        let notes = match parse_notes(&self.state.config.notes) {
            Ok(notes) if !notes.is_empty() => notes,
            Ok(_) => {
                self.state.add_log(
                    LogLevel::Error,
                    "Start failed: note selection resolved to zero notes.".to_string(),
                );
                return;
            }
            Err(error) => {
                self.state.add_log(
                    LogLevel::Error,
                    format!("Start failed: invalid note range/list: {}", error),
                );
                return;
            }
        };
        let velocities = match parse_velocities(&self.state.config.vel) {
            Ok(velocities) if !velocities.is_empty() => velocities,
            Ok(_) => {
                self.state.add_log(
                    LogLevel::Error,
                    "Start failed: velocity selection resolved to zero layers.".to_string(),
                );
                return;
            }
            Err(error) => {
                self.state.add_log(
                    LogLevel::Error,
                    format!("Start failed: invalid velocity layers: {}", error),
                );
                return;
            }
        };
        self.state.add_log(
            LogLevel::Info,
            format!(
                "Validated session input: {} note(s), {} velocity layer(s).",
                notes.len(),
                velocities.len()
            ),
        );

        // --- MIDI connection -----------------------------------------------
        let midi_target = self.state.config.midi_out.clone();
        self.state.add_log(
            LogLevel::Info,
            format!("Opening MIDI connection for '{}'…", midi_target),
        );

        let (midi_conn, connected_port_name, available_ports) =
            match autosample_core::midi::connect_midi_output_by_name(&midi_target) {
                Ok(triple) => triple,
                Err(error) => {
                    self.state.add_log(
                        LogLevel::Error,
                        format!(
                            "Start failed: MIDI init/connection failed for '{}':\n{:#}",
                            midi_target, error
                        ),
                    );
                    return;
                }
            };

        self.state.add_log(
            LogLevel::Info,
            format!("MIDI connected: {}", connected_port_name),
        );

        // Release meter stream so the sampling engine can own the device.
        self.stop_input_meter();

        // --- launch engine thread ------------------------------------------
        let (tx, rx) = unbounded();
        self.event_rx = Some(rx);
        self.engine_running.store(true, Ordering::SeqCst);

        let engine_cancel = Arc::new(AtomicBool::new(false));
        self.engine_cancel = Some(engine_cancel.clone());

        // We hand the connection to the engine thread.  It will be returned
        // (or dropped) when the thread finishes.
        let config = self.state.config.clone();
        let running = self.engine_running.clone();
        let tx_for_errors = tx.clone();

        thread::spawn(move || {
            let mut engine = AutosampleEngine::new();
            if let Err(e) = engine.run_with_connected_midi_and_cancel(
                config,
                tx,
                midi_conn,
                connected_port_name,
                available_ports,
                engine_cancel,
            ) {
                let _ = tx_for_errors.send(ProgressUpdate::Log {
                    level: LogLevel::Error,
                    message: format!("Run failed:\n{:#}", e),
                });
                let _ = tx_for_errors.send(ProgressUpdate::Cancelled);
            }
            running.store(false, Ordering::SeqCst);
        });

        self.state.engine_status = EngineStatus::Running;
    }

    pub fn stop_session(&mut self) {
        // Signal the engine to cancel; it will send All-Notes-Off itself
        // before returning, so we do NOT open a second MIDI connection here.
        if let Some(cancel_flag) = &self.engine_cancel {
            cancel_flag.store(true, Ordering::SeqCst);
        }

        self.stop_requested = true;

        // Give CoreMIDI time to settle before the user can click Start again.
        self.restart_blocked_until =
            Some(Instant::now() + Duration::from_millis(POST_STOP_SETTLE_MS));

        self.state.add_log(
            LogLevel::Info,
            "Stop requested: cancelling active sampling loop.".to_string(),
        );

        // If the engine wasn't running there is nothing to wait for.
        if !self.engine_running.load(Ordering::SeqCst) {
            self.stop_requested = false;
        }
    }

    // -----------------------------------------------------------------------
    // Emergency All-Notes-Off
    //
    // Only opens a new MIDI connection when we have no other choice (i.e. on
    // Drop / Quit when the engine is not running).  If the engine is still
    // live the cancel flag is sufficient — the engine sends note-off itself.
    // -----------------------------------------------------------------------

    fn send_all_notes_off_best_effort(&mut self, reason: &str) {
        // If the engine is still running, cancelling it is enough.
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
            format!(
                "Sending All Notes Off for '{}' ({})",
                midi_target, reason
            ),
        );

        // Re-use active connection when available to avoid a fresh CoreMIDI
        // client init cycle.
        if let Some(conn) = self.active_midi_conn.as_mut() {
            match autosample_core::midi::send_all_notes_off(conn) {
                Ok(_) => {
                    self.state.add_log(
                        LogLevel::Info,
                        format!("All Notes Off sent via active connection ({})", reason),
                    );
                    return;
                }
                Err(err) => {
                    self.state.add_log(
                        LogLevel::Warning,
                        format!("All Notes Off via active connection failed: {}", err),
                    );
                    // Fall through to open a fresh connection below.
                }
            }
        }

        // Last resort: open a brand-new connection.
        match autosample_core::midi::connect_midi_output_by_name(&midi_target) {
            Ok((mut conn, port_name, _)) => {
                if let Err(err) = autosample_core::midi::send_all_notes_off(&mut conn) {
                    self.state.add_log(
                        LogLevel::Warning,
                        format!("All Notes Off failed on '{}': {}", port_name, err),
                    );
                } else {
                    self.state.add_log(
                        LogLevel::Info,
                        format!("All Notes Off sent to '{}'", port_name),
                    );
                }
                // Keep the freshly opened connection alive to avoid
                // immediately stressing the backend on the next operation.
                self.active_midi_conn = Some(conn);
            }
            Err(err) => {
                self.state.add_log(
                    LogLevel::Warning,
                    format!(
                        "Could not open MIDI output '{}' for All Notes Off: {:#}",
                        midi_target, err
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
        // Poll device scan results.
        self.state.poll_device_scan_result();

        if self.state.consume_audio_permission_recheck_requested() {
            self.state.reset_audio_permission_state();
            self.startup_audio_permission_probe_done = false;
            self.state.add_log(
                LogLevel::Info,
                "Retrying audio permission check…".to_string(),
            );
        }

        if !self.startup_audio_permission_probe_done
            && !self.state.is_device_scan_running()
            && !self.state.config.audio_in.trim().is_empty()
        {
            self.run_audio_permission_probe("startup check");
            self.startup_audio_permission_probe_done = true;
        }
        self.ensure_input_meter();
        self.poll_input_meter();

        // Detect engine thread completion.
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

        // Drain engine event channel.
        if let Some(rx) = &self.event_rx {
            while let Ok(event) = rx.try_recv() {
                self.state.handle_engine_event(event);
            }
        }

        // Repaint scheduling.
        if self.state.engine_status == EngineStatus::Running {
            ctx.request_repaint();
        } else if self.state.input_meter_db.is_some() {
            ctx.request_repaint_after(Duration::from_millis(60));
        }
        if self.state.is_device_scan_running() {
            ctx.request_repaint_after(Duration::from_millis(100));
        }

        // -------------------------------------------------------------------
        // Menu bar
        // -------------------------------------------------------------------
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
                    ui.label(format!("Status: {}", self.audio_permission_menu_status()));

                    if let AudioInputPermissionState::Denied(reason) =
                        &self.state.audio_permission_state
                    {
                        ui.label(reason);
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

        // -------------------------------------------------------------------
        // Main UI
        // -------------------------------------------------------------------
        if let Some(cmd) = ui::single_screen::show(ctx, &mut self.state) {
            match cmd {
                ui::progress::RunCommand::Start => self.start_session(),
                ui::progress::RunCommand::Stop => self.stop_session(),
                ui::progress::RunCommand::ClearLogs => self.state.logs.clear(),
                ui::progress::RunCommand::ClearProject => {
                    if self.engine_running.load(Ordering::SeqCst) || self.stop_requested {
                        self.state.add_log(
                            LogLevel::Warning,
                            "Clear Project blocked while run teardown is in progress.".to_string(),
                        );
                    } else {
                        self.state.clear_project();
                    }
                }
            }
        }
    }
}