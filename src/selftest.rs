//! `--selftest`: drives the real engine logic against simulated monitors
//! (the mock backend) in a temporary folder and checks what reaches the
//! "hardware". Nothing on this PC is changed.

use crate::engine::{Cmd, Shared};
use crate::mock::{MockBackend, MockDisplays};
use crate::model::{DisplayChange, Feature};
use crate::policy::{Core, Inputs};
use crate::profiles::Foreground;
use crate::revert::{RevertState, RevertTimer, CONFIRM_SECONDS};
use crate::settings::{AppProfile, Settings, Trigger, KELVIN_NEUTRAL};
use crate::{demo, timeutil};
use std::sync::Arc;
use std::time::{Duration, Instant};

struct Run {
    log: Vec<String>,
}

impl Run {
    fn check(&mut self, ok: bool, what: &str) -> Result<(), String> {
        if ok {
            self.log.push(format!("ok    {what}"));
            Ok(())
        } else {
            self.log.push(format!("FAIL  {what}"));
            Err(self.log.join("\n"))
        }
    }
}

fn settle(core: &Core) {
    core.controller.wait_idle(Duration::from_secs(10));
}

pub fn run() -> Result<Vec<String>, String> {
    let dir = std::env::temp_dir().join(format!("displayforge-selftest-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    let result = run_in(&dir);
    let _ = std::fs::remove_dir_all(&dir);
    result
}

fn run_in(dir: &std::path::Path) -> Result<Vec<String>, String> {
    let mut r = Run { log: Vec::new() };
    let mut settings = demo::settings();
    settings.schedule.enabled = false;
    settings.idle.after_minutes = 3;
    settings.idle.brightness = 5;
    settings.below_zero = true;
    let mut shared = Shared::new(dir.to_path_buf(), settings, demo::tz(), false);
    shared.fixed_now = Some(demo::now());
    let shared = Arc::new(shared);
    let backend: MockBackend = demo::backend();
    let displays = MockDisplays::new(demo::displays());
    let mut core = Core::new(
        shared.clone(),
        Box::new(backend.clone()),
        Box::new(displays.clone()),
        Box::new(|| {}),
    );
    let start = Instant::now();
    while !core.controller.ready() && start.elapsed() < Duration::from_secs(10) {
        std::thread::sleep(Duration::from_millis(10));
    }
    core.sync_monitors();
    let now = demo::now();
    let idle = |secs| Inputs {
        now,
        front: None,
        idle_secs: secs,
    };

    // 1. Start: four monitors found, nothing written.
    core.tick(&idle(0));
    settle(&core);
    r.check(
        core.controller.snapshot().len() == 4,
        "four simulated monitors found (laptop, two DDC/CI, one without DDC/CI)",
    )?;
    r.check(
        backend.log().writes.is_empty(),
        "starting changes no monitor setting",
    )?;

    // 2. Link all: one value moves every linked monitor, through calibration.
    core.handle(Cmd::Brightness {
        key: None,
        value: 70,
    });
    settle(&core);
    r.check(
        backend.value(demo::KEY_MAIN, Feature::Brightness) == Some(70)
            && backend.value(demo::KEY_LAPTOP, Feature::Brightness) == Some(70)
            && backend.value(demo::KEY_SIDE, Feature::Brightness) == Some(66),
        "brightness 70 on all linked monitors (calibrated 10-90 monitor gets 66)",
    )?;
    let writes = backend.log().writes.len();
    for v in 0..40 {
        core.handle(Cmd::Brightness {
            key: Some(demo::KEY_MAIN.into()),
            value: 30 + v,
        });
    }
    settle(&core);
    let burst = backend.log().writes.len() - writes;
    r.check(
        backend.value(demo::KEY_MAIN, Feature::Brightness) == Some(69) && burst < 40 * 4,
        &format!("40 quick slider moves end on the last value ({burst} write(s), coalesced)"),
    )?;

    // 3. Presets and night light.
    core.handle(Cmd::Preset("Night".into()));
    settle(&core);
    let gamma = backend.log().gamma.len();
    r.check(
        backend.value(demo::KEY_MAIN, Feature::Brightness) == Some(25)
            && core.settings.night_light
            && gamma == 4,
        "preset Night: brightness 25 and a warm gamma ramp on all four screens",
    )?;
    let ramp = backend.log().gamma.get(demo::KEY_MAIN).cloned();
    r.check(
        ramp.is_some_and(|g| g.b[255] < g.r[255]),
        "the night-light ramp has less blue than red",
    )?;
    core.handle(Cmd::NightLight(false));
    settle(&core);
    r.check(
        backend.log().gamma.is_empty(),
        "night light off restores the normal ramp everywhere",
    )?;

    // 4. Below zero: hardware at its minimum plus software dimming.
    core.handle(Cmd::Brightness {
        key: Some(demo::KEY_MAIN.into()),
        value: -30,
    });
    settle(&core);
    r.check(
        backend.value(demo::KEY_MAIN, Feature::Brightness) == Some(0)
            && backend.log().gamma.contains_key(demo::KEY_MAIN),
        "brightness -30: hardware at 0 and a darker gamma ramp",
    )?;
    core.handle(Cmd::Brightness {
        key: Some(demo::KEY_MAIN.into()),
        value: 50,
    });
    settle(&core);
    r.check(
        !backend.log().gamma.contains_key(demo::KEY_MAIN),
        "back above zero removes the software dimming",
    )?;

    // 5. Per-app profile, then restore.
    if let Ok(mut s) = shared.settings.lock() {
        s.profiles = vec![AppProfile {
            exe: "player.exe".into(),
            trigger: Trigger::Fullscreen,
            brightness: Some(100),
            contrast: Some(90),
            kelvin: Some(KELVIN_NEUTRAL),
            ..AppProfile::default()
        }];
    }
    core.handle(Cmd::SettingsChanged);
    let player = Foreground {
        exe: "player.exe".into(),
        fullscreen: true,
        monitor: Some(demo::KEY_MAIN.into()),
    };
    core.tick(&Inputs {
        now,
        front: Some(player),
        idle_secs: 0,
    });
    settle(&core);
    r.check(
        backend.value(demo::KEY_MAIN, Feature::Brightness) == Some(100)
            && backend.value(demo::KEY_MAIN, Feature::Contrast) == Some(90),
        "full-screen player: profile sets brightness 100 and contrast 90",
    )?;
    core.tick(&idle(0));
    settle(&core);
    r.check(
        backend.value(demo::KEY_MAIN, Feature::Brightness) == Some(50)
            && backend.value(demo::KEY_MAIN, Feature::Contrast) == Some(70),
        "player closed: brightness and contrast restored",
    )?;

    // 6. Idle dimming.
    if let Ok(mut s) = shared.settings.lock() {
        s.idle.enabled = true;
    }
    core.handle(Cmd::SettingsChanged);
    core.tick(&idle(4 * 60));
    settle(&core);
    r.check(
        backend.value(demo::KEY_MAIN, Feature::Brightness) == Some(5),
        "idle for 4 minutes: dimmed to 5",
    )?;
    core.tick(&idle(0));
    settle(&core);
    r.check(
        backend.value(demo::KEY_MAIN, Feature::Brightness) == Some(50),
        "input again: brightness back",
    )?;

    // 7. Schedule across midnight.
    if let Ok(mut s) = shared.settings.lock() {
        s.idle.enabled = false;
        s.schedule = crate::settings::Schedule::default();
        s.schedule.enabled = true;
        s.schedule.transition_minutes = 0;
    }
    core.handle(Cmd::SettingsChanged);
    core.handle(Cmd::ResumeSchedule);
    let tz = demo::tz();
    let day = timeutil::local_day(now, tz.as_ref());
    let late = timeutil::local_to_utc(day + 1, 2 * 60, tz.as_ref());
    core.tick(&Inputs {
        now: late,
        front: None,
        idle_secs: 0,
    });
    settle(&core);
    r.check(
        backend.value(demo::KEY_MAIN, Feature::Brightness) == Some(30),
        "02:00: the 22:30 schedule point (brightness 30) still applies after midnight",
    )?;

    // 8. Display change, then the 15-second revert.
    let d = demo::displays()[1].clone();
    let smaller = d.modes.iter().find(|m| m.width < d.current.width).copied();
    core.handle(Cmd::ApplyDisplay {
        device: d.device.clone(),
        change: DisplayChange {
            mode: smaller,
            ..Default::default()
        },
    });
    let before = shared
        .live()
        .display_result
        .and_then(|(_, r)| r.ok())
        .unwrap_or_default();
    let mut timer = RevertTimer::new(0, CONFIRM_SECONDS);
    let state = timer.tick(CONFIRM_SECONDS * 1000);
    if state == RevertState::Revert {
        core.handle(Cmd::ApplyDisplay {
            device: d.device.clone(),
            change: before,
        });
    }
    let now_mode = displays.state.lock().ok().map(|s| s[1].current);
    r.check(
        state == RevertState::Revert && now_mode == Some(d.current),
        "unconfirmed resolution change is reverted after 15 seconds (simulated display)",
    )?;

    // 9. Settings saved, exported and imported.
    core.handle(Cmd::Kelvin(3800));
    let saved = Settings::load(dir);
    let json = saved.to_json();
    r.check(
        saved.kelvin == 3800 && Settings::from_json(&json).ok() == Some(saved.clone()),
        "settings saved to the folder and round-trip through export/import",
    )?;

    // 10. Parsers.
    let caps = crate::hw::caps::parse(
        "(prot(monitor)type(LCD)model(TEST 27)cmds(01 02 03)vcp(10 12 14(05 08) 60(0F 11) D6(01 04))mccs_ver(2.1))",
    );
    r.check(
        caps.supports(0x10) && caps.values(0x60) == [0x0F, 0x11],
        "capabilities string parsed (brightness, two inputs)",
    )?;
    r.check(
        crate::hotkey::parse("Ctrl+Alt+Up").is_ok() && crate::hotkey::parse("Up").is_err(),
        "hotkey text parsed",
    )?;

    // 11. Shutting down leaves no gamma ramp behind.
    core.handle(Cmd::NightLight(true));
    settle(&core);
    let had = backend.log().gamma.len();
    core.shutdown();
    r.check(
        had == 4 && backend.log().gamma.is_empty(),
        "quitting restores the normal gamma ramp on every screen",
    )?;
    Ok(r.log)
}

#[cfg(test)]
mod tests {
    #[test]
    fn selftest_passes() {
        match super::run() {
            Ok(log) => assert!(log.iter().all(|l| l.starts_with("ok"))),
            Err(log) => panic!("{log}"),
        }
    }
}
