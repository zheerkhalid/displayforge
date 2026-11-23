//! The engine thread. On the real PC (`run`) it owns a hidden window with
//! the tray icon, global hotkeys, the mouse-wheel-over-the-tray hook, the
//! dimming overlays and the messages from a second start; it wakes on
//! commands, timers and system notifications. `run_headless` drives the
//! same [`Core`] without touching the system (demo, screenshots).

use crate::engine::{Cmd, Shared, UiRequest};
use crate::hw::overlay::Overlay;
use crate::model::{Backend, DisplayConfig, MonitorKey};
use crate::policy::{Core, Inputs};
use crate::settings::HotAction;
use crate::sys::wide;
use std::cell::RefCell;
use std::collections::BTreeMap;
use std::sync::atomic::{AtomicIsize, Ordering};
use std::sync::mpsc::{Receiver, RecvTimeoutError, Sender};
use std::sync::Arc;
use std::time::{Duration, Instant};
use windows::core::{w, PCWSTR};
use windows::Win32::Foundation::{HINSTANCE, HWND, LPARAM, LRESULT, POINT, WPARAM};
use windows::Win32::System::DataExchange::COPYDATASTRUCT;
use windows::Win32::System::LibraryLoader::GetModuleHandleW;
use windows::Win32::UI::Input::KeyboardAndMouse::{
    RegisterHotKey, UnregisterHotKey, HOT_KEY_MODIFIERS, MOD_NOREPEAT,
};
use windows::Win32::UI::Shell::{
    Shell_NotifyIconGetRect, Shell_NotifyIconW, NIF_ICON, NIF_MESSAGE, NIF_TIP, NIM_ADD,
    NIM_DELETE, NIM_MODIFY, NOTIFYICONDATAW, NOTIFYICONIDENTIFIER,
};
use windows::Win32::UI::WindowsAndMessaging::{
    AppendMenuW, CallNextHookEx, CreatePopupMenu, CreateWindowExW, DefWindowProcW, DestroyMenu,
    DestroyWindow, DispatchMessageW, FindWindowW, GetCursorPos, GetMessageW, KillTimer, LoadIconW,
    PostMessageW, RegisterClassW, RegisterWindowMessageW, SendMessageTimeoutW, SetForegroundWindow,
    SetTimer, SetWindowsHookExW, TrackPopupMenu, TranslateMessage, UnhookWindowsHookEx, HHOOK,
    HMENU, IDI_APPLICATION, MF_CHECKED, MF_POPUP, MF_SEPARATOR, MF_STRING, MSG, MSLLHOOKSTRUCT,
    SMTO_ABORTIFHUNG, TPM_NONOTIFY, TPM_RETURNCMD, TPM_RIGHTBUTTON, WH_MOUSE_LL, WM_APP,
    WM_COPYDATA, WM_DISPLAYCHANGE, WM_HOTKEY, WM_LBUTTONDBLCLK, WM_LBUTTONUP, WM_MOUSEWHEEL,
    WM_POWERBROADCAST, WM_RBUTTONUP, WM_TIMER, WNDCLASSW, WS_EX_TOOLWINDOW, WS_POPUP,
};

pub const WINDOW_CLASS: PCWSTR = w!("DisplayForge.Engine");
const WM_TRAY: u32 = WM_APP + 1;
/// Posted by `Control::send` and the controller to wake the loop.
pub const WM_WAKE: u32 = WM_APP + 2;
const TIMER_TICK: usize = 1;
const TIMER_RESCAN: usize = 2;
const TICK_MS: u32 = 500;
const PBT_APMRESUMEAUTOMATIC: usize = 0x12;
const COPYDATA_MAGIC: usize = 0x4446_4f52; // "DFOR"

const MENU_OPEN: usize = 1;
const MENU_NIGHT: usize = 2;
const MENU_RESUME: usize = 3;
const MENU_OFF: usize = 4;
const MENU_QUIT: usize = 5;
const MENU_PRESET: usize = 100;

/// Something the window procedure saw; handled by the loop.
#[derive(Debug)]
enum Event {
    TrayLeft,
    TrayDouble,
    TrayMenu,
    Wheel(i32),
    Hotkey(i32),
    Text(String),
    DisplayChanged,
    Resumed,
    TaskbarCreated,
    Tick,
    Rescan,
}

thread_local! {
    static EVENTS: RefCell<Vec<Event>> = const { RefCell::new(Vec::new()) };
    static TASKBAR_MSG: RefCell<u32> = const { RefCell::new(0) };
    static TRAY_WINDOW: RefCell<isize> = const { RefCell::new(0) };
}

fn push(e: Event) {
    EVENTS.with(|q| q.borrow_mut().push(e));
}

fn hwnd(v: isize) -> HWND {
    HWND(v as *mut core::ffi::c_void)
}

/// Milliseconds since 1970, for the flyout close / reopen guard.
pub fn now_ms() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0)
}

/// Sends a request to the window: straight to an open window, else to the
/// main thread (which opens one).
pub fn route(shared: &Shared, requests: &Sender<UiRequest>, r: UiRequest) {
    if r == UiRequest::Flyout
        && now_ms().saturating_sub(shared.closed_at_ms.load(Ordering::SeqCst)) < 400
    {
        // The click that closed the flyout (by taking its focus) must not reopen it.
        return;
    }
    if r == UiRequest::Quit {
        shared.quit.store(true, Ordering::SeqCst);
    }
    if shared.is_window_open() {
        if let Ok(mut q) = shared.requests.lock() {
            q.push(r.clone());
        }
        shared.wake();
        if r == UiRequest::Quit {
            let _ = requests.send(r);
        }
    } else {
        let _ = requests.send(r);
    }
}

/// A command line handed over by a second start (see `engine::signal_running`).
pub enum Handover {
    Request(UiRequest),
    Command(Cmd),
    /// `brightness <value> <monitor number>`
    BrightnessOf(i32, usize),
}

/// Parses the text a second start sends.
pub fn parse_handover(text: &str) -> Option<Handover> {
    let mut parts = text.splitn(3, ' ');
    let verb = parts.next()?;
    let a = parts.next();
    let b = parts.next();
    Some(match verb {
        "open" => Handover::Request(UiRequest::Open(a.map(|v| v.to_string()))),
        "flyout" => Handover::Request(UiRequest::Flyout),
        "quit" => Handover::Request(UiRequest::Quit),
        "preset" => Handover::Command(Cmd::Preset(text.get(7..)?.trim().to_string())),
        "brightness" => {
            let v: i32 = a?.parse().ok()?;
            match b {
                Some(n) => Handover::BrightnessOf(v, n.parse().ok()?),
                None => Handover::Command(Cmd::Brightness {
                    key: None,
                    value: v,
                }),
            }
        }
        _ => return None,
    })
}

/// Hands text to the running DisplayForge's engine window. False when none runs.
pub fn send_text(text: &str) -> bool {
    let data = wide(text);
    // SAFETY: the window is looked up by class; the data outlives the
    // synchronous, time-limited send.
    unsafe {
        let Ok(target) = FindWindowW(WINDOW_CLASS, PCWSTR::null()) else {
            return false;
        };
        if target.0.is_null() {
            return false;
        }
        let cds = COPYDATASTRUCT {
            dwData: COPYDATA_MAGIC,
            cbData: (data.len() * 2) as u32,
            lpData: data.as_ptr() as *mut core::ffi::c_void,
        };
        let mut result = 0usize;
        let sent = SendMessageTimeoutW(
            target,
            WM_COPYDATA,
            WPARAM(0),
            LPARAM(&cds as *const COPYDATASTRUCT as isize),
            SMTO_ABORTIFHUNG,
            3000,
            Some(&mut result as *mut usize),
        );
        sent.0 != 0
    }
}

unsafe extern "system" fn wnd_proc(
    window: HWND,
    msg: u32,
    wparam: WPARAM,
    lparam: LPARAM,
) -> LRESULT {
    match msg {
        WM_TRAY => {
            match (lparam.0 as u32) & 0xffff {
                WM_LBUTTONUP => push(Event::TrayLeft),
                WM_LBUTTONDBLCLK => push(Event::TrayDouble),
                WM_RBUTTONUP => push(Event::TrayMenu),
                _ => {}
            }
            LRESULT(0)
        }
        WM_HOTKEY => {
            push(Event::Hotkey(wparam.0 as i32));
            LRESULT(0)
        }
        WM_TIMER => {
            push(if wparam.0 == TIMER_RESCAN {
                let _ = KillTimer(window, TIMER_RESCAN);
                Event::Rescan
            } else {
                Event::Tick
            });
            LRESULT(0)
        }
        WM_DISPLAYCHANGE => {
            push(Event::DisplayChanged);
            LRESULT(0)
        }
        WM_POWERBROADCAST => {
            if wparam.0 == PBT_APMRESUMEAUTOMATIC {
                push(Event::Resumed);
            }
            LRESULT(1)
        }
        WM_COPYDATA => {
            // SAFETY: lparam points to a COPYDATASTRUCT for the duration of the message.
            let cds = &*(lparam.0 as *const COPYDATASTRUCT);
            if cds.dwData == COPYDATA_MAGIC && !cds.lpData.is_null() && cds.cbData < 4096 {
                let units =
                    std::slice::from_raw_parts(cds.lpData as *const u16, cds.cbData as usize / 2);
                push(Event::Text(crate::sys::from_wide(units)));
            }
            LRESULT(1)
        }
        WM_WAKE => LRESULT(0),
        _ => {
            let taskbar = TASKBAR_MSG.with(|m| *m.borrow());
            if taskbar != 0 && msg == taskbar {
                push(Event::TaskbarCreated);
                return LRESULT(0);
            }
            DefWindowProcW(window, msg, wparam, lparam)
        }
    }
}

/// Low-level mouse hook: the wheel over the tray icon changes brightness.
unsafe extern "system" fn mouse_hook(code: i32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
    if code >= 0 && wparam.0 as u32 == WM_MOUSEWHEEL {
        // SAFETY: for WH_MOUSE_LL, lparam points to an MSLLHOOKSTRUCT.
        let info = &*(lparam.0 as *const MSLLHOOKSTRUCT);
        let window = TRAY_WINDOW.with(|w| *w.borrow());
        if window != 0 {
            let id = NOTIFYICONIDENTIFIER {
                cbSize: std::mem::size_of::<NOTIFYICONIDENTIFIER>() as u32,
                hWnd: hwnd(window),
                uID: 1,
                ..Default::default()
            };
            if let Ok(r) = Shell_NotifyIconGetRect(&id) {
                let p = info.pt;
                if p.x >= r.left && p.x < r.right && p.y >= r.top && p.y < r.bottom {
                    let delta = ((info.mouseData >> 16) & 0xffff) as u16 as i16;
                    push(Event::Wheel(if delta > 0 { 1 } else { -1 }));
                    let _ = PostMessageW(hwnd(window), WM_WAKE, WPARAM(0), LPARAM(0));
                    return LRESULT(1);
                }
            }
        }
    }
    CallNextHookEx(HHOOK::default(), code, wparam, lparam)
}

struct Tray {
    window: HWND,
    shown: bool,
    tip: String,
}

impl Tray {
    fn data(&self) -> NOTIFYICONDATAW {
        let mut data = NOTIFYICONDATAW {
            cbSize: std::mem::size_of::<NOTIFYICONDATAW>() as u32,
            hWnd: self.window,
            uID: 1,
            uFlags: NIF_ICON | NIF_MESSAGE | NIF_TIP,
            uCallbackMessage: WM_TRAY,
            ..Default::default()
        };
        // SAFETY: loading the icon embedded by the build script (resource 1).
        unsafe {
            let module = GetModuleHandleW(PCWSTR::null()).unwrap_or_default();
            data.hIcon = LoadIconW(HINSTANCE(module.0), PCWSTR(std::ptr::without_provenance(1)))
                .or_else(|_| LoadIconW(HINSTANCE::default(), IDI_APPLICATION))
                .unwrap_or_default();
        }
        let max = data.szTip.len() - 1;
        for (i, unit) in self.tip.encode_utf16().take(max).enumerate() {
            data.szTip[i] = unit;
        }
        data
    }

    fn show(&mut self) {
        let data = self.data();
        // SAFETY: the structure is fully initialised and outlives the call.
        unsafe {
            let _ = Shell_NotifyIconW(NIM_DELETE, &data);
            self.shown = Shell_NotifyIconW(NIM_ADD, &data).as_bool();
        }
    }

    fn set_tip(&mut self, tip: String) {
        if tip == self.tip {
            return;
        }
        self.tip = tip;
        if self.shown {
            let data = self.data();
            // SAFETY: as above.
            unsafe {
                let _ = Shell_NotifyIconW(NIM_MODIFY, &data);
            }
        }
    }

    fn remove(&mut self) {
        if self.shown {
            let data = self.data();
            // SAFETY: as above.
            unsafe {
                let _ = Shell_NotifyIconW(NIM_DELETE, &data);
            }
            self.shown = false;
        }
    }
}

/// The real-PC engine loop. Returns when `Cmd::Quit` arrives.
pub fn run(
    shared: Arc<Shared>,
    backend: Box<dyn Backend>,
    displays: Box<dyn DisplayConfig>,
    commands: Receiver<Cmd>,
    requests: Sender<UiRequest>,
    window_slot: Arc<AtomicIsize>,
) {
    // SAFETY: a standard hidden window, timers, hotkeys, hook and message
    // loop, all owned by this thread and torn down before it returns.
    unsafe {
        let module = GetModuleHandleW(PCWSTR::null()).unwrap_or_default();
        let class = WNDCLASSW {
            lpfnWndProc: Some(wnd_proc),
            hInstance: HINSTANCE(module.0),
            lpszClassName: WINDOW_CLASS,
            ..Default::default()
        };
        RegisterClassW(&class);
        // Top-level (not message-only): display and power broadcasts only reach those.
        let Ok(window) = CreateWindowExW(
            WS_EX_TOOLWINDOW,
            WINDOW_CLASS,
            w!("DisplayForge engine"),
            WS_POPUP,
            0,
            0,
            0,
            0,
            HWND::default(),
            HMENU::default(),
            HINSTANCE(module.0),
            None,
        ) else {
            // No window: still serve commands without tray and hotkeys.
            return run_headless(shared, backend, displays, commands, requests);
        };
        let raw = window.0 as isize;
        window_slot.store(raw, Ordering::SeqCst);
        TRAY_WINDOW.with(|w| *w.borrow_mut() = raw);
        TASKBAR_MSG.with(|m| *m.borrow_mut() = RegisterWindowMessageW(w!("TaskbarCreated")));

        let wake_slot = window_slot.clone();
        let mut core = Core::new(
            shared.clone(),
            backend,
            displays,
            Box::new(move || {
                let w = wake_slot.load(Ordering::SeqCst);
                if w != 0 {
                    let _ = PostMessageW(hwnd(w), WM_WAKE, WPARAM(0), LPARAM(0));
                }
            }),
        );
        let mut tray = Tray {
            window,
            shown: false,
            tip: "DisplayForge".into(),
        };
        tray.show();
        let mut hotkeys = register_hotkeys(window, &core, &mut Vec::new());
        let mut hook = set_hook(core.settings.scroll_on_tray, None);
        let mut overlays: BTreeMap<MonitorKey, Dim> = BTreeMap::new();
        SetTimer(window, TIMER_TICK, TICK_MS, None);
        let mut last_apps = Instant::now() - Duration::from_secs(60);
        let mut running = true;

        let mut msg = MSG::default();
        while running && GetMessageW(&mut msg, HWND::default(), 0, 0).as_bool() {
            let _ = TranslateMessage(&msg);
            DispatchMessageW(&msg);

            // Commands from the window and the command line.
            while let Ok(cmd) = commands.try_recv() {
                let before = core.settings.clone();
                match cmd {
                    Cmd::ScreensOff => crate::hw::screens_off(),
                    other => running &= core.handle(other),
                }
                if core.settings.hotkeys != before.hotkeys {
                    hotkeys = register_hotkeys(window, &core, &mut hotkeys);
                }
                if core.settings.scroll_on_tray != before.scroll_on_tray {
                    hook = set_hook(core.settings.scroll_on_tray, hook);
                }
                if !running {
                    break;
                }
            }
            let events: Vec<Event> = EVENTS.with(|q| std::mem::take(&mut *q.borrow_mut()));
            for event in events {
                match event {
                    Event::TrayLeft => route(&shared, &requests, UiRequest::Flyout),
                    Event::TrayDouble => route(&shared, &requests, UiRequest::Open(None)),
                    Event::TrayMenu => {
                        if let Some(choice) = tray_menu(window, &core) {
                            running &= menu_choice(choice, &mut core, &shared, &requests);
                        }
                    }
                    Event::Wheel(dir) => {
                        let step = core.settings.step as i32;
                        core.handle(Cmd::Step(dir * step));
                    }
                    Event::Hotkey(id) => {
                        let action = core
                            .settings
                            .hotkeys
                            .get((id - 1).max(0) as usize)
                            .map(|h| h.action.clone());
                        if let Some(action) = action {
                            hotkey_action(action, &mut core, &shared, &requests);
                        }
                    }
                    Event::Text(text) => match parse_handover(&text) {
                        Some(Handover::Request(r)) => route(&shared, &requests, r),
                        Some(Handover::Command(c)) => {
                            core.handle(c);
                        }
                        Some(Handover::BrightnessOf(v, n)) => {
                            let key = core
                                .controller
                                .snapshot()
                                .into_iter()
                                .find(|s| s.monitor.number == n)
                                .map(|s| s.monitor.key);
                            if let Some(key) = key {
                                core.handle(Cmd::Brightness {
                                    key: Some(key),
                                    value: v,
                                });
                            }
                        }
                        None => {}
                    },
                    Event::DisplayChanged | Event::Resumed => {
                        // Wait for things to settle, then look again and re-apply.
                        SetTimer(window, TIMER_RESCAN, 2000, None);
                    }
                    Event::Rescan => {
                        core.handle(Cmd::Rescan);
                        core.invalidate();
                        overlays.clear();
                    }
                    Event::TaskbarCreated => tray.show(),
                    Event::Tick => {
                        let front = core.front_from(crate::foreground::current());
                        if shared.is_window_open() && last_apps.elapsed() > Duration::from_secs(5) {
                            core.running_apps = Some(crate::foreground::running_apps());
                            last_apps = Instant::now();
                        }
                        let inputs = Inputs {
                            now: shared.now(),
                            front,
                            idle_secs: crate::sys::idle_seconds(),
                        };
                        core.tick(&inputs);
                    }
                }
            }
            if core.changed.load(Ordering::SeqCst) {
                core.publish();
            }
            sync_overlays(&core, &mut overlays);
            let tip = match core.headline_brightness() {
                Some(b) => format!("DisplayForge: brightness {b}%"),
                None => "DisplayForge".to_string(),
            };
            tray.set_tip(tip);
        }

        // Leave nothing behind: overlays, hotkeys, hook, tray icon, gamma ramps.
        overlays.clear();
        for id in hotkeys {
            let _ = UnregisterHotKey(window, id);
        }
        set_hook(false, hook);
        let _ = KillTimer(window, TIMER_TICK);
        let _ = KillTimer(window, TIMER_RESCAN);
        tray.remove();
        window_slot.store(0, Ordering::SeqCst);
        TRAY_WINDOW.with(|w| *w.borrow_mut() = 0);
        let _ = DestroyWindow(window);
        core.shutdown();
    }
}

/// An overlay window with what it was last set to.
struct Dim {
    overlay: Overlay,
    rect: crate::model::PxRect,
    alpha: u8,
}

fn sync_overlays(core: &Core, overlays: &mut BTreeMap<MonitorKey, Dim>) {
    overlays.retain(|k, _| core.overlays.contains_key(k));
    if core.overlays.is_empty() {
        return;
    }
    let monitors = core.controller.snapshot();
    for (key, alpha) in &core.overlays {
        let Some(m) = monitors.iter().find(|s| &s.monitor.key == key) else {
            continue;
        };
        let rect = m.monitor.rect;
        if !overlays.contains_key(key) {
            if let Some(overlay) = Overlay::new(rect) {
                overlays.insert(
                    key.clone(),
                    Dim {
                        overlay,
                        rect,
                        alpha: 0,
                    },
                );
            }
        }
        if let Some(d) = overlays.get_mut(key) {
            if d.rect != rect {
                d.rect = rect;
                d.overlay.set_rect(rect);
            }
            if d.alpha != *alpha {
                d.alpha = *alpha;
                d.overlay.set_alpha(*alpha);
            }
        }
    }
}

/// (Re)registers the enabled hotkeys; ids are 1-based indexes into the list.
fn register_hotkeys(window: HWND, core: &Core, old: &mut Vec<i32>) -> Vec<i32> {
    let mut ids = Vec::new();
    // SAFETY: registering hotkeys on our own window.
    unsafe {
        for id in old.drain(..) {
            let _ = UnregisterHotKey(window, id);
        }
        let mut failed = Vec::new();
        for (i, h) in core.settings.hotkeys.iter().enumerate() {
            if !h.enabled || h.keys.trim().is_empty() {
                continue;
            }
            let Ok(keys) = crate::hotkey::parse(&h.keys) else {
                failed.push(h.keys.clone());
                continue;
            };
            let repeat = matches!(
                h.action,
                HotAction::BrightnessUp | HotAction::BrightnessDown
            );
            let mut mods = HOT_KEY_MODIFIERS(keys.mods);
            if !repeat {
                mods |= MOD_NOREPEAT;
            }
            let id = i as i32 + 1;
            if RegisterHotKey(window, id, mods, keys.vk).is_ok() {
                ids.push(id);
            } else {
                failed.push(h.keys.clone());
            }
        }
        if !failed.is_empty() {
            if let Ok(mut live) = core.shared.live.lock() {
                live.notice = Some(format!(
                    "These shortcuts could not be registered (another program may use them): {}",
                    failed.join(", ")
                ));
            }
            core.shared.wake();
        }
    }
    ids
}

fn set_hook(on: bool, current: Option<HHOOK>) -> Option<HHOOK> {
    // SAFETY: installing / removing our own low-level hook on this thread,
    // which runs a message loop.
    unsafe {
        match (on, current) {
            (true, Some(h)) => Some(h),
            (true, None) => {
                let module = GetModuleHandleW(PCWSTR::null()).unwrap_or_default();
                SetWindowsHookExW(WH_MOUSE_LL, Some(mouse_hook), HINSTANCE(module.0), 0).ok()
            }
            (false, Some(h)) => {
                let _ = UnhookWindowsHookEx(h);
                None
            }
            (false, None) => None,
        }
    }
}

fn hotkey_action(
    action: HotAction,
    core: &mut Core,
    shared: &Shared,
    requests: &Sender<UiRequest>,
) {
    let step = core.settings.step as i32;
    match action {
        HotAction::BrightnessUp => {
            core.handle(Cmd::Step(step));
        }
        HotAction::BrightnessDown => {
            core.handle(Cmd::Step(-step));
        }
        HotAction::Preset(name) => {
            core.handle(Cmd::Preset(name));
        }
        HotAction::ToggleNightLight => {
            let on = shared.live().night_on;
            core.handle(Cmd::NightLight(!on));
        }
        HotAction::ScreensOff => crate::hw::screens_off(),
        HotAction::OpenFlyout => route(shared, requests, UiRequest::Flyout),
    }
}

fn menu_choice(
    choice: usize,
    core: &mut Core,
    shared: &Shared,
    requests: &Sender<UiRequest>,
) -> bool {
    match choice {
        MENU_OPEN => route(shared, requests, UiRequest::Open(None)),
        MENU_NIGHT => {
            let on = shared.live().night_on;
            core.handle(Cmd::NightLight(!on));
        }
        MENU_RESUME => {
            core.handle(Cmd::ResumeSchedule);
        }
        MENU_OFF => crate::hw::screens_off(),
        MENU_QUIT => route(shared, requests, UiRequest::Quit),
        n if n >= MENU_PRESET => {
            let name = core
                .settings
                .presets
                .get(n - MENU_PRESET)
                .map(|p| p.name.clone());
            if let Some(name) = name {
                core.handle(Cmd::Preset(name));
            }
        }
        _ => {}
    }
    true
}

unsafe fn tray_menu(window: HWND, core: &Core) -> Option<usize> {
    let menu = CreatePopupMenu().ok()?;
    let _ = AppendMenuW(menu, MF_STRING, MENU_OPEN, w!("Open DisplayForge"));
    let _ = AppendMenuW(menu, MF_SEPARATOR, 0, PCWSTR::null());
    if let Ok(sub) = CreatePopupMenu() {
        for (i, p) in core.settings.presets.iter().enumerate().take(20) {
            let name = wide(&p.name);
            let _ = AppendMenuW(sub, MF_STRING, MENU_PRESET + i, PCWSTR(name.as_ptr()));
        }
        let _ = AppendMenuW(menu, MF_POPUP, sub.0 as usize, w!("Presets"));
    }
    let night = core.shared.live().night_on;
    let flags = if night {
        MF_STRING | MF_CHECKED
    } else {
        MF_STRING
    };
    let _ = AppendMenuW(menu, flags, MENU_NIGHT, w!("Night light"));
    if core.settings.schedule.enabled && !core.following {
        let _ = AppendMenuW(
            menu,
            MF_STRING,
            MENU_RESUME,
            w!("Follow the schedule again"),
        );
    }
    let _ = AppendMenuW(menu, MF_STRING, MENU_OFF, w!("Turn screens off"));
    let _ = AppendMenuW(menu, MF_SEPARATOR, 0, PCWSTR::null());
    let _ = AppendMenuW(menu, MF_STRING, MENU_QUIT, w!("Quit DisplayForge"));
    let mut pt = POINT::default();
    let _ = GetCursorPos(&mut pt);
    // Needed so the menu closes when the user clicks elsewhere.
    let _ = SetForegroundWindow(window);
    let choice = TrackPopupMenu(
        menu,
        TPM_RETURNCMD | TPM_NONOTIFY | TPM_RIGHTBUTTON,
        pt.x,
        pt.y,
        0,
        window,
        None,
    );
    let _ = DestroyMenu(menu);
    (choice.0 > 0).then_some(choice.0 as usize)
}

/// The engine loop without any system integration: commands, a tick every
/// half second with no foreground app and no idle time.
pub fn run_headless(
    shared: Arc<Shared>,
    backend: Box<dyn Backend>,
    displays: Box<dyn DisplayConfig>,
    commands: Receiver<Cmd>,
    _requests: Sender<UiRequest>,
) {
    let mut core = Core::new(shared.clone(), backend, displays, Box::new(|| {}));
    let mut last_tick = Instant::now() - Duration::from_secs(1);
    loop {
        match commands.recv_timeout(Duration::from_millis(100)) {
            Ok(Cmd::Quit) | Err(RecvTimeoutError::Disconnected) => break,
            Ok(cmd) => {
                core.handle(cmd);
            }
            Err(RecvTimeoutError::Timeout) => {}
        }
        if last_tick.elapsed() >= Duration::from_millis(TICK_MS as u64) {
            last_tick = Instant::now();
            core.tick(&Inputs {
                now: shared.now(),
                front: None,
                idle_secs: 0,
            });
        } else if core.changed.load(Ordering::SeqCst) {
            core.publish();
        }
    }
    core.shutdown();
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn handover_parsing() {
        assert!(matches!(
            parse_handover("open"),
            Some(Handover::Request(UiRequest::Open(None)))
        ));
        assert!(matches!(
            parse_handover("open schedule"),
            Some(Handover::Request(UiRequest::Open(Some(v)))) if v == "schedule"
        ));
        assert!(matches!(
            parse_handover("brightness 40"),
            Some(Handover::Command(Cmd::Brightness {
                key: None,
                value: 40
            }))
        ));
        assert!(matches!(
            parse_handover("brightness 40 2"),
            Some(Handover::BrightnessOf(40, 2))
        ));
        assert!(matches!(
            parse_handover("preset Late night"),
            Some(Handover::Command(Cmd::Preset(n))) if n == "Late night"
        ));
        assert!(parse_handover("brightness x").is_none());
        assert!(parse_handover("format c:").is_none());
    }
}
