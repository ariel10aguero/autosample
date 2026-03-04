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

const POST_STOP_SETTLE_MS: u64 = 2000;
const METER_FLOOR_DB: f32 = -60.0;
const METER_SMOOTHING_ALPHA: f32 = 0.25;

// ---------------------------------------------------------------------------
// Windows microphone permission — called IN-PROCESS so Windows registers
// autosample-gui.exe in Settings → Privacy → Microphone
// ---------------------------------------------------------------------------

#[cfg(target_os = "windows")]
mod windows_permission {
    use windows::{
        Foundation::IAsyncAction,
        Media::Capture::{
            MediaCapture, MediaCaptureInitializationSettings, StreamingCaptureMode,
        },
    };

    #[derive(Debug)]
    pub enum PermissionResult {
        Granted,
        Denied,
        Error(String),
    }

    /// Calls MediaCapture.InitializeAsync() directly inside this process.
    /// This is the ONLY reliable way to:
    ///   1. Register the .exe in Settings → Privacy → Microphone
    ///   2. Trigger the one-time "Allow microphone?" consent dialog
    ///   3. Receive Granted/Denied without spawning a helper process
    pub fn request_microphone_permission() -> PermissionResult {
        // Run on a dedicated thread so we can block without freezing egui.
        // The channel carries the result back.
        let (tx, rx) = std::sync::mpsc::channel();

        std::thread::spawn(move || {
            let result = try_init_media_capture();
            let _ = tx.send(result);
        });

        // Block up to 30 s for the user to respond to the consent dialog.
        match rx.recv_timeout(std::time::Duration::from_secs(30)) {
            Ok(r) => r,
            Err(_) => PermissionResult::Error("Timed out waiting for permission response".into()),
        }
    }

    fn try_init_media_capture() -> PermissionResult {
        // Initialize WinRT for this thread
        unsafe {
            use windows::Win32::System::WinRT::RoInitialize;
            use windows::Win32::System::WinRT::RO_INIT_MULTITHREADED;
            let _ = RoInitialize(RO_INIT_MULTITHREADED);
        }

        let capture = match MediaCapture::new() {
            Ok(c) => c,
            Err(e) => return PermissionResult::Error(format!("MediaCapture::new failed: {}", e)),
        };

        let settings = match MediaCaptureInitializationSettings::new() {
            Ok(s) => s,
            Err(e) => {
                return PermissionResult::Error(format!(
                    "MediaCaptureInitializationSettings::new failed: {}",
                    e
                ))
            }
        };

        if let Err(e) = settings.SetStreamingCaptureMode(StreamingCaptureMode::Audio) {
            return PermissionResult::Error(format!("SetStreamingCaptureMode failed: {}", e));
        }

        let async_op: IAsyncAction = match capture.InitializeWithSettingsAsync(&settings) {
            Ok(op) => op,
            Err(e) => {
                // E_ACCESSDENIED (0x80070005) means the user previously denied
                // or the system policy blocks microphone access.
                if e.code().0 as u32 == 0x80070005 {
                    return PermissionResult::Denied;
                }
                return PermissionResult::Error(format!("InitializeWithSettingsAsync failed: {}", e));
            }
        };

        // Block this worker thread until the async operation completes.
        // The OS will show the consent dialog to the user if needed.
        match async_op.get() {
            Ok(_) => PermissionResult::Granted,
            Err(e) => {
                if e.code().0 as u32 == 0x80070005 {
                    PermissionResult::Denied
                } else {
                    PermissionResult::Error(format!("InitializeAsync failed: {}", e))
                }
            }
        }
    }
}

// ---------------------------------------------------------------------------
// App struct
// ---------------------------------------------------------------------------

pub struct AutosampleApp {
    pub state: AppState,

    engine_running: Arc<AtomicBool>,
    engine_cancel: Option<Arc<AtomicBool>>,
    active_midi_conn: Option<MidiOutputConnection>,

    stop_requested: bool,
    restart_blocked_until: Option<Instant>,
    event_rx: Option<Receiver<ProgressUpdate>>,
    meter_capture: Option<AudioCapture>,
    meter_rx: Option<Receiver<Vec<f32>>>,
    meter_config_key: Option<(String, u32, u16)>,
    startup_audio_permission_probe_done: bool,

    // Windows: permission check runs on a background thread.
    // We poll this channel each frame.
    #[cfg(target_os = "windows")]
    win_permission_rx: Option<std::sync::mpsc::Receiver<windows_permission::PermissionResult>>,
    #[cfg(not(target_os = "windows"))]
    win_permission_rx: Option<()>, // zero-size placeholder on other platforms
}

impl AutosampleApp {
    pub fn new(cc: &eframe::CreationContext<'_>) -> Self {
        cc.egui_ctx.set_visuals(egui::Visuals::dark());
        egui_extras::install_image_loaders(&cc.egui_ctx);

        ensure_midi_warmup();

        let mut state = AppState::default();
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
            win_permission_rx: None,
        }
    }

    // -----------------------------------------------------------------------
    // Permission helpers
    // -----------------------------------------------------------------------

    fn audio_permission_platform_hint() -> &'static str {
        #[cfg(target_os = "macos")]
        {
            "Allow microphone access in System Settings → Privacy & Security → Microphone."
        }
        #[cfg(target_os = "windows")]
        {
            "Click \"Request Microphone Permission\" in the app. \
             If the button doesn't trigger a prompt, open \
             Settings → Privacy & security → Microphone and enable access."
        }
        #[cfg(all(not(target_os = "macos"), not(target_os = "windows")))]
        {
            "Ensure your desktop audio service and app sandbox policies allow microphone input."
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

    /// On Windows: spawns the WinRT permission check on a background thread
    /// and stores the receiver so we can poll it each frame without blocking.
    ///
    /// On other platforms: attempts to open the CPAL device directly.
    fn run_audio_permission_probe(&mut self, reason: &str) {
        let audio_in = self.state.config.audio_in.trim().to_string();
        if audio_in.is_empty() {
            return;
        }

        self.state.set_audio_permission_checking();

        #[cfg(target_os = "windows")]
        {
            // If a check is already in flight, don't start another.
            if self.win_permission_rx.is_some() {
                return;
            }

            self.state.add_log(
                LogLevel::Info,
                format!(
                    "Requesting Windows microphone permission ({}). \
                     A system dialog may appear…",
                    reason
                ),
            );

            let (tx, rx) = std::sync::mpsc::channel();
            self.win_permission_rx = Some(rx);

            // Spawn so egui keeps rendering while the user responds to the dialog.
            std::thread::spawn(move || {
                let result = windows_permission::request_microphone_permission();
                let _ = tx.send(result);
            });

            // Result is handled in poll_windows_permission() called each frame.
            return;
        }

        // ── Non-Windows: probe directly ────────────────────────────────────
        #[allow(unreachable_code)]
        self.probe_cpal_device(&audio_in, reason);
    }

    fn probe_cpal_device(&mut self, audio_in: &str, reason: &str) {
        let device = match find_audio_device(audio_in) {
            Ok(d) => d,
            Err(err) => {
                let msg = format!(
                    "Could not open input device '{}': {}. {}",
                    audio_in,
                    err,
                    Self::audio_permission_platform_hint()
                );
                self.state.set_audio_permission_denied(msg.clone());
                self.state
                    .add_log(LogLevel::Warning, format!("Audio permission check failed: {}", msg));
                return;
            }
        };

        let (tx, _rx) = unbounded();
        match start_audio_capture(device, self.state.config.sr, self.state.config.channels, tx) {
            Ok(_capture) => {
                self.state.set_audio_permission_granted();
                self.state.add_log(
                    LogLevel::Info,
                    format!("Audio input access confirmed ({}).", reason),
                );
            }
            Err(err) => {
                let msg = format!(
                    "Audio input stream could not start: {}. {}",
                    err,
                    Self::audio_permission_platform_hint()
                );
                self.state.set_audio_permission_denied(msg.clone());
                self.state
                    .add_log(LogLevel::Warning, format!("Audio permission check failed: {}", msg));
            }
        }
    }

    /// Called every frame on Windows to check if the background WinRT
    /// permission request has completed.
    #[cfg(target_os = "windows")]
    fn poll_windows_permission(&mut self) {
        use std::sync::mpsc::TryRecvError;

        let done = match &self.win_permission_rx {
            None => return,
            Some(rx) => match rx.try_recv() {
                Ok(result) => Some(result),
                Err(TryRecvError::Empty) => return,   // still waiting
                Err(TryRecvError::Disconnected) => {
                    Some(windows_permission::PermissionResult::Error(
                        "Permission thread disconnected".into(),
                    ))
                }
            },
        };

        self.win_permission_rx = None;

        match done {
            None => {}
            Some(windows_permission::PermissionResult::Granted) => {
                self.state.add_log(
                    LogLevel::Info,
                    "Windows microphone permission granted. Verifying device…".to_string(),
                );
                // Now confirm with CPAL that the device actually opens.
                let audio_in = self.state.config.audio_in.trim().to_string();
                if !audio_in.is_empty() {
                    self.probe_cpal_device(&audio_in, "post-WinRT grant check");
                } else {
                    self.state.set_audio_permission_granted();
                }
            }
            Some(windows_permission::PermissionResult::Denied) => {
                let msg = format!(
                    "Windows denied microphone access. {}",
                    Self::audio_permission_platform_hint()
                );
                self.state.set_audio_permission_denied(msg.clone());
                self.state.add_log(LogLevel::Warning, msg);
            }
            Some(windows_permission::PermissionResult::Error(e)) => {
                self.state.add_log(
                    LogLevel::Info,
                    format!(
                        "WinRT permission check could not complete ({}). \
                         Falling back to direct device open…",
                        e
                    ),
                );
                // Fall back to CPAL probe — better than nothing.
                let audio_in = self.state.config.audio_in.trim().to_string();
                if !audio_in.is_empty() {
                    self.probe_cpal_device(&audio_in, "WinRT fallback");
                }
            }
        }
    }

    #[cfg(not(target_os = "windows"))]
    fn poll_windows_permission(&mut self) {
        // no-op on non-Windows
    }

    // -----------------------------------------------------------------------
    // Input meter
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
                "Start blocked: device refresh is still running.".to_string(),
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
                "Start blocked: stop is still in progress.".to_string(),
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

        let notes = match parse_notes(&self.state.config.notes) {
            Ok(n) if !n.is_empty() => n,
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
                "Validated: {} note(s), {} velocity layer(s).",
                notes.len(),
                velocities.len()
            ),
        );

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
                        format!("Start failed: MIDI connection failed: {:#}", e),
                    );
                    return;
                }
            };

        self.state.add_log(
            LogLevel::Info,
            format!("MIDI connected: {}", connected_port_name),
        );

        self.stop_input_meter();

        let (tx, rx) = unbounded();
        self.event_rx = Some(rx);
        self.engine_running.store(true, Ordering::SeqCst);

        let engine_cancel = Arc::new(AtomicBool::new(false));
        self.engine_cancel = Some(engine_cancel.clone());

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
                Err(err) => {
                    self.state.add_log(
                        LogLevel::Warning,
                        format!("All Notes Off via active connection failed: {}", err),
                    );
                }
            }
        }

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
                self.active_midi_conn = Some(conn);
            }
            Err(err) => {
                self.state.add_log(
                    LogLevel::Warning,
                    format!("Could not open MIDI output for All Notes Off: {:#}", err),
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
        // Poll the async Windows permission check result first.
        self.poll_windows_permission();

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

        if let Some(rx) = &self.event_rx {
            while let Ok(event) = rx.try_recv() {
                self.state.handle_engine_event(event);
            }
        }

        // Repaint scheduling — keep ticking while permission check is in flight.
        #[cfg(target_os = "windows")]
        if self.win_permission_rx.is_some() {
            ctx.request_repaint_after(Duration::from_millis(100));
        }

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
                        ui.label(
                            egui::RichText::new(reason)
                                .color(egui::Color32::RED)
                                .small(),
                        );
                    }

                    let checking = matches!(
                        self.state.audio_permission_state,
                        AudioInputPermissionState::Checking
                    );

                    if ui
                        .add_enabled(
                            !checking,
                            egui::Button::new("🎤 Request Microphone Permission"),
                        )
                        .clicked()
                    {
                        self.state.request_audio_permission_recheck();
                        ui.close_menu();
                    }

                    #[cfg(target_os = "windows")]
                    if ui.button("⚙ Open Windows Microphone Settings").clicked() {
                        let _ = std::process::Command::new("cmd")
                            .args(["/c", "start", "ms-settings:privacy-microphone"])
                            .spawn();
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