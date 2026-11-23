#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use displayforge::engine::{Engine, EngineOptions, Shared, UiRequest};
use displayforge::settings::{self, Settings};
use displayforge::ui::{self, Mode, Options};
use eframe::egui;
use std::path::PathBuf;
use std::sync::atomic::Ordering;
use std::sync::Arc;

const USAGE: &str = "DisplayForge: monitor control for Windows

  DisplayForge.exe                       open the window (and the tray icon)
  DisplayForge.exe --background          start in the notification area only
                                         (what the start-with-Windows entry runs)
  DisplayForge.exe --list                list monitors and what each supports
  DisplayForge.exe --brightness <0-100> [--monitor <N>]
                                         set brightness (all monitors, or number N)
  DisplayForge.exe --preset <name>       apply a preset (Day, Evening, Night, Reading ...)
  DisplayForge.exe --quit                ask the running DisplayForge to exit
  DisplayForge.exe --demo                invented monitors; nothing on this PC is changed
  DisplayForge.exe --screenshot <png> [--view <name>] [--light]
                                         render a page of the demo to a PNG and exit
  DisplayForge.exe --selftest            run the engine against simulated monitors and
                                         check the results
  --data <folder>                        use this settings folder instead of
                                         %LOCALAPPDATA%\\DisplayForge (also: DISPLAYFORGE_DATA)

When DisplayForge is already running, --brightness and --preset are handed to it.
DisplayForge makes no network requests.";

fn main() -> eframe::Result<()> {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let has = |flag: &str| args.iter().any(|a| a == flag);
    let value = |flag: &str| {
        args.iter()
            .position(|a| a == flag)
            .and_then(|i| args.get(i + 1))
            .cloned()
    };

    if has("--help") || has("-h") || has("/?") {
        displayforge::sys::attach_parent_console();
        println!("{USAGE}");
        return Ok(());
    }
    if has("--selftest") {
        displayforge::sys::attach_parent_console();
        match displayforge::selftest::run() {
            Ok(log) => {
                for line in log {
                    println!("{line}");
                }
                println!("SELFTEST PASSED");
                std::process::exit(0);
            }
            Err(e) => {
                println!("{e}");
                println!("SELFTEST FAILED");
                std::process::exit(1);
            }
        }
    }
    let data_dir = value("--data")
        .map(PathBuf::from)
        .unwrap_or_else(settings::default_data_dir);

    if has("--list") || has("--brightness") || has("--preset") {
        displayforge::sys::attach_parent_console();
        let code = displayforge::cli::run(&args, &data_dir);
        std::process::exit(code);
    }
    if has("--quit") {
        displayforge::engine::signal_running(&UiRequest::Quit);
        return Ok(());
    }

    // The demo and screenshots: invented monitors, mock hardware, nothing saved.
    if has("--demo") || has("--screenshot") {
        let shared = displayforge::demo::shared();
        let engine = Engine::start(
            shared.clone(),
            Box::new(displayforge::demo::backend()),
            Box::new(displayforge::mock::MockDisplays::new(
                displayforge::demo::displays(),
            )),
            EngineOptions { system: false },
        );
        let view = value("--view").or_else(|| std::env::var("DISPLAYFORGE_VIEW").ok());
        let options = Options {
            mode: Mode::Main,
            screenshot: value("--screenshot").map(PathBuf::from),
            view,
            light: has("--light"),
            shared: Some(shared),
            control: Some(engine.control()),
        };
        let result = ui::run(options, window_icon());
        engine.stop();
        return result;
    }

    // One DisplayForge per user session: a second start opens the window of the first.
    let Some(_instance) = displayforge::sys::single_instance("DisplayForge.Instance") else {
        displayforge::engine::signal_running(&UiRequest::Open(value("--view")));
        return Ok(());
    };

    let settings = Settings::load(&data_dir);
    let shared = Arc::new(Shared::new(
        data_dir.clone(),
        settings,
        Arc::new(displayforge::timeutil::SystemTz),
        false,
    ));
    let engine = Engine::start(
        shared.clone(),
        Box::new(displayforge::hw::WinBackend::new()),
        Box::new(displayforge::hw::display::WinDisplays::new()),
        EngineOptions { system: true },
    );

    let mut next: Option<UiRequest> = if has("--background") {
        None
    } else {
        Some(UiRequest::Open(value("--view")))
    };
    loop {
        if let Some(request) = next.take() {
            let mode = match request {
                UiRequest::Quit => break,
                UiRequest::Flyout => Mode::Flyout,
                UiRequest::Open(_) => Mode::Main,
            };
            let view = match request {
                UiRequest::Open(view) => view,
                _ => None,
            };
            shared.window_open.store(true, Ordering::SeqCst);
            let options = Options {
                mode,
                view,
                shared: Some(shared.clone()),
                control: Some(engine.control()),
                ..Default::default()
            };
            let result = ui::run(options, window_icon());
            shared.window_open.store(false, Ordering::SeqCst);
            shared
                .closed_at_ms
                .store(displayforge::service::now_ms(), Ordering::SeqCst);
            if result.is_err() || shared.quit.load(Ordering::SeqCst) {
                break;
            }
            // A request that arrived while the window was closing.
            let pending = shared.take_requests();
            if let Some(r) = pending.into_iter().last() {
                next = Some(r);
                continue;
            }
            if mode == Mode::Main && !shared.settings().close_to_tray {
                break;
            }
        }
        next = Some(engine.wait_request());
    }
    engine.stop();
    Ok(())
}

fn window_icon() -> egui::IconData {
    let bytes = include_bytes!(concat!(env!("OUT_DIR"), "/icon-256.png"));
    match image::load_from_memory(bytes) {
        Ok(img) => {
            let rgba = img.into_rgba8();
            let (width, height) = rgba.dimensions();
            egui::IconData {
                rgba: rgba.into_raw(),
                width,
                height,
            }
        }
        Err(_) => egui::IconData::default(),
    }
}
