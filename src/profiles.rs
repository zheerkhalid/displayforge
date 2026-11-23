//! Per-app profiles and the precedence between manual, schedule, profile and idle values.
use crate::model::MonitorKey;
use crate::settings::{AppProfile, Trigger, KELVIN_MIN, KELVIN_NEUTRAL};

/// The app in the foreground.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Foreground {
    /// Executable file name, lower case, e.g. "vlc.exe".
    pub exe: String,
    pub fullscreen: bool,
    /// The monitor its window is on.
    pub monitor: Option<MonitorKey>,
}

/// The comparable form of an executable name: file name only, lower case,
/// without a trailing ".exe" ("C:\Apps\VLC.EXE" -> "vlc").
pub fn normalize_exe(name: &str) -> String {
    let file = name
        .trim()
        .rsplit(['\\', '/'])
        .next()
        .unwrap_or("")
        .trim()
        .to_lowercase();
    match file.strip_suffix(".exe") {
        Some(stem) => stem.to_string(),
        None => file,
    }
}

/// True when a profile's `exe` pattern names the foreground executable.
pub fn exe_matches(pattern: &str, exe: &str) -> bool {
    let p = normalize_exe(pattern);
    !p.is_empty() && p == normalize_exe(exe)
}

/// The profile that applies to the foreground app: the first enabled match in list order.
/// A `Fullscreen` profile only matches while the app is full screen.
pub fn pick<'a>(profiles: &'a [AppProfile], fg: &Foreground) -> Option<(usize, &'a AppProfile)> {
    profiles.iter().enumerate().find(|(_, p)| {
        p.enabled
            && exe_matches(&p.exe, &fg.exe)
            && (p.trigger != Trigger::Fullscreen || fg.fullscreen)
    })
}

/// The layers that decide a monitor's brightness.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Layers {
    /// What the user (or the last schedule point / preset) set.
    pub base: i32,
    /// An active app profile's brightness.
    pub profile: Option<i32>,
    /// The idle-dim level while idle.
    pub idle: Option<i32>,
}

/// The brightness to apply: a profile's value replaces the base; idle
/// dimming then lowers it (never raises it).
pub fn effective_brightness(l: &Layers) -> i32 {
    let value = l.profile.unwrap_or(l.base);
    match l.idle {
        Some(idle) => idle.min(value),
        None => value,
    }
}

/// The colour temperature to apply: the profile's, else night light's (or
/// neutral when off), within `KELVIN_MIN..=KELVIN_NEUTRAL`.
pub fn effective_kelvin(night_on: bool, kelvin: u32, profile: Option<u32>) -> u32 {
    let k = match profile {
        Some(k) => k,
        None if night_on => kelvin,
        None => KELVIN_NEUTRAL,
    };
    k.clamp(KELVIN_MIN, KELVIN_NEUTRAL)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn prof(exe: &str, trigger: Trigger, brightness: Option<i32>) -> AppProfile {
        AppProfile {
            exe: exe.into(),
            trigger,
            brightness,
            ..Default::default()
        }
    }

    fn fg(exe: &str, fullscreen: bool) -> Foreground {
        Foreground {
            exe: exe.into(),
            fullscreen,
            monitor: None,
        }
    }

    #[test]
    fn exe_names_match_loosely() {
        assert!(exe_matches("vlc.exe", "vlc.exe"));
        assert!(exe_matches("VLC", "vlc.exe"));
        assert!(exe_matches("vlc.EXE", "VLC.exe"));
        assert!(exe_matches(
            " C:\\Program Files\\VideoLAN\\vlc.exe ",
            "vlc.exe"
        ));
        assert!(exe_matches("vlc.exe", "D:/apps/VLC.EXE"));
        assert!(!exe_matches("vlc", "vlcx.exe"));
        assert!(!exe_matches("", "vlc.exe"));
        assert!(!exe_matches("   ", ""));
        assert!(!exe_matches("game", "game.exe.bak"));
        assert_eq!(normalize_exe("a\\B.Exe"), "b");
    }

    #[test]
    fn first_enabled_match_wins() {
        let mut list = vec![
            prof("other.exe", Trigger::Foreground, Some(10)),
            prof("vlc.exe", Trigger::Foreground, Some(20)),
            prof("vlc", Trigger::Foreground, Some(30)),
        ];
        assert_eq!(pick(&list, &fg("vlc.exe", false)).unwrap().0, 1);
        list[1].enabled = false;
        let (i, p) = pick(&list, &fg("vlc.exe", false)).unwrap();
        assert_eq!((i, p.brightness), (2, Some(30)));
        assert!(pick(&list, &fg("notepad.exe", false)).is_none());
        assert!(pick(&[], &fg("vlc.exe", false)).is_none());
    }

    #[test]
    fn fullscreen_trigger_needs_fullscreen() {
        let list = vec![
            prof("game.exe", Trigger::Fullscreen, Some(100)),
            prof("game.exe", Trigger::Foreground, Some(50)),
        ];
        assert_eq!(pick(&list, &fg("game.exe", true)).unwrap().0, 0);
        assert_eq!(pick(&list, &fg("game.exe", false)).unwrap().0, 1);
        let only_fs = vec![prof("game.exe", Trigger::Fullscreen, Some(100))];
        assert!(pick(&only_fs, &fg("game.exe", false)).is_none());
        // A foreground profile also applies in full screen.
        let only_fg = vec![prof("game.exe", Trigger::Foreground, Some(100))];
        assert!(pick(&only_fg, &fg("game.exe", true)).is_some());
    }

    #[test]
    fn brightness_precedence() {
        let l = |base, profile, idle| Layers {
            base,
            profile,
            idle,
        };
        assert_eq!(effective_brightness(&l(60, None, None)), 60);
        assert_eq!(effective_brightness(&l(60, Some(100), None)), 100);
        assert_eq!(effective_brightness(&l(60, Some(20), None)), 20);
        // Idle dims ...
        assert_eq!(effective_brightness(&l(60, None, Some(10))), 10);
        assert_eq!(effective_brightness(&l(60, Some(100), Some(10))), 10);
        // ... but never brightens.
        assert_eq!(effective_brightness(&l(5, None, Some(10))), 5);
        assert_eq!(effective_brightness(&l(60, Some(3), Some(10))), 3);
        assert_eq!(effective_brightness(&l(-30, None, Some(10))), -30);
    }

    #[test]
    fn kelvin_precedence() {
        assert_eq!(effective_kelvin(false, 3400, None), 6500);
        assert_eq!(effective_kelvin(true, 3400, None), 3400);
        assert_eq!(effective_kelvin(true, 3400, Some(6500)), 6500);
        assert_eq!(effective_kelvin(false, 3400, Some(4000)), 4000);
        assert_eq!(effective_kelvin(true, 100, None), KELVIN_MIN);
        assert_eq!(effective_kelvin(true, 3400, Some(9000)), KELVIN_NEUTRAL);
    }
}
