//! The engine: owns the monitor controller and applies everything that
//! happens without the window: presets, night light, the schedule, per-app
//! profiles, idle dimming, hotkeys and the tray icon.
//!
//! The user interface reads [`Shared`] (settings and a [`LiveState`]
//! snapshot) and sends [`Cmd`]s through [`Control`].

use crate::model::{DisplayChange, DisplayState, Feature, Level, Monitor, MonitorKey};
use crate::settings::Settings;
use crate::timeutil::Tz;
use std::collections::BTreeMap;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};

/// One monitor as the UI sees it.
#[derive(Clone, Debug, PartialEq)]
pub struct MonitorView {
    pub monitor: Monitor,
    /// The name to show: the user's rename, else the monitor's own.
    pub display_name: String,
    /// Last known hardware levels, by feature.
    pub levels: BTreeMap<Feature, Level>,
    /// Brightness on the user scale: 0..=100, or below zero (down to
    /// `settings::MIN_BELOW_ZERO`) when software dimming is added.
    pub brightness: i32,
    /// A fade is in progress.
    pub fading: bool,
    /// The last error talking to this monitor, in plain words.
    pub error: Option<String>,
}

impl MonitorView {
    pub fn level(&self, f: Feature) -> Option<Level> {
        self.levels.get(&f).copied()
    }
}

/// Where the schedule stands.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct ScheduleStatus {
    pub enabled: bool,
    /// False after a manual change, until the next schedule point.
    pub following: bool,
    /// "Since 17:12 (sunset - 30 min)": the point now in effect.
    pub current: Option<String>,
    /// The next point: UTC time and a label.
    pub next: Option<(i64, String)>,
    /// Today's sunrise and sunset (UTC), when a location is set and the sun rises and sets.
    pub sunrise: Option<i64>,
    pub sunset: Option<i64>,
}

/// A snapshot of what the engine is doing, for the UI.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct LiveState {
    /// False until the first scan of the monitors is complete.
    pub ready: bool,
    pub monitors: Vec<MonitorView>,
    pub displays: Vec<DisplayState>,
    /// Night light as currently applied (`kelvin` is 6500 when off).
    pub night_on: bool,
    pub kelvin: u32,
    /// The executable whose profile is active, if any.
    pub active_profile: Option<String>,
    pub schedule: ScheduleStatus,
    pub idle_dimmed: bool,
    /// Executable names of apps with visible windows (for "add a profile").
    pub running_apps: Vec<String>,
    /// The last preset applied by name.
    pub last_preset: Option<String>,
    /// The answer to the last `ApplyDisplay`: what to revert to, or an error.
    pub display_result: Option<(String, Result<DisplayChange, String>)>,
    /// The answer to `ReadAllVcp`: monitor key and (code, value or error) pairs.
    pub vcp_dump: Option<(MonitorKey, crate::control::VcpDump)>,
    /// A short message for the UI to show once (and clear).
    pub notice: Option<String>,
}

/// What the UI (or the command line) asks the engine to do.
#[derive(Clone, Debug, PartialEq)]
pub enum Cmd {
    /// Set brightness on the user scale; `None` = every linked monitor.
    Brightness {
        key: Option<MonitorKey>,
        value: i32,
    },
    /// Change brightness by a step on every linked monitor (hotkeys, tray wheel).
    Step(i32),
    /// Set a hardware feature: a percentage for continuous features, the
    /// raw code for input source, power mode and colour preset.
    Feature {
        key: MonitorKey,
        feature: Feature,
        value: u32,
    },
    Preset(String),
    NightLight(bool),
    Kelvin(u32),
    /// The UI changed `Shared::settings`; re-read and save them.
    SettingsChanged,
    /// Look for monitors again.
    Rescan,
    /// Change resolution, refresh rate, orientation or primary display. The
    /// result (what to revert to) appears in `LiveState::display_result`.
    ApplyDisplay {
        device: String,
        change: DisplayChange,
    },
    /// Read every VCP code the monitor lists (read-only); see `LiveState::vcp_dump`.
    ReadAllVcp(MonitorKey),
    /// Follow the schedule again after a manual change.
    ResumeSchedule,
    ScreensOff,
    Quit,
}

/// Requests for the window: open a page, show the flyout, or quit.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum UiRequest {
    Open(Option<String>),
    Flyout,
    Quit,
}

/// Sends commands to the engine.
pub trait Control: Send + Sync {
    fn send(&self, cmd: Cmd);
}

/// State shared between the engine and the window.
pub struct Shared {
    pub dir: PathBuf,
    pub demo: bool,
    pub settings: Mutex<Settings>,
    pub live: Mutex<LiveState>,
    pub tz: Arc<dyn Tz>,
    /// A fixed "now" (demo and screenshots), else the clock.
    pub fixed_now: Option<i64>,
    /// Called when `live` changed, to repaint the window.
    pub waker: Mutex<Option<Box<dyn Fn() + Send>>>,
    /// Requests for the window while it is open.
    pub requests: Mutex<Vec<UiRequest>>,
    /// Set while a window (main or flyout) is open.
    pub window_open: AtomicBool,
    /// When the last window closed (milliseconds since the engine started);
    /// a tray click right after the flyout closed must not reopen it.
    pub closed_at_ms: std::sync::atomic::AtomicU64,
    pub quit: AtomicBool,
}

impl Shared {
    pub fn new(dir: PathBuf, settings: Settings, tz: Arc<dyn Tz>, demo: bool) -> Self {
        Shared {
            dir,
            demo,
            settings: Mutex::new(settings),
            live: Mutex::new(LiveState::default()),
            tz,
            fixed_now: None,
            waker: Mutex::new(None),
            requests: Mutex::new(Vec::new()),
            window_open: AtomicBool::new(false),
            closed_at_ms: std::sync::atomic::AtomicU64::new(0),
            quit: AtomicBool::new(false),
        }
    }

    pub fn now(&self) -> i64 {
        self.fixed_now.unwrap_or_else(crate::timeutil::now)
    }

    pub fn settings(&self) -> Settings {
        self.settings.lock().map(|s| s.clone()).unwrap_or_default()
    }

    pub fn live(&self) -> LiveState {
        self.live.lock().map(|l| l.clone()).unwrap_or_default()
    }

    pub fn wake(&self) {
        if let Ok(w) = self.waker.lock() {
            if let Some(w) = w.as_ref() {
                w();
            }
        }
    }

    /// Takes the pending window requests.
    pub fn take_requests(&self) -> Vec<UiRequest> {
        self.requests
            .lock()
            .map(|mut r| std::mem::take(&mut *r))
            .unwrap_or_default()
    }

    pub fn is_window_open(&self) -> bool {
        self.window_open.load(Ordering::SeqCst)
    }
}

/// How much of the system the engine may touch.
#[derive(Clone, Copy, Debug)]
pub struct EngineOptions {
    /// Tray icon, hotkeys, foreground tracking, idle detection and overlays.
    /// Off for the demo, screenshots, the self-test and command-line use.
    pub system: bool,
}

/// The running engine: a thread with the [`crate::policy::Core`].
pub struct Engine {
    shared: Arc<Shared>,
    tx: std::sync::mpsc::Sender<Cmd>,
    window: Arc<std::sync::atomic::AtomicIsize>,
    requests: Mutex<std::sync::mpsc::Receiver<UiRequest>>,
    thread: Option<std::thread::JoinHandle<()>>,
}

struct Sender {
    tx: Mutex<std::sync::mpsc::Sender<Cmd>>,
    window: Arc<std::sync::atomic::AtomicIsize>,
}

impl Control for Sender {
    fn send(&self, cmd: Cmd) {
        if let Ok(tx) = self.tx.lock() {
            let _ = tx.send(cmd);
        }
        wake_window(&self.window);
    }
}

fn wake_window(window: &std::sync::atomic::AtomicIsize) {
    let w = window.load(Ordering::SeqCst);
    if w != 0 {
        // SAFETY: posting a message to our own engine window.
        unsafe {
            let _ = windows::Win32::UI::WindowsAndMessaging::PostMessageW(
                windows::Win32::Foundation::HWND(w as *mut core::ffi::c_void),
                crate::service::WM_WAKE,
                windows::Win32::Foundation::WPARAM(0),
                windows::Win32::Foundation::LPARAM(0),
            );
        }
    }
}

impl Engine {
    /// Starts the engine thread on these backends.
    pub fn start(
        shared: Arc<Shared>,
        backend: Box<dyn crate::model::Backend>,
        displays: Box<dyn crate::model::DisplayConfig>,
        options: EngineOptions,
    ) -> Engine {
        let (tx, rx) = std::sync::mpsc::channel();
        let (req_tx, req_rx) = std::sync::mpsc::channel();
        let window = Arc::new(std::sync::atomic::AtomicIsize::new(0));
        let thread = {
            let shared = shared.clone();
            let window = window.clone();
            std::thread::Builder::new()
                .name("displayforge-engine".into())
                .spawn(move || {
                    if options.system {
                        crate::service::run(shared, backend, displays, rx, req_tx, window);
                    } else {
                        crate::service::run_headless(shared, backend, displays, rx, req_tx);
                    }
                })
                .ok()
        };
        Engine {
            shared,
            tx,
            window,
            requests: Mutex::new(req_rx),
            thread,
        }
    }

    /// A handle for sending commands.
    pub fn control(&self) -> Arc<dyn Control> {
        Arc::new(Sender {
            tx: Mutex::new(self.tx.clone()),
            window: self.window.clone(),
        })
    }

    pub fn shared(&self) -> &Arc<Shared> {
        &self.shared
    }

    /// Blocks until the tray (or a second start) asks for the window.
    pub fn wait_request(&self) -> UiRequest {
        match self.requests.lock() {
            Ok(rx) => rx.recv().unwrap_or(UiRequest::Quit),
            Err(_) => UiRequest::Quit,
        }
    }

    /// Stops the engine; it removes the tray icon, hotkeys and overlays and
    /// restores every gamma ramp it changed.
    pub fn stop(mut self) {
        let _ = self.tx.send(Cmd::Quit);
        wake_window(&self.window);
        if let Some(t) = self.thread.take() {
            let _ = t.join();
        }
    }
}

impl Drop for Engine {
    fn drop(&mut self) {
        if let Some(t) = self.thread.take() {
            let _ = self.tx.send(Cmd::Quit);
            wake_window(&self.window);
            let _ = t.join();
        }
    }
}

/// Hands a request to an already running DisplayForge (second start, `--quit`).
/// False when none is running.
pub fn signal_running(request: &UiRequest) -> bool {
    let text = match request {
        UiRequest::Open(Some(view)) => format!("open {view}"),
        UiRequest::Open(None) => "open".to_string(),
        UiRequest::Flyout => "flyout".to_string(),
        UiRequest::Quit => "quit".to_string(),
    };
    crate::service::send_text(&text)
}
