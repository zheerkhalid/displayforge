//! Command-line actions: `--list`, `--brightness`, `--preset`.
//!
//! When DisplayForge is already running, `--brightness` and `--preset` are
//! handed to it, so its sliders, schedule and night light stay in step.
//! Otherwise they talk to the monitors directly (night light and dimming
//! below zero need the running app, because a gamma ramp is not kept after
//! DisplayForge exits).

use crate::engine::{Cmd, Shared};
use crate::model::{Connection, Feature};
use crate::policy::Core;
use crate::settings::Settings;
use std::path::Path;
use std::sync::Arc;
use std::time::{Duration, Instant};

fn value_after(args: &[String], flag: &str) -> Option<String> {
    args.iter()
        .position(|a| a == flag)
        .and_then(|i| args.get(i + 1))
        .cloned()
}

/// What a command line asks for.
#[derive(Debug, PartialEq, Eq)]
pub enum Action {
    List,
    Brightness { value: i32, monitor: Option<usize> },
    Preset(String),
}

pub fn parse(args: &[String]) -> Result<Action, String> {
    if args.iter().any(|a| a == "--list") {
        return Ok(Action::List);
    }
    if let Some(name) = value_after(args, "--preset") {
        return Ok(Action::Preset(name));
    }
    if args.iter().any(|a| a == "--preset") {
        return Err("--preset needs a name, e.g. --preset Night".into());
    }
    if args.iter().any(|a| a == "--brightness") {
        let raw =
            value_after(args, "--brightness").ok_or("--brightness needs a value from 0 to 100")?;
        let value: i32 = raw
            .trim_end_matches('%')
            .parse()
            .map_err(|_| format!("\"{raw}\" is not a number from 0 to 100"))?;
        if !(0..=100).contains(&value) {
            return Err(format!("{value} is outside 0 to 100"));
        }
        let monitor = match value_after(args, "--monitor") {
            Some(n) => Some(
                n.parse::<usize>()
                    .ok()
                    .filter(|n| *n >= 1)
                    .ok_or_else(|| format!("\"{n}\" is not a monitor number (see --list)"))?,
            ),
            None => None,
        };
        return Ok(Action::Brightness { value, monitor });
    }
    Err("nothing to do (see --help)".into())
}

/// Runs a command-line action and returns the process exit code.
pub fn run(args: &[String], data_dir: &Path) -> i32 {
    let action = match parse(args) {
        Ok(a) => a,
        Err(e) => {
            eprintln!("DisplayForge: {e}");
            return 2;
        }
    };
    // Hand over to a running DisplayForge when there is one.
    let handover = match &action {
        Action::Brightness { value, monitor } => Some(match monitor {
            Some(n) => format!("brightness {value} {n}"),
            None => format!("brightness {value}"),
        }),
        Action::Preset(name) => Some(format!("preset {name}")),
        Action::List => None,
    };
    if let Some(text) = handover {
        if crate::service::send_text(&text) {
            println!("Handed to the running DisplayForge.");
            return 0;
        }
    }

    let settings = Settings::load(data_dir);
    let shared = Arc::new(Shared::new(
        data_dir.to_path_buf(),
        settings,
        Arc::new(crate::timeutil::SystemTz),
        // Never save settings from the command line.
        true,
    ));
    let mut core = Core::new(
        shared,
        Box::new(crate::hw::WinBackend::new()),
        Box::new(crate::mock::MockDisplays::new(Vec::new())),
        Box::new(|| {}),
    );
    let start = Instant::now();
    while !core.controller.ready() && start.elapsed() < Duration::from_secs(30) {
        std::thread::sleep(Duration::from_millis(50));
    }
    core.sync_monitors();
    let mut snaps = core.controller.snapshot();
    snaps.sort_by_key(|s| s.monitor.number);
    let code = if snaps.is_empty() {
        eprintln!("DisplayForge: no monitors found.");
        1
    } else {
        match action {
            Action::List => {
                for s in &snaps {
                    let m = &s.monitor;
                    let mut line = format!(
                        "{}  {}  [{}]{}",
                        m.number,
                        m.name,
                        m.connection.label(),
                        if m.primary { "  primary" } else { "" }
                    );
                    for f in [Feature::Brightness, Feature::Contrast, Feature::Volume] {
                        if let Some(l) = s.levels.get(&f) {
                            line.push_str(&format!(
                                "  {} {}%",
                                f.label().to_lowercase(),
                                l.percent()
                            ));
                        }
                    }
                    println!("{line}");
                    let more: Vec<&str> = m
                        .features
                        .iter()
                        .filter(|f| !matches!(f, Feature::Brightness))
                        .map(|f| f.label())
                        .collect();
                    if !more.is_empty() {
                        println!("   also: {}", more.join(", "));
                    }
                    if let Some(note) = &m.note {
                        println!("   {note}");
                    }
                }
                0
            }
            Action::Brightness { value, monitor } => {
                let key = match monitor {
                    Some(n) => match snaps.iter().find(|s| s.monitor.number == n) {
                        Some(s) => Some(s.monitor.key.clone()),
                        None => {
                            eprintln!("DisplayForge: there is no monitor {n} (see --list).");
                            core.shutdown();
                            return 2;
                        }
                    },
                    None => None,
                };
                let hw: Vec<String> = snaps
                    .iter()
                    .filter(|s| key.as_ref().is_none_or(|k| *k == s.monitor.key))
                    .filter(|s| s.monitor.connection == Connection::SoftwareOnly)
                    .map(|s| s.monitor.name.clone())
                    .collect();
                core.handle(Cmd::Brightness { key, value });
                if !hw.is_empty() {
                    println!(
                        "Without DisplayForge running, these screens cannot be dimmed: {}",
                        hw.join(", ")
                    );
                }
                0
            }
            Action::Preset(name) => {
                if core.apply_preset(&name) {
                    println!(
                        "Applied \"{name}\" (brightness and contrast; night light needs DisplayForge running)."
                    );
                    0
                } else {
                    eprintln!("DisplayForge: there is no preset called \"{name}\".");
                    2
                }
            }
        }
    };
    core.controller.wait_idle(Duration::from_secs(15));
    let errors: Vec<String> = core
        .controller
        .snapshot()
        .into_iter()
        .filter_map(|s| s.error.map(|e| format!("{}: {e}", s.monitor.name)))
        .collect();
    core.shutdown();
    for e in &errors {
        eprintln!("{e}");
    }
    if code == 0 && !errors.is_empty() {
        1
    } else {
        code
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn a(s: &str) -> Vec<String> {
        s.split_whitespace().map(String::from).collect()
    }

    #[test]
    fn parses_actions() {
        assert_eq!(parse(&a("--list")), Ok(Action::List));
        assert_eq!(
            parse(&a("--brightness 40")),
            Ok(Action::Brightness {
                value: 40,
                monitor: None
            })
        );
        assert_eq!(
            parse(&a("--brightness 40% --monitor 2")),
            Ok(Action::Brightness {
                value: 40,
                monitor: Some(2)
            })
        );
        assert_eq!(
            parse(&a("--preset Night")),
            Ok(Action::Preset("Night".into()))
        );
        assert!(parse(&a("--brightness 140")).is_err());
        assert!(parse(&a("--brightness x")).is_err());
        assert!(parse(&a("--brightness 10 --monitor 0")).is_err());
        assert!(parse(&a("--preset")).is_err());
        assert!(parse(&a("")).is_err());
    }
}
