//! Mock backends for the demo, the self-test and unit tests. Nothing here
//! touches real hardware.

use crate::model::{
    Backend, DisplayChange, DisplayConfig, DisplayMode, DisplayState, Feature, GammaRamp, HdrState,
    HwError, Level, Monitor, Orientation,
};
use std::collections::BTreeMap;
use std::sync::{Arc, Mutex};

/// What happened to a mock monitor, for assertions.
#[derive(Clone, Debug, Default)]
pub struct MockLog {
    /// Every write: (monitor key, feature, raw value).
    pub writes: Vec<(String, Feature, u32)>,
    /// The current gamma ramp per monitor (absent = identity).
    pub gamma: BTreeMap<String, GammaRamp>,
    /// Number of gamma writes.
    pub gamma_writes: usize,
    pub enumerations: usize,
}

/// An in-memory set of monitors. Clones share the same state and log.
#[derive(Clone)]
pub struct MockBackend {
    monitors: Arc<Mutex<Vec<Monitor>>>,
    values: Arc<Mutex<BTreeMap<(String, u8), Level>>>,
    pub log: Arc<Mutex<MockLog>>,
    /// Simulated time per request, milliseconds (0 in tests).
    pub delay_ms: u64,
}

impl MockBackend {
    pub fn new(monitors: Vec<Monitor>) -> Self {
        let mut values = BTreeMap::new();
        for m in &monitors {
            for f in &m.features {
                let level = match f {
                    Feature::Brightness => Level {
                        value: 60,
                        max: 100,
                    },
                    Feature::Contrast => Level {
                        value: 50,
                        max: 100,
                    },
                    Feature::Volume => Level {
                        value: 30,
                        max: 100,
                    },
                    Feature::InputSource => Level {
                        value: m
                            .caps
                            .as_ref()
                            .and_then(|c| c.values(0x60).first().copied())
                            .unwrap_or(0x0F) as u32,
                        max: 0,
                    },
                    Feature::PowerMode => Level { value: 1, max: 5 },
                    Feature::ColorPreset => Level {
                        value: 0x05,
                        max: 0,
                    },
                    Feature::RedGain | Feature::GreenGain | Feature::BlueGain => Level {
                        value: 50,
                        max: 100,
                    },
                };
                values.insert((m.key.clone(), f.vcp()), level);
            }
        }
        MockBackend {
            monitors: Arc::new(Mutex::new(monitors)),
            values: Arc::new(Mutex::new(values)),
            log: Arc::new(Mutex::new(MockLog::default())),
            delay_ms: 0,
        }
    }

    /// Sets a stored value directly (no write is logged).
    pub fn put(&self, key: &str, feature: Feature, value: u32, max: u32) {
        if let Ok(mut v) = self.values.lock() {
            v.insert((key.to_string(), feature.vcp()), Level { value, max });
        }
    }

    pub fn value(&self, key: &str, feature: Feature) -> Option<u32> {
        self.values
            .lock()
            .ok()?
            .get(&(key.to_string(), feature.vcp()))
            .map(|l| l.value)
    }

    pub fn log(&self) -> MockLog {
        self.log.lock().map(|l| l.clone()).unwrap_or_default()
    }

    /// Replaces the monitor list (simulates plugging a monitor in or out).
    pub fn set_monitors(&self, monitors: Vec<Monitor>) {
        if let Ok(mut m) = self.monitors.lock() {
            *m = monitors;
        }
    }

    fn pause(&self) {
        if self.delay_ms > 0 {
            std::thread::sleep(std::time::Duration::from_millis(self.delay_ms));
        }
    }

    fn monitor(&self, key: &str) -> Result<Monitor, HwError> {
        self.monitors
            .lock()
            .ok()
            .and_then(|m| m.iter().find(|m| m.key == key).cloned())
            .ok_or_else(|| HwError("This monitor is no longer connected.".into()))
    }
}

impl Backend for MockBackend {
    fn enumerate(&mut self) -> Vec<Monitor> {
        self.pause();
        if let Ok(mut log) = self.log.lock() {
            log.enumerations += 1;
        }
        self.monitors.lock().map(|m| m.clone()).unwrap_or_default()
    }

    fn get(&mut self, key: &str, feature: Feature) -> Result<Level, HwError> {
        self.read_vcp(key, feature.vcp())
    }

    fn set(&mut self, key: &str, feature: Feature, value: u32) -> Result<(), HwError> {
        self.pause();
        let m = self.monitor(key)?;
        if !m.supports(feature) {
            return Err(HwError(format!(
                "{} does not support {} over DDC/CI.",
                m.name,
                feature.label().to_lowercase()
            )));
        }
        if let Ok(mut v) = self.values.lock() {
            let e = v
                .entry((key.to_string(), feature.vcp()))
                .or_insert(Level { value: 0, max: 100 });
            e.value = if e.max > 0 { value.min(e.max) } else { value };
        }
        if let Ok(mut log) = self.log.lock() {
            log.writes.push((key.to_string(), feature, value));
        }
        Ok(())
    }

    fn read_vcp(&mut self, key: &str, code: u8) -> Result<Level, HwError> {
        self.pause();
        let m = self.monitor(key)?;
        self.values
            .lock()
            .ok()
            .and_then(|v| v.get(&(key.to_string(), code)).copied())
            .ok_or_else(|| HwError(format!("{} did not answer for code 0x{code:02X}.", m.name)))
    }

    fn set_gamma(&mut self, key: &str, ramp: Option<&GammaRamp>) -> Result<(), HwError> {
        self.monitor(key)?;
        if let Ok(mut log) = self.log.lock() {
            log.gamma_writes += 1;
            match ramp {
                Some(r) if !r.is_identity() => {
                    log.gamma.insert(key.to_string(), r.clone());
                }
                _ => {
                    log.gamma.remove(key);
                }
            }
        }
        Ok(())
    }

    fn min_interval_ms(&self, _key: &str) -> u64 {
        0
    }
}

/// In-memory display settings.
#[derive(Clone)]
pub struct MockDisplays {
    pub state: Arc<Mutex<Vec<DisplayState>>>,
    /// Every successful change, for assertions.
    pub applied: Arc<Mutex<Vec<(String, DisplayChange)>>>,
}

impl MockDisplays {
    pub fn new(displays: Vec<DisplayState>) -> Self {
        MockDisplays {
            state: Arc::new(Mutex::new(displays)),
            applied: Arc::new(Mutex::new(Vec::new())),
        }
    }

    /// A display state with a typical list of modes.
    pub fn display(
        device: &str,
        name: &str,
        native: (u32, u32),
        refresh: &[u32],
        x: i32,
        primary: bool,
        scale: u32,
    ) -> DisplayState {
        let mut modes = Vec::new();
        let sizes = [
            native,
            (native.0 * 3 / 4, native.1 * 3 / 4),
            (native.0 * 2 / 3, native.1 * 2 / 3),
            (native.0 / 2, native.1 / 2),
        ];
        for (w, h) in sizes {
            for &hz in refresh {
                modes.push(DisplayMode {
                    width: w,
                    height: h,
                    refresh: hz,
                    bits: 32,
                });
            }
        }
        modes.sort_by(|a, b| b.cmp(a));
        modes.dedup();
        DisplayState {
            device: device.into(),
            name: name.into(),
            current: DisplayMode {
                width: native.0,
                height: native.1,
                refresh: refresh[0],
                bits: 32,
            },
            modes,
            orientation: Orientation::Landscape,
            primary,
            rect: crate::model::PxRect::new(x, 0, native.0 as i32, native.1 as i32),
            scale_percent: scale,
            hdr: HdrState::default(),
        }
    }
}

impl DisplayConfig for MockDisplays {
    fn displays(&mut self) -> Vec<DisplayState> {
        self.state.lock().map(|s| s.clone()).unwrap_or_default()
    }

    fn apply(&mut self, device: &str, change: &DisplayChange) -> Result<DisplayChange, HwError> {
        let mut all = self
            .state
            .lock()
            .map_err(|_| HwError::from("display state unavailable"))?;
        let index = all
            .iter()
            .position(|d| d.device == device)
            .ok_or_else(|| HwError::from("That display is not connected."))?;
        let mut before = DisplayChange::default();
        {
            let d = &mut all[index];
            if let Some(mode) = change.mode {
                if !d.modes.contains(&mode) {
                    return Err(HwError::from("The display does not support that mode."));
                }
                before.mode = Some(d.current);
                d.current = mode;
            }
            if let Some(o) = change.orientation {
                before.orientation = Some(d.orientation);
                d.orientation = o;
            }
        }
        if change.make_primary {
            if let Some(old) = all.iter().position(|d| d.primary) {
                if old != index {
                    // Reverting "make primary" means making the old one primary
                    // again; the caller applies that change to the old device.
                    before.make_primary = true;
                    for d in all.iter_mut() {
                        d.primary = d.device == device;
                    }
                    drop(all);
                    if let Ok(mut a) = self.applied.lock() {
                        a.push((device.to_string(), change.clone()));
                    }
                    return Ok(before);
                }
            }
        }
        drop(all);
        if let Ok(mut a) = self.applied.lock() {
            a.push((device.to_string(), change.clone()));
        }
        Ok(before)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::{Connection, PxRect};

    fn monitor(key: &str) -> Monitor {
        Monitor {
            key: key.into(),
            number: 1,
            name: "Test".into(),
            device: "\\\\.\\DISPLAY1".into(),
            connection: Connection::DdcCi,
            rect: PxRect::new(0, 0, 1920, 1080),
            primary: true,
            features: vec![Feature::Brightness, Feature::Contrast],
            caps: None,
            edid: None,
            note: None,
        }
    }

    #[test]
    fn mock_reads_writes_and_gamma() {
        let mut b = MockBackend::new(vec![monitor("A")]);
        assert_eq!(b.get("A", Feature::Brightness).unwrap().value, 60);
        b.set("A", Feature::Brightness, 30).unwrap();
        assert_eq!(b.value("A", Feature::Brightness), Some(30));
        assert!(b.set("A", Feature::Volume, 3).is_err());
        assert!(b.get("B", Feature::Brightness).is_err());
        let mut r = GammaRamp::identity();
        r.r[255] = 1000;
        b.set_gamma("A", Some(&r)).unwrap();
        assert!(b.log().gamma.contains_key("A"));
        b.set_gamma("A", None).unwrap();
        assert!(b.log().gamma.is_empty());
        assert_eq!(b.log().writes.len(), 1);
    }

    #[test]
    fn mock_display_changes_and_revert() {
        let d = MockDisplays::display("D1", "One", (2560, 1440), &[144, 60], 0, true, 125);
        let mut m = MockDisplays::new(vec![d.clone()]);
        let change = DisplayChange {
            mode: Some(DisplayMode {
                width: 1920,
                height: 1080,
                refresh: 60,
                bits: 32,
            }),
            ..Default::default()
        };
        let before = m.apply("D1", &change).unwrap();
        assert_eq!(before.mode, Some(d.current));
        assert_eq!(m.displays()[0].current.width, 1920);
        m.apply("D1", &before).unwrap();
        assert_eq!(m.displays()[0].current, d.current);
        let bad = DisplayChange {
            mode: Some(DisplayMode {
                width: 1,
                height: 1,
                refresh: 1,
                bits: 32,
            }),
            ..Default::default()
        };
        assert!(m.apply("D1", &bad).is_err());
    }
}
