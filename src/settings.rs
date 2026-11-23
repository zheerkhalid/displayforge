//! User settings, saved as JSON in `%LOCALAPPDATA%\DisplayForge\settings.json`.
//!
//! Every field has a default so that older or hand-edited files still load.

use crate::model::{Feature, MonitorKey};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

pub const FILE: &str = "settings.json";

/// Lowest value of the brightness scale when dimming below zero is on: -50
/// means "hardware at its minimum, then 50 % darker in software".
pub const MIN_BELOW_ZERO: i32 = -50;

/// Night-light colour temperature limits (Kelvin). 6500 K is neutral.
pub const KELVIN_MIN: u32 = 1900;
pub const KELVIN_NEUTRAL: u32 = 6500;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum ThemeChoice {
    #[default]
    System,
    Dark,
    Light,
}

/// How brightness below the hardware minimum (and dimming on monitors
/// without hardware control) is done.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum DimMethod {
    /// Darken the gamma ramp. Invisible to screenshots, may be limited by Windows.
    #[default]
    Gamma,
    /// A dark, click-through window over the monitor.
    Overlay,
}

/// Per-monitor preferences.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct MonitorPrefs {
    /// A name chosen by the user (empty = the monitor's own name).
    pub rename: String,
    /// Calibration: the hardware percentage that brightness 0 and 100 map to.
    pub min: u32,
    pub max: u32,
    /// Included when "link all" moves every monitor together.
    pub linked: bool,
    /// Hidden from the tray flyout.
    pub hidden: bool,
    pub dim_method: DimMethod,
}

impl Default for MonitorPrefs {
    fn default() -> Self {
        MonitorPrefs {
            rename: String::new(),
            min: 0,
            max: 100,
            linked: true,
            hidden: false,
            dim_method: DimMethod::Gamma,
        }
    }
}

/// A named set of values applied with one click (tray, hotkey, schedule, `--preset`).
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Preset {
    pub name: String,
    /// Brightness for every monitor (may be below zero when that is enabled).
    pub brightness: i32,
    /// Contrast for monitors that support it; `None` leaves it alone.
    pub contrast: Option<u32>,
    /// Night light: `Some(kelvin)` turns it on at that temperature, `None` turns it off.
    pub kelvin: Option<u32>,
    /// Per-monitor brightness overrides.
    pub per_monitor: BTreeMap<MonitorKey, i32>,
}

impl Default for Preset {
    fn default() -> Self {
        Preset {
            name: "Preset".into(),
            brightness: 70,
            contrast: None,
            kelvin: None,
            per_monitor: BTreeMap::new(),
        }
    }
}

pub fn default_presets() -> Vec<Preset> {
    let p = |name: &str, brightness: i32, contrast: Option<u32>, kelvin: Option<u32>| Preset {
        name: name.into(),
        brightness,
        contrast,
        kelvin,
        per_monitor: BTreeMap::new(),
    };
    vec![
        p("Day", 80, None, None),
        p("Evening", 55, None, Some(4500)),
        p("Night", 25, None, Some(3200)),
        p("Reading", 45, None, Some(5000)),
    ]
}

/// When a schedule point takes effect.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum When {
    /// Local clock time, minutes after midnight.
    Time { minutes: u32 },
    /// Sunrise at the chosen location, plus an offset in minutes (may be negative).
    Sunrise { offset: i32 },
    /// Sunset at the chosen location, plus an offset in minutes (may be negative).
    Sunset { offset: i32 },
}

/// One point of the daily schedule: from `when` on, these values apply.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct SchedulePoint {
    pub when: When,
    /// Brightness for every monitor, or `None` to leave brightness alone.
    pub brightness: Option<i32>,
    /// Night-light temperature, `Some(6500)` = neutral; `None` leaves it alone.
    pub kelvin: Option<u32>,
    /// Apply a preset instead (its values win over the two above).
    pub preset: Option<String>,
}

impl Default for SchedulePoint {
    fn default() -> Self {
        SchedulePoint {
            when: When::Time { minutes: 8 * 60 },
            brightness: Some(70),
            kelvin: None,
            preset: None,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Schedule {
    pub enabled: bool,
    pub points: Vec<SchedulePoint>,
    /// Gradual change: values move from the previous point's to this point's
    /// over this many minutes, starting at the point's time. 0 = at once.
    pub transition_minutes: u32,
}

impl Default for Schedule {
    fn default() -> Self {
        Schedule {
            enabled: false,
            points: vec![
                SchedulePoint {
                    when: When::Sunrise { offset: 0 },
                    brightness: Some(80),
                    kelvin: Some(KELVIN_NEUTRAL),
                    preset: None,
                },
                SchedulePoint {
                    when: When::Sunset { offset: -30 },
                    brightness: Some(55),
                    kelvin: Some(4500),
                    preset: None,
                },
                SchedulePoint {
                    when: When::Time {
                        minutes: 22 * 60 + 30,
                    },
                    brightness: Some(30),
                    kelvin: Some(3400),
                    preset: None,
                },
            ],
            transition_minutes: 30,
        }
    }
}

/// A place for sunrise and sunset, picked from the built-in city list or typed in.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Location {
    pub name: String,
    /// Degrees, north positive.
    pub lat: f64,
    /// Degrees, east positive.
    pub lon: f64,
}

/// When a per-app profile applies.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum Trigger {
    /// While the app's window is in the foreground.
    #[default]
    Foreground,
    /// Only while the app is in the foreground and full screen.
    Fullscreen,
}

/// Apply values while an app is in front; restore when it is not.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct AppProfile {
    pub enabled: bool,
    /// Executable file name, e.g. "vlc.exe" (matched case-insensitively).
    pub exe: String,
    pub trigger: Trigger,
    pub brightness: Option<i32>,
    pub contrast: Option<u32>,
    /// `Some(6500)` pauses night light while the app is in front.
    pub kelvin: Option<u32>,
    /// Only the monitor the app's window is on (else all monitors).
    pub only_its_monitor: bool,
}

impl Default for AppProfile {
    fn default() -> Self {
        AppProfile {
            enabled: true,
            exe: String::new(),
            trigger: Trigger::Foreground,
            brightness: Some(100),
            contrast: None,
            kelvin: Some(KELVIN_NEUTRAL),
            only_its_monitor: false,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct IdleDim {
    pub enabled: bool,
    /// Dim after this many minutes without keyboard or mouse input.
    pub after_minutes: u32,
    /// Dim to this brightness (or the current one, if lower).
    pub brightness: i32,
    /// Do not dim while a full-screen app (a video, a game) is in front.
    pub skip_fullscreen: bool,
}

impl Default for IdleDim {
    fn default() -> Self {
        IdleDim {
            enabled: false,
            after_minutes: 5,
            brightness: 10,
            skip_fullscreen: true,
        }
    }
}

/// What a global hotkey does.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum HotAction {
    BrightnessUp,
    BrightnessDown,
    Preset(String),
    ToggleNightLight,
    /// Turns all screens off (they wake on input).
    ScreensOff,
    OpenFlyout,
}

impl HotAction {
    pub fn label(&self) -> String {
        match self {
            HotAction::BrightnessUp => "Brightness up".into(),
            HotAction::BrightnessDown => "Brightness down".into(),
            HotAction::Preset(name) => format!("Preset: {name}"),
            HotAction::ToggleNightLight => "Night light on/off".into(),
            HotAction::ScreensOff => "Turn screens off".into(),
            HotAction::OpenFlyout => "Show the quick panel".into(),
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Hotkey {
    pub action: HotAction,
    /// e.g. "Ctrl+Alt+Up" (see `hotkey::parse`). Empty = not set.
    pub keys: String,
    pub enabled: bool,
}

pub fn default_hotkeys() -> Vec<Hotkey> {
    let h = |action: HotAction, keys: &str| Hotkey {
        action,
        keys: keys.into(),
        enabled: false,
    };
    vec![
        h(HotAction::BrightnessUp, "Ctrl+Alt+Up"),
        h(HotAction::BrightnessDown, "Ctrl+Alt+Down"),
        h(HotAction::ToggleNightLight, "Ctrl+Alt+N"),
        h(HotAction::ScreensOff, "Ctrl+Alt+End"),
        h(HotAction::OpenFlyout, "Ctrl+Alt+B"),
        h(HotAction::Preset("Day".into()), "Ctrl+Alt+1"),
        h(HotAction::Preset("Night".into()), "Ctrl+Alt+2"),
    ]
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Settings {
    pub theme: ThemeChoice,
    /// Closing the window keeps DisplayForge in the notification area.
    pub close_to_tray: bool,
    /// Fade brightness changes instead of jumping.
    pub fade: bool,
    /// Length of a fade for a manual change, milliseconds.
    pub fade_ms: u32,
    /// Step for hotkeys and the mouse wheel on the tray icon (percentage points).
    pub step: u32,
    /// The mouse wheel over the tray icon changes brightness.
    pub scroll_on_tray: bool,
    /// Allow brightness below zero (software dimming past the hardware minimum).
    pub below_zero: bool,
    /// The tray flyout moves all linked monitors together.
    pub link_all: bool,
    /// Show the serial number on the monitor information page.
    pub show_serials: bool,
    pub monitors: BTreeMap<MonitorKey, MonitorPrefs>,
    pub presets: Vec<Preset>,
    pub night_light: bool,
    pub kelvin: u32,
    pub schedule: Schedule,
    pub location: Option<Location>,
    pub profiles: Vec<AppProfile>,
    pub idle: IdleDim,
    pub hotkeys: Vec<Hotkey>,
    /// Which hardware features the monitor page shows, beyond brightness.
    pub show_features: Vec<Feature>,
}

impl Default for Settings {
    fn default() -> Self {
        Settings {
            theme: ThemeChoice::System,
            close_to_tray: true,
            fade: true,
            fade_ms: 400,
            step: 10,
            scroll_on_tray: true,
            below_zero: false,
            link_all: false,
            show_serials: false,
            monitors: BTreeMap::new(),
            presets: default_presets(),
            night_light: false,
            kelvin: 4500,
            schedule: Schedule::default(),
            location: None,
            profiles: Vec::new(),
            idle: IdleDim::default(),
            hotkeys: default_hotkeys(),
            show_features: vec![Feature::Contrast, Feature::Volume, Feature::InputSource],
        }
    }
}

impl Settings {
    pub fn load(dir: &Path) -> Settings {
        std::fs::read_to_string(dir.join(FILE))
            .ok()
            .and_then(|text| Settings::from_json(&text).ok())
            .unwrap_or_default()
    }

    pub fn save(&self, dir: &Path) -> std::io::Result<()> {
        std::fs::create_dir_all(dir)?;
        let tmp = dir.join("settings.json.tmp");
        std::fs::write(&tmp, self.to_json())?;
        std::fs::rename(&tmp, dir.join(FILE))
    }

    pub fn to_json(&self) -> String {
        serde_json::to_string_pretty(self).unwrap_or_default()
    }

    /// Parses settings (also used for import) and repairs out-of-range values.
    pub fn from_json(text: &str) -> Result<Settings, String> {
        let mut s: Settings = serde_json::from_str(text).map_err(|e| e.to_string())?;
        s.sanitize();
        Ok(s)
    }

    pub fn sanitize(&mut self) {
        self.kelvin = self.kelvin.clamp(KELVIN_MIN, KELVIN_NEUTRAL);
        self.step = self.step.clamp(1, 50);
        self.fade_ms = self.fade_ms.min(3000);
        self.schedule.transition_minutes = self.schedule.transition_minutes.min(180);
        self.idle.after_minutes = self.idle.after_minutes.clamp(1, 240);
        for m in self.monitors.values_mut() {
            m.min = m.min.min(100);
            m.max = m.max.min(100);
        }
        if self.presets.is_empty() {
            self.presets = default_presets();
        }
    }

    pub fn prefs(&self, key: &str) -> MonitorPrefs {
        self.monitors.get(key).cloned().unwrap_or_default()
    }

    pub fn prefs_mut(&mut self, key: &str) -> &mut MonitorPrefs {
        self.monitors.entry(key.to_string()).or_default()
    }

    pub fn preset(&self, name: &str) -> Option<&Preset> {
        self.presets
            .iter()
            .find(|p| p.name.eq_ignore_ascii_case(name))
    }

    /// The lowest brightness the sliders allow.
    pub fn min_brightness(&self) -> i32 {
        if self.below_zero {
            MIN_BELOW_ZERO
        } else {
            0
        }
    }
}

pub fn default_data_dir() -> PathBuf {
    if let Some(dir) = std::env::var_os("DISPLAYFORGE_DATA") {
        return PathBuf::from(dir);
    }
    std::env::var_os("LOCALAPPDATA")
        .map(PathBuf::from)
        .unwrap_or_else(std::env::temp_dir)
        .join("DisplayForge")
}

#[cfg(test)]
#[allow(clippy::field_reassign_with_default)]
mod tests {
    use super::*;

    #[test]
    fn round_trip_and_defaults() {
        let s = Settings::default();
        let back = Settings::from_json(&s.to_json()).unwrap();
        assert_eq!(s, back);
        let partial = Settings::from_json(r#"{"kelvin": 100, "step": 0}"#).unwrap();
        assert_eq!(partial.kelvin, KELVIN_MIN);
        assert_eq!(partial.step, 1);
        assert_eq!(partial.presets.len(), 4);
        assert!(Settings::from_json("not json").is_err());
    }

    #[test]
    fn save_and_load() {
        let dir =
            std::env::temp_dir().join(format!("displayforge-settings-{}", std::process::id()));
        let mut s = Settings::default();
        s.kelvin = 3300;
        s.prefs_mut("ABC").rename = "Left".into();
        s.save(&dir).unwrap();
        let back = Settings::load(&dir);
        assert_eq!(back.kelvin, 3300);
        assert_eq!(back.prefs("ABC").rename, "Left");
        assert_eq!(back.prefs("missing"), MonitorPrefs::default());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn presets_by_name() {
        let s = Settings::default();
        assert_eq!(s.preset("night").unwrap().brightness, 25);
        assert!(s.preset("nope").is_none());
        assert_eq!(s.min_brightness(), 0);
    }
}
