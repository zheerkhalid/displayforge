//! What every window and page works with: the shared state, the settings
//! being edited, commands to the engine, values shown while the engine
//! catches up, and toasts.

use crate::engine::{Cmd, Control, LiveState, MonitorView, Shared, UiRequest};
use crate::model::{Feature, MonitorKey};
use crate::settings::Settings;
use std::collections::HashMap;
use std::sync::Arc;
use std::time::{Duration, Instant};

/// How long a value set in the UI wins over the engine's last report.
const HOLD: Duration = Duration::from_millis(1500);
/// Settings are saved this long after the last change (sliders move often).
const SAVE_DELAY: Duration = Duration::from_millis(350);

pub struct Toast {
    pub text: String,
    pub error: bool,
    pub born: Instant,
}

/// A value the user just set, keyed by what it is for.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub enum Held {
    Brightness(MonitorKey),
    Feature(MonitorKey, Feature),
    LinkAll,
    Night,
    Kelvin,
}

pub struct Core {
    pub shared: Arc<Shared>,
    control: Option<Arc<dyn Control>>,
    pub demo: bool,
    /// The settings being edited; written back to `shared` when changed.
    pub settings: Settings,
    baseline: Settings,
    save_due: Option<Instant>,
    /// This frame's copy of the engine's state.
    pub live: LiveState,
    held: HashMap<Held, (i32, Instant)>,
    pub toasts: Vec<Toast>,
    /// The theme is forced (screenshots with `--light`).
    pub forced_light: bool,
}

impl Core {
    pub fn new(shared: Arc<Shared>, control: Option<Arc<dyn Control>>) -> Self {
        let settings = shared.settings();
        Core {
            demo: shared.demo,
            live: shared.live(),
            baseline: settings.clone(),
            settings,
            save_due: None,
            shared,
            control,
            held: HashMap::new(),
            toasts: Vec::new(),
            forced_light: false,
        }
    }

    /// Reads the engine's state and any settings it changed. Call at the start of a frame.
    pub fn begin_frame(&mut self) {
        self.live = self.shared.live();
        if let Some(notice) = self.live.notice.take() {
            if let Ok(mut live) = self.shared.live.lock() {
                live.notice = None;
            }
            let error = notice.starts_with("Could not") || notice.contains("failed");
            self.toast(notice, error);
        }
        let current = self.shared.settings();
        if current != self.baseline {
            // The engine changed something (a hotkey toggled night light ...).
            self.settings = current.clone();
            self.baseline = current;
        }
        self.held.retain(|_, (_, t)| t.elapsed() < HOLD);
    }

    /// Writes edited settings back and, a moment after the last change, tells
    /// the engine. Call at the end of a frame. Returns true while a save is pending.
    pub fn end_frame(&mut self) -> bool {
        if self.settings != self.baseline {
            self.settings.sanitize();
            if let Ok(mut s) = self.shared.settings.lock() {
                *s = self.settings.clone();
            }
            self.baseline = self.settings.clone();
            self.save_due = Some(Instant::now() + SAVE_DELAY);
        }
        match self.save_due {
            Some(due) if Instant::now() >= due => {
                self.save_due = None;
                self.send(Cmd::SettingsChanged);
                false
            }
            Some(_) => true,
            None => false,
        }
    }

    /// Sends a pending settings change at once (window closing).
    pub fn flush(&mut self) {
        self.end_frame();
        if self.save_due.take().is_some() {
            self.send(Cmd::SettingsChanged);
        }
    }

    pub fn send(&self, cmd: Cmd) {
        if let Some(control) = &self.control {
            control.send(cmd);
        }
    }

    pub fn request(&self, request: UiRequest) {
        if let Ok(mut r) = self.shared.requests.lock() {
            r.push(request);
        }
    }

    pub fn toast(&mut self, text: impl Into<String>, error: bool) {
        self.toasts.push(Toast {
            text: text.into(),
            error,
            born: Instant::now(),
        });
    }

    pub fn now(&self) -> i64 {
        self.shared.now()
    }

    /// Remembers a value the user set so it shows until the engine reports it.
    pub fn hold(&mut self, what: Held, value: i32) {
        self.held.insert(what, (value, Instant::now()));
    }

    /// The value to show: the one just set, else the engine's.
    pub fn shown(&self, what: &Held, actual: i32) -> i32 {
        self.held.get(what).map(|(v, _)| *v).unwrap_or(actual)
    }

    pub fn holding(&self) -> bool {
        !self.held.is_empty()
    }

    // ------------------------------------------------------------------------------
    // Common actions
    // ------------------------------------------------------------------------------

    pub fn set_brightness(&mut self, key: Option<&str>, value: i32) {
        match key {
            Some(k) => self.hold(Held::Brightness(k.to_string()), value),
            None => {
                self.hold(Held::LinkAll, value);
                let linked: Vec<String> = self
                    .live
                    .monitors
                    .iter()
                    .filter(|m| self.settings.prefs(&m.monitor.key).linked)
                    .map(|m| m.monitor.key.clone())
                    .collect();
                for k in linked {
                    self.hold(Held::Brightness(k), value);
                }
            }
        }
        self.send(Cmd::Brightness {
            key: key.map(|k| k.to_string()),
            value,
        });
    }

    pub fn set_feature(&mut self, key: &str, feature: Feature, value: u32) {
        self.hold(Held::Feature(key.to_string(), feature), value as i32);
        self.send(Cmd::Feature {
            key: key.to_string(),
            feature,
            value,
        });
    }

    pub fn set_night(&mut self, on: bool) {
        self.hold(Held::Night, on as i32);
        self.send(Cmd::NightLight(on));
    }

    pub fn set_kelvin(&mut self, kelvin: u32) {
        self.hold(Held::Kelvin, kelvin as i32);
        self.send(Cmd::Kelvin(kelvin));
    }

    pub fn apply_preset(&mut self, name: &str) {
        if let Some(preset) = self.settings.preset(name).cloned() {
            let keys: Vec<String> = self
                .live
                .monitors
                .iter()
                .map(|m| m.monitor.key.clone())
                .collect();
            for k in keys {
                let v = preset
                    .per_monitor
                    .get(&k)
                    .copied()
                    .unwrap_or(preset.brightness);
                self.hold(Held::Brightness(k), v);
            }
            self.hold(Held::LinkAll, preset.brightness);
            match preset.kelvin {
                Some(k) => {
                    self.hold(Held::Night, 1);
                    self.hold(Held::Kelvin, k as i32);
                }
                None => self.hold(Held::Night, 0),
            }
        }
        self.send(Cmd::Preset(name.to_string()));
    }

    pub fn night_on(&self) -> bool {
        self.shown(&Held::Night, self.live.night_on as i32) != 0
    }

    pub fn kelvin(&self) -> u32 {
        let actual = if self.live.night_on {
            self.live.kelvin
        } else {
            self.settings.kelvin
        };
        self.shown(&Held::Kelvin, actual as i32).max(0) as u32
    }

    pub fn brightness(&self, m: &MonitorView) -> i32 {
        self.shown(&Held::Brightness(m.monitor.key.clone()), m.brightness)
    }

    /// The level of a feature as a percentage (continuous) or a raw code.
    pub fn feature(&self, m: &MonitorView, f: Feature) -> Option<i32> {
        let actual = m.level(f).map(|l| {
            if f.continuous() {
                l.percent() as i32
            } else {
                l.value as i32
            }
        });
        match self.held.get(&Held::Feature(m.monitor.key.clone(), f)) {
            Some((v, _)) => Some(*v),
            None => actual,
        }
    }

    /// What the "link all" slider shows: the average of the linked monitors.
    pub fn link_value(&self) -> i32 {
        let linked: Vec<i32> = self
            .live
            .monitors
            .iter()
            .filter(|m| self.settings.prefs(&m.monitor.key).linked)
            .map(|m| self.brightness(m))
            .collect();
        let actual = if linked.is_empty() {
            0
        } else {
            (linked.iter().sum::<i32>() as f32 / linked.len() as f32).round() as i32
        };
        self.shown(&Held::LinkAll, actual)
    }

    /// The name to show for a monitor: the rename being edited, else the engine's.
    pub fn monitor_name(&self, m: &MonitorView) -> String {
        let rename = self.settings.prefs(&m.monitor.key).rename;
        if !rename.trim().is_empty() {
            rename.trim().to_string()
        } else if !m.display_name.is_empty() {
            m.display_name.clone()
        } else {
            m.monitor.name.clone()
        }
    }

    pub fn tz(&self) -> &dyn crate::timeutil::Tz {
        self.shared.tz.as_ref()
    }
}
