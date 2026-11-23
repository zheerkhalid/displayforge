//! The engine's decisions, without any Windows plumbing: what brightness,
//! colour temperature and dimming each monitor should have, given the
//! user's settings, manual changes, the schedule, the app in front and idle
//! time. Driven by `service.rs` (real PC) or directly (demo, tests, self-test).

use crate::color;
use crate::control::Controller;
use crate::engine::{Cmd, LiveState, MonitorView, ScheduleStatus, Shared};
use crate::model::{
    calibrate, uncalibrate, Backend, Connection, DisplayConfig, DisplayState, Feature, Monitor,
    MonitorKey,
};
use crate::profiles::{self, Foreground, Layers};
use crate::schedule;
use crate::settings::{AppProfile, DimMethod, Settings, KELVIN_NEUTRAL, MIN_BELOW_ZERO};
use crate::timeutil;
use std::collections::{BTreeMap, BTreeSet};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

/// What the outside world looks like at a tick.
#[derive(Clone, Debug, Default)]
pub struct Inputs {
    pub now: i64,
    /// The app in front, when known.
    pub front: Option<Foreground>,
    /// Seconds since the last keyboard or mouse input.
    pub idle_secs: u64,
}

/// What was last sent to a monitor, so that nothing is sent twice.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
struct Applied {
    hw: Option<u32>,
    /// (kelvin, factor in thousandths) of the gamma ramp; `None` = identity.
    ramp: Option<(u32, u32)>,
    alpha: u8,
}

/// Brightness factor for a screen without hardware control: the whole
/// scale is done in software, 100 = untouched, the bottom = a quarter.
pub fn software_factor(value: i32) -> f32 {
    let v = value.clamp(MIN_BELOW_ZERO, 100) as f32;
    let span = (100 - MIN_BELOW_ZERO) as f32;
    0.25 + 0.75 * (v - MIN_BELOW_ZERO as f32) / span
}

pub struct Core {
    pub shared: Arc<Shared>,
    pub controller: Arc<Controller>,
    displays: Box<dyn DisplayConfig>,
    pub settings: Settings,
    /// The user's brightness per monitor (the base layer).
    pub base: BTreeMap<MonitorKey, i32>,
    /// The schedule is in control (false after a manual change, until the next point).
    pub following: bool,
    last_point: Option<(usize, i64)>,
    /// Night-light temperature asked for by the schedule while following.
    sched_kelvin: Option<u32>,
    sched_status: ScheduleStatus,
    pub profile: Option<(usize, AppProfile)>,
    profile_monitor: Option<MonitorKey>,
    saved_contrast: BTreeMap<MonitorKey, u32>,
    pub idle: bool,
    front: Option<Foreground>,
    applied: BTreeMap<MonitorKey, Applied>,
    gamma_set: BTreeSet<MonitorKey>,
    /// Overlay opacity wanted per monitor (the service creates the windows).
    pub overlays: BTreeMap<MonitorKey, u8>,
    display_list: Vec<DisplayState>,
    /// The display that was primary before the last "make primary".
    primary_before: Option<String>,
    pub running_apps: Option<Vec<String>>,
    last_preset: Option<String>,
    pending_notice: Option<String>,
    /// Set by the controller when its snapshot changed.
    pub changed: Arc<AtomicBool>,
    monitor_count: usize,
}

impl Core {
    /// Starts the controller on `backend`. `wake` is called (from the
    /// controller's thread) whenever monitor state changed.
    pub fn new(
        shared: Arc<Shared>,
        backend: Box<dyn Backend>,
        mut displays: Box<dyn DisplayConfig>,
        wake: Box<dyn Fn() + Send>,
    ) -> Core {
        let changed = Arc::new(AtomicBool::new(true));
        let flag = changed.clone();
        let controller = Controller::start(
            backend,
            Box::new(move || {
                flag.store(true, Ordering::SeqCst);
                wake();
            }),
        );
        let settings = shared.settings();
        let display_list = displays.displays();
        let last_preset = shared.live().last_preset;
        Core {
            shared,
            controller: Arc::new(controller),
            displays,
            settings,
            base: BTreeMap::new(),
            following: true,
            last_point: None,
            sched_kelvin: None,
            sched_status: ScheduleStatus::default(),
            profile: None,
            profile_monitor: None,
            saved_contrast: BTreeMap::new(),
            idle: false,
            front: None,
            applied: BTreeMap::new(),
            gamma_set: BTreeSet::new(),
            overlays: BTreeMap::new(),
            display_list,
            primary_before: None,
            running_apps: None,
            last_preset,
            pending_notice: None,
            changed,
            monitor_count: 0,
        }
    }

    fn fade_ms(&self) -> u32 {
        if self.settings.fade {
            self.settings.fade_ms
        } else {
            0
        }
    }

    fn monitors(&self) -> Vec<Monitor> {
        self.controller
            .snapshot()
            .into_iter()
            .map(|s| s.monitor)
            .collect()
    }

    /// Picks up monitors the controller found: their current brightness
    /// becomes the base, so starting DisplayForge changes nothing.
    pub fn sync_monitors(&mut self) {
        if !self.controller.ready() {
            return;
        }
        let snaps = self.controller.snapshot();
        // A value the window already shows (the demo's prepared state) wins
        // for screens without hardware control, whose level cannot be read.
        let shown: BTreeMap<MonitorKey, i32> = self
            .shared
            .live()
            .monitors
            .into_iter()
            .map(|m| (m.monitor.key, m.brightness))
            .collect();
        if snaps.len() != self.monitor_count {
            self.monitor_count = snaps.len();
            // Gamma ramps are reset by Windows when displays change.
            self.applied
                .retain(|k, _| snaps.iter().any(|s| &s.monitor.key == k));
        }
        for s in snaps {
            let key = s.monitor.key.clone();
            if self.base.contains_key(&key) {
                continue;
            }
            let prefs = self.settings.prefs(&key);
            let level = s.levels.get(&Feature::Brightness).copied();
            let base = match (s.monitor.supports(Feature::Brightness), level) {
                (true, Some(l)) => uncalibrate(l.percent(), prefs.min, prefs.max) as i32,
                _ => shown
                    .get(&key)
                    .copied()
                    .unwrap_or(100)
                    .clamp(self.settings.min_brightness(), 100),
            };
            self.base.insert(key.clone(), base);
            // Treat the current value as already sent: starting writes nothing.
            let hw = level.map(|_| calibrate(base.max(0) as u32, prefs.min, prefs.max));
            self.applied.insert(
                key,
                Applied {
                    hw,
                    ramp: None,
                    alpha: 0,
                },
            );
        }
    }

    /// Forgets what was sent, so the next apply re-sends everything
    /// (after a display change or resume from sleep).
    pub fn invalidate(&mut self) {
        for a in self.applied.values_mut() {
            a.ramp = Some((0, 0));
        }
    }

    fn night(&self) -> (bool, u32) {
        if self.settings.schedule.enabled && self.following {
            if let Some(k) = self.sched_kelvin {
                return (k < KELVIN_NEUTRAL, k);
            }
        }
        (self.settings.night_light, self.settings.kelvin)
    }

    fn profile_applies(&self, key: &str) -> Option<&AppProfile> {
        let (_, p) = self.profile.as_ref()?;
        if p.only_its_monitor && self.profile_monitor.as_deref() != Some(key) {
            return None;
        }
        Some(p)
    }

    /// The brightness and colour temperature a monitor should have now.
    pub fn target(&self, key: &str) -> (i32, u32) {
        let base = self.base.get(key).copied().unwrap_or(100);
        let profile = self.profile_applies(key);
        let idle = self.idle.then_some(self.settings.idle.brightness);
        let value = profiles::effective_brightness(&Layers {
            base,
            profile: profile.and_then(|p| p.brightness),
            idle,
        })
        .clamp(self.settings.min_brightness(), 100);
        let (night_on, kelvin) = self.night();
        let kelvin = profiles::effective_kelvin(night_on, kelvin, profile.and_then(|p| p.kelvin));
        (value, kelvin)
    }

    /// Sends whatever differs from what was last sent.
    pub fn apply(&mut self, fade_ms: u32) {
        for m in self.monitors() {
            let key = m.key.clone();
            let prefs = self.settings.prefs(&key);
            let (value, kelvin) = self.target(&key);
            let (hw, factor) = if m.supports(Feature::Brightness) {
                let (hw, f) = color::split_brightness(value);
                (Some(calibrate(hw, prefs.min, prefs.max)), f)
            } else {
                (None, software_factor(value))
            };
            let (ramp_factor, alpha) = match prefs.dim_method {
                DimMethod::Gamma => (factor, 0),
                DimMethod::Overlay => (1.0, color::overlay_alpha(factor)),
            };
            let ramp_id = if kelvin >= KELVIN_NEUTRAL && ramp_factor >= 0.999 {
                None
            } else {
                Some((kelvin, (ramp_factor * 1000.0).round() as u32))
            };
            let last = self.applied.get(&key).copied().unwrap_or_default();
            if let Some(hw) = hw {
                if last.hw != Some(hw) {
                    self.controller
                        .set_percent(&key, Feature::Brightness, hw, fade_ms);
                }
            }
            if last.ramp != ramp_id {
                match ramp_id {
                    None => {
                        if self.gamma_set.remove(&key) {
                            self.controller.set_gamma(&key, None);
                        }
                    }
                    Some((k, f)) => {
                        let ramp = color::ramp_safe(k, f as f32 / 1000.0);
                        self.controller.set_gamma(&key, Some(ramp));
                        self.gamma_set.insert(key.clone());
                    }
                }
            }
            if alpha > 0 {
                self.overlays.insert(key.clone(), alpha);
            } else {
                self.overlays.remove(&key);
            }
            self.applied.insert(
                key,
                Applied {
                    hw: hw.or(last.hw),
                    ramp: ramp_id,
                    alpha,
                },
            );
        }
    }

    fn linked_keys(&self) -> Vec<MonitorKey> {
        self.monitors()
            .into_iter()
            .filter(|m| self.settings.prefs(&m.key).linked)
            .map(|m| m.key)
            .collect()
    }

    fn manual_change(&mut self) {
        if self.settings.schedule.enabled {
            self.following = false;
        }
    }

    fn save_settings(&mut self) {
        if let Ok(mut s) = self.shared.settings.lock() {
            s.night_light = self.settings.night_light;
            s.kelvin = self.settings.kelvin;
        }
        if !self.shared.demo {
            if let Err(e) = self.shared.settings().save(&self.shared.dir) {
                self.pending_notice = Some(format!("The settings could not be saved: {e}"));
            }
        }
    }

    pub fn apply_preset(&mut self, name: &str) -> bool {
        let Some(p) = self.settings.preset(name).cloned() else {
            self.pending_notice = Some(format!("There is no preset called \"{name}\"."));
            return false;
        };
        let min = self.settings.min_brightness();
        for m in self.monitors() {
            let v = p
                .per_monitor
                .get(&m.key)
                .copied()
                .unwrap_or(p.brightness)
                .clamp(min, 100);
            self.base.insert(m.key.clone(), v);
            if let Some(c) = p.contrast {
                if m.supports(Feature::Contrast) {
                    self.controller.set_percent(
                        &m.key,
                        Feature::Contrast,
                        c.min(100),
                        self.fade_ms(),
                    );
                }
            }
        }
        match p.kelvin {
            Some(k) => {
                self.settings.night_light = k < KELVIN_NEUTRAL;
                self.settings.kelvin = k.clamp(crate::settings::KELVIN_MIN, KELVIN_NEUTRAL);
            }
            None => self.settings.night_light = false,
        }
        self.last_preset = Some(p.name.clone());
        self.manual_change();
        self.save_settings();
        self.apply(self.fade_ms().max(600));
        true
    }

    /// Handles one command. Returns false for `Quit`.
    pub fn handle(&mut self, cmd: Cmd) -> bool {
        self.sync_monitors();
        let min = self.settings.min_brightness();
        match cmd {
            Cmd::Brightness { key, value } => {
                let value = value.clamp(min, 100);
                let keys = match key {
                    Some(k) => vec![k],
                    None => self.linked_keys(),
                };
                for k in keys {
                    self.base.insert(k, value);
                }
                self.manual_change();
                self.apply(self.fade_ms());
            }
            Cmd::Step(delta) => {
                for k in self.linked_keys() {
                    let v = self.base.get(&k).copied().unwrap_or(100);
                    self.base.insert(k, (v + delta).clamp(min, 100));
                }
                self.manual_change();
                self.apply(self.fade_ms().min(250));
            }
            Cmd::Feature {
                key,
                feature,
                value,
            } => {
                if feature == Feature::Brightness {
                    return self.handle(Cmd::Brightness {
                        key: Some(key),
                        value: value as i32,
                    });
                }
                if feature.continuous() {
                    self.controller
                        .set_percent(&key, feature, value.min(100), 0);
                } else {
                    self.controller.set_raw(&key, feature, value);
                }
            }
            Cmd::Preset(name) => {
                self.apply_preset(&name);
            }
            Cmd::NightLight(on) => {
                self.settings.night_light = on;
                self.manual_change();
                self.save_settings();
                self.apply(0);
            }
            Cmd::Kelvin(k) => {
                self.settings.kelvin = k.clamp(crate::settings::KELVIN_MIN, KELVIN_NEUTRAL);
                self.settings.night_light = true;
                self.manual_change();
                self.save_settings();
                self.apply(0);
            }
            Cmd::SettingsChanged => {
                let mut s = self.shared.settings();
                s.sanitize();
                self.settings = s;
                if !self.shared.demo {
                    if let Err(e) = self.settings.save(&self.shared.dir) {
                        self.pending_notice = Some(format!("The settings could not be saved: {e}"));
                    }
                }
                let min = self.settings.min_brightness();
                for v in self.base.values_mut() {
                    *v = (*v).clamp(min, 100);
                }
                // Calibration or dim method may have changed: send everything again.
                for a in self.applied.values_mut() {
                    a.hw = None;
                    a.ramp = Some((0, 0));
                }
                self.apply(0);
            }
            Cmd::Rescan => {
                self.controller.rescan();
                self.display_list = self.displays.displays();
            }
            Cmd::ApplyDisplay { device, change } => {
                let old_primary = self
                    .display_list
                    .iter()
                    .find(|d| d.primary)
                    .map(|d| d.device.clone());
                // "Make primary" sent for the display that already is primary is
                // the revert of an earlier change: the old primary gets it back.
                let target = match (&old_primary, &self.primary_before) {
                    (Some(p), Some(old))
                        if change.make_primary && *p == device && *old != device =>
                    {
                        old.clone()
                    }
                    _ => device.clone(),
                };
                let result = self
                    .displays
                    .apply(&target, &change)
                    .map_err(|e| e.to_string());
                if change.make_primary && result.is_ok() {
                    self.primary_before = if target == device { old_primary } else { None };
                }
                self.display_list = self.displays.displays();
                if let Ok(mut live) = self.shared.live.lock() {
                    live.display_result = Some((device, result));
                }
                self.controller.rescan();
            }
            Cmd::ReadAllVcp(key) => {
                let controller = self.controller.clone();
                let shared = self.shared.clone();
                std::thread::spawn(move || {
                    let values = controller.read_all(&key);
                    if let Ok(mut live) = shared.live.lock() {
                        live.vcp_dump = Some((key, values));
                    }
                    shared.wake();
                });
            }
            Cmd::ResumeSchedule => {
                self.following = true;
                self.last_point = None;
            }
            Cmd::ScreensOff => {
                // Done by the service on the real PC; nothing to simulate.
            }
            Cmd::Quit => return false,
        }
        self.publish();
        true
    }

    fn foreground_monitor(&self, front: &crate::foreground::Front) -> Option<MonitorKey> {
        let cx = front.rect.x + front.rect.w / 2;
        let cy = front.rect.y + front.rect.h / 2;
        self.monitors()
            .into_iter()
            .find(|m| m.rect.contains(cx, cy))
            .map(|m| m.key)
    }

    /// Converts a foreground window into the profile matcher's input.
    pub fn front_from(&self, front: Option<crate::foreground::Front>) -> Option<Foreground> {
        front.map(|f| Foreground {
            monitor: self.foreground_monitor(&f),
            exe: f.exe,
            fullscreen: f.fullscreen,
        })
    }

    /// Runs the schedule, profiles and idle dimming for this moment.
    pub fn tick(&mut self, inputs: &Inputs) {
        self.sync_monitors();
        let mut fade = 1000;
        let tz = self.shared.tz.clone();

        // The schedule.
        let s = &self.settings;
        let resolved = if s.schedule.enabled {
            schedule::resolve(
                &s.schedule,
                &s.presets,
                s.location.as_ref(),
                inputs.now,
                tz.as_ref(),
            )
        } else {
            None
        };
        match &resolved {
            Some(r) => {
                let point = r.point.map(|p| (p, r.since));
                if point != self.last_point {
                    // A new point began: the schedule takes over again.
                    self.following = true;
                    self.last_point = point;
                }
                if self.following {
                    if let Some(b) = r.brightness {
                        let b = b.clamp(self.settings.min_brightness(), 100);
                        for k in self.linked_keys() {
                            self.base.insert(k, b);
                        }
                    }
                    self.sched_kelvin = r.kelvin;
                }
            }
            None => self.sched_kelvin = None,
        }
        self.sched_status = self.schedule_status(resolved.as_ref(), inputs.now);

        // Per-app profiles.
        self.front = inputs.front.clone();
        let picked = self
            .front
            .as_ref()
            .and_then(|f| profiles::pick(&self.settings.profiles, f))
            .map(|(i, p)| (i, p.clone()));
        if picked != self.profile {
            self.switch_profile(picked);
            fade = 500;
        }

        // Idle dimming.
        let idle_cfg = &self.settings.idle;
        let fullscreen = self.front.as_ref().map(|f| f.fullscreen).unwrap_or(false);
        let want_idle = idle_cfg.enabled
            && inputs.idle_secs >= idle_cfg.after_minutes as u64 * 60
            && !(idle_cfg.skip_fullscreen && fullscreen);
        if want_idle != self.idle {
            self.idle = want_idle;
            fade = if want_idle { 2500 } else { 300 };
        }

        self.apply(fade);
        self.publish();
    }

    fn switch_profile(&mut self, picked: Option<(usize, AppProfile)>) {
        // Restore the contrast the previous profile changed.
        for (key, c) in std::mem::take(&mut self.saved_contrast) {
            self.controller.set_percent(&key, Feature::Contrast, c, 300);
        }
        self.profile_monitor = self.front.as_ref().and_then(|f| f.monitor.clone());
        self.profile = picked;
        if let Some((_, p)) = &self.profile {
            if let Some(c) = p.contrast {
                for s in self.controller.snapshot() {
                    let key = s.monitor.key.clone();
                    if !s.monitor.supports(Feature::Contrast) {
                        continue;
                    }
                    if p.only_its_monitor && self.profile_monitor.as_deref() != Some(&key) {
                        continue;
                    }
                    if let Some(l) = s.levels.get(&Feature::Contrast) {
                        self.saved_contrast.insert(key.clone(), l.percent());
                    }
                    self.controller
                        .set_percent(&key, Feature::Contrast, c.min(100), 300);
                }
            }
        }
    }

    fn schedule_status(&self, r: Option<&schedule::Resolved>, now: i64) -> ScheduleStatus {
        let s = &self.settings;
        let tz = self.shared.tz.clone();
        let mut status = ScheduleStatus {
            enabled: s.schedule.enabled,
            following: self.following,
            ..Default::default()
        };
        if let Some(loc) = &s.location {
            let day = timeutil::local_day(now, tz.as_ref());
            let sun = crate::solar::sun_times(day, loc.lat, loc.lon, tz.as_ref());
            status.sunrise = sun.sunrise;
            status.sunset = sun.sunset;
        }
        if let Some(r) = r {
            if let Some(p) = r.point.and_then(|i| s.schedule.points.get(i)) {
                status.current = Some(format!(
                    "Since {} ({})",
                    timeutil::fmt_hm(r.since, tz.as_ref()),
                    schedule::when_label(&p.when)
                ));
            }
            if let Some((i, t)) = r.next {
                if let Some(p) = s.schedule.points.get(i) {
                    status.next = Some((t, schedule::when_label(&p.when)));
                }
            }
        }
        status
    }

    /// The overall brightness shown in the tray tooltip: the first linked monitor's.
    pub fn headline_brightness(&self) -> Option<i32> {
        self.linked_keys()
            .first()
            .and_then(|k| self.base.get(k).copied())
    }

    /// Writes the engine's part of the live state and wakes the window when it changed.
    pub fn publish(&mut self) {
        let snaps = self.controller.snapshot();
        let mut monitors: Vec<MonitorView> = snaps
            .into_iter()
            .map(|s| {
                let prefs = self.settings.prefs(&s.monitor.key);
                let brightness = self.base.get(&s.monitor.key).copied().unwrap_or(100);
                MonitorView {
                    display_name: if prefs.rename.trim().is_empty() {
                        s.monitor.name.clone()
                    } else {
                        prefs.rename.trim().to_string()
                    },
                    levels: s.levels,
                    brightness,
                    fading: s.fading,
                    error: s.error,
                    monitor: s.monitor,
                }
            })
            .collect();
        monitors.sort_by_key(|m| m.monitor.number);
        let (night_on, kelvin) = self.night();
        let profile_kelvin = self.profile.as_ref().and_then(|(_, p)| p.kelvin);
        let kelvin_now = profiles::effective_kelvin(night_on, kelvin, profile_kelvin);
        let ready = self.controller.ready();
        let changed = self.changed.swap(false, Ordering::SeqCst);
        let notice = self.pending_notice.take();
        let mut dirty = changed || notice.is_some();
        if let Ok(mut live) = self.shared.live.lock() {
            let before: LiveState = live.clone();
            live.ready = ready;
            live.monitors = monitors;
            live.displays = self.display_list.clone();
            live.night_on = kelvin_now < KELVIN_NEUTRAL;
            live.kelvin = kelvin_now;
            live.active_profile = self.profile.as_ref().map(|(_, p)| p.exe.clone());
            live.schedule = self.sched_status.clone();
            live.idle_dimmed = self.idle;
            if let Some(apps) = self.running_apps.take() {
                live.running_apps = apps;
            }
            live.last_preset = self.last_preset.clone();
            if notice.is_some() {
                live.notice = notice;
            }
            dirty |= *live != before;
        }
        if dirty {
            self.shared.wake();
        }
    }

    /// Stops the controller, which restores every gamma ramp it changed.
    pub fn shutdown(self) {
        let Core { controller, .. } = self;
        match Arc::try_unwrap(controller) {
            Ok(c) => c.stop(),
            Err(arc) => {
                // A "read all" thread still holds it; it ends soon.
                for _ in 0..50 {
                    std::thread::sleep(std::time::Duration::from_millis(100));
                    if Arc::strong_count(&arc) == 1 {
                        break;
                    }
                }
                if let Ok(c) = Arc::try_unwrap(arc) {
                    c.stop();
                }
            }
        }
    }

    /// Monitors and what the engine would show for each (used by `--list`).
    pub fn connection_of(&self, key: &str) -> Option<Connection> {
        self.monitors()
            .into_iter()
            .find(|m| m.key == key)
            .map(|m| m.connection)
    }
}

#[cfg(test)]
#[allow(clippy::field_reassign_with_default)]
mod tests {
    use super::*;
    use crate::demo;
    use crate::mock::{MockBackend, MockDisplays};
    use crate::settings::{Schedule, SchedulePoint, Trigger, When};
    use std::time::Duration;

    fn core_with(settings: Settings) -> (Core, MockBackend) {
        let mut shared = Shared::new(
            std::env::temp_dir().join("displayforge-policy-test"),
            settings,
            demo::tz(),
            true,
        );
        shared.fixed_now = Some(demo::now());
        let backend = demo::backend();
        let core = Core::new(
            Arc::new(shared),
            Box::new(backend.clone()),
            Box::new(MockDisplays::new(demo::displays())),
            Box::new(|| {}),
        );
        let start = std::time::Instant::now();
        while !core.controller.ready() && start.elapsed() < Duration::from_secs(5) {
            std::thread::sleep(Duration::from_millis(10));
        }
        (core, backend)
    }

    fn settle(core: &Core) {
        assert!(core.controller.wait_idle(Duration::from_secs(5)));
    }

    #[test]
    fn starting_changes_nothing() {
        let (mut core, backend) = core_with(Settings::default());
        core.tick(&Inputs {
            now: demo::now(),
            ..Default::default()
        });
        settle(&core);
        assert!(
            backend.log().writes.is_empty(),
            "{:?}",
            backend.log().writes
        );
        assert!(backend.log().gamma.is_empty());
        assert_eq!(core.base[demo::KEY_MAIN], 55);
        core.shutdown();
    }

    #[test]
    fn manual_brightness_and_calibration() {
        let mut s = Settings::default();
        s.prefs_mut(demo::KEY_SIDE).min = 10;
        s.prefs_mut(demo::KEY_SIDE).max = 90;
        let (mut core, backend) = core_with(s);
        core.handle(Cmd::Brightness {
            key: None,
            value: 100,
        });
        settle(&core);
        assert_eq!(
            backend.value(demo::KEY_MAIN, Feature::Brightness),
            Some(100)
        );
        assert_eq!(backend.value(demo::KEY_SIDE, Feature::Brightness), Some(90));
        core.handle(Cmd::Brightness {
            key: Some(demo::KEY_LAPTOP.into()),
            value: 20,
        });
        settle(&core);
        assert_eq!(
            backend.value(demo::KEY_LAPTOP, Feature::Brightness),
            Some(20)
        );
        assert_eq!(
            backend.value(demo::KEY_MAIN, Feature::Brightness),
            Some(100)
        );
        core.shutdown();
    }

    #[test]
    fn below_zero_uses_gamma_and_is_restored() {
        let s = Settings {
            below_zero: true,
            ..Settings::default()
        };
        let (mut core, backend) = core_with(s);
        core.handle(Cmd::Brightness {
            key: Some(demo::KEY_MAIN.into()),
            value: -30,
        });
        settle(&core);
        assert_eq!(backend.value(demo::KEY_MAIN, Feature::Brightness), Some(0));
        assert!(backend.log().gamma.contains_key(demo::KEY_MAIN));
        core.handle(Cmd::Brightness {
            key: Some(demo::KEY_MAIN.into()),
            value: 40,
        });
        settle(&core);
        assert!(!backend.log().gamma.contains_key(demo::KEY_MAIN));
        // Night light, then shutting down restores every ramp.
        core.handle(Cmd::NightLight(true));
        settle(&core);
        assert!(backend.log().gamma.len() >= 3);
        core.shutdown();
        assert!(backend.log().gamma.is_empty());
    }

    #[test]
    fn presets_set_brightness_contrast_and_night_light() {
        let (mut core, backend) = core_with(Settings::default());
        assert!(core.apply_preset("night"));
        settle(&core);
        assert_eq!(backend.value(demo::KEY_MAIN, Feature::Brightness), Some(25));
        assert!(core.settings.night_light);
        assert_eq!(core.settings.kelvin, 3200);
        assert!(!core.apply_preset("nope"));
        core.shutdown();
    }

    #[test]
    fn profile_overrides_and_restores() {
        let mut s = Settings::default();
        s.profiles = vec![AppProfile {
            exe: "vlc.exe".into(),
            trigger: Trigger::Fullscreen,
            brightness: Some(100),
            contrast: Some(80),
            kelvin: Some(KELVIN_NEUTRAL),
            ..AppProfile::default()
        }];
        let (mut core, backend) = core_with(s);
        let vlc = |fullscreen| Inputs {
            now: demo::now(),
            front: Some(Foreground {
                exe: "vlc.exe".into(),
                fullscreen,
                monitor: Some(demo::KEY_MAIN.into()),
            }),
            idle_secs: 0,
        };
        core.tick(&vlc(false));
        settle(&core);
        assert!(core.profile.is_none());
        core.tick(&vlc(true));
        settle(&core);
        assert_eq!(
            backend.value(demo::KEY_MAIN, Feature::Brightness),
            Some(100)
        );
        assert_eq!(backend.value(demo::KEY_MAIN, Feature::Contrast), Some(80));
        core.tick(&Inputs {
            now: demo::now(),
            ..Default::default()
        });
        settle(&core);
        assert_eq!(backend.value(demo::KEY_MAIN, Feature::Brightness), Some(55));
        assert_eq!(backend.value(demo::KEY_MAIN, Feature::Contrast), Some(70));
        core.shutdown();
    }

    #[test]
    fn idle_dims_and_wakes() {
        let mut s = Settings::default();
        s.idle.enabled = true;
        s.idle.after_minutes = 2;
        s.idle.brightness = 10;
        let (mut core, backend) = core_with(s);
        core.tick(&Inputs {
            now: demo::now(),
            front: None,
            idle_secs: 200,
        });
        settle(&core);
        assert_eq!(backend.value(demo::KEY_MAIN, Feature::Brightness), Some(10));
        core.tick(&Inputs {
            now: demo::now(),
            front: None,
            idle_secs: 1,
        });
        settle(&core);
        assert_eq!(backend.value(demo::KEY_MAIN, Feature::Brightness), Some(55));
        core.shutdown();
    }

    #[test]
    fn schedule_follows_until_manual_change() {
        let mut s = Settings::default();
        s.schedule = Schedule {
            enabled: true,
            points: vec![
                SchedulePoint {
                    when: When::Time { minutes: 8 * 60 },
                    brightness: Some(90),
                    kelvin: Some(KELVIN_NEUTRAL),
                    preset: None,
                },
                SchedulePoint {
                    when: When::Time { minutes: 19 * 60 },
                    brightness: Some(40),
                    kelvin: Some(4000),
                    preset: None,
                },
            ],
            transition_minutes: 0,
        };
        let (mut core, backend) = core_with(s);
        let at = |h: i64, m: i64| Inputs {
            now: demo::now() - (19 * 60 + 40) * 60 + (h * 60 + m) * 60,
            ..Default::default()
        };
        core.tick(&at(19, 40));
        settle(&core);
        assert_eq!(backend.value(demo::KEY_MAIN, Feature::Brightness), Some(40));
        assert!(backend.log().gamma.contains_key(demo::KEY_MAIN));
        core.handle(Cmd::Brightness {
            key: None,
            value: 70,
        });
        core.tick(&at(19, 45));
        settle(&core);
        assert!(!core.following);
        assert_eq!(backend.value(demo::KEY_MAIN, Feature::Brightness), Some(70));
        core.handle(Cmd::ResumeSchedule);
        core.tick(&at(19, 50));
        settle(&core);
        assert_eq!(backend.value(demo::KEY_MAIN, Feature::Brightness), Some(40));
        core.shutdown();
    }

    #[test]
    fn display_change_reports_what_to_revert_to() {
        let (mut core, _) = core_with(Settings::default());
        let d = demo::displays()[1].clone();
        let mode = d.modes.iter().find(|m| m.width < d.current.width).copied();
        core.handle(Cmd::ApplyDisplay {
            device: d.device.clone(),
            change: crate::model::DisplayChange {
                mode,
                ..Default::default()
            },
        });
        let result = core.shared.live().display_result.unwrap();
        assert_eq!(result.1.unwrap().mode, Some(d.current));
        core.shutdown();
    }

    #[test]
    fn making_primary_is_reverted_to_the_old_primary() {
        let (mut core, _) = core_with(Settings::default());
        let laptop = demo::displays()[0].device.clone();
        let main = demo::displays()[1].device.clone();
        let make = crate::model::DisplayChange {
            make_primary: true,
            ..Default::default()
        };
        core.handle(Cmd::ApplyDisplay {
            device: laptop.clone(),
            change: make.clone(),
        });
        let (device, before) = core.shared.live().display_result.unwrap();
        assert_eq!(device, laptop);
        let before = before.unwrap();
        assert!(before.make_primary);
        let primary = |core: &Core| {
            core.shared
                .live()
                .displays
                .iter()
                .find(|d| d.primary)
                .map(|d| d.device.clone())
        };
        core.publish();
        assert_eq!(primary(&core), Some(laptop.clone()));
        // The window sends the "before" change for the same device.
        core.handle(Cmd::ApplyDisplay {
            device: laptop,
            change: before,
        });
        core.publish();
        assert_eq!(primary(&core), Some(main));
        core.shutdown();
    }

    #[test]
    fn software_only_factor_range() {
        assert!((software_factor(100) - 1.0).abs() < 1e-6);
        assert!((software_factor(MIN_BELOW_ZERO) - 0.25).abs() < 1e-6);
        assert!(software_factor(50) < software_factor(60));
    }
}
