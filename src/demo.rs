//! Invented monitors, displays and settings for `--demo` and screenshots.
//! Every name, number and model here is made up.

use crate::engine::{LiveState, MonitorView, ScheduleStatus, Shared};
use crate::mock::{MockBackend, MockDisplays};
use crate::model::{
    Capabilities, Connection, DisplayState, EdidInfo, Feature, HdrState, Level, Monitor, PxRect,
};
use crate::settings::{AppProfile, Location, Settings, Trigger, KELVIN_NEUTRAL};
use crate::timeutil::{days_from_civil, FixedTz, Tz};
use std::collections::BTreeMap;
use std::path::PathBuf;
use std::sync::Arc;

/// The demo's clock: Tuesday 24 November 2026, 19:40 local time (UTC+3).
pub const OFFSET: i32 = 3 * 3600;

pub fn now() -> i64 {
    days_from_civil(2026, 11, 24) * 86_400 + (19 * 60 + 40) * 60 - OFFSET as i64
}

pub fn tz() -> Arc<dyn Tz> {
    Arc::new(FixedTz(OFFSET))
}

pub const KEY_LAPTOP: &str = "demo-laptop";
pub const KEY_MAIN: &str = "demo-27q";
pub const KEY_SIDE: &str = "demo-24p";
pub const KEY_TV: &str = "demo-tv";

fn caps(model: &str, vcp: &[(u8, &[u16])]) -> Capabilities {
    Capabilities {
        protocol: Some("monitor".into()),
        kind: Some("LCD".into()),
        model: Some(model.into()),
        mccs_version: Some("2.2".into()),
        vcp: vcp.iter().map(|(c, v)| (*c, v.to_vec())).collect(),
    }
}

#[allow(clippy::too_many_arguments)]
fn edid(
    id: &str,
    maker: &str,
    product: u16,
    name: &str,
    year: u16,
    size: (u8, u8),
    native: (u32, u32, f32),
    bits: u8,
) -> EdidInfo {
    EdidInfo {
        manufacturer_id: id.into(),
        manufacturer: maker.into(),
        product_code: product,
        name: Some(name.into()),
        serial: Some("DEMO-0000-0000".into()),
        week: Some(14),
        year: Some(year),
        width_cm: size.0,
        height_cm: size.1,
        version: (1, 4),
        digital: true,
        bit_depth: Some(bits),
        native: Some(native),
    }
}

/// Four invented monitors: a laptop panel, two desk monitors over DDC/CI
/// and a TV that does not answer DDC/CI.
pub fn monitors() -> Vec<Monitor> {
    let ddc = [
        Feature::Brightness,
        Feature::Contrast,
        Feature::Volume,
        Feature::InputSource,
        Feature::PowerMode,
        Feature::ColorPreset,
        Feature::RedGain,
        Feature::GreenGain,
        Feature::BlueGain,
    ];
    vec![
        Monitor {
            key: KEY_LAPTOP.into(),
            number: 1,
            name: "Built-in display".into(),
            device: "\\\\.\\DISPLAY1".into(),
            connection: Connection::Internal,
            rect: PxRect::new(0, 360, 1920, 1200),
            primary: false,
            features: vec![Feature::Brightness],
            caps: None,
            edid: Some(edid(
                "PCR",
                "Panelcraft",
                0x14a2,
                "Panelcraft 14 WUXGA",
                2024,
                (30, 19),
                (1920, 1200, 60.0),
                8,
            )),
            note: None,
        },
        Monitor {
            key: KEY_MAIN.into(),
            number: 2,
            name: "Arcline 27Q".into(),
            device: "\\\\.\\DISPLAY2".into(),
            connection: Connection::DdcCi,
            rect: PxRect::new(1920, 0, 2560, 1440),
            primary: true,
            features: ddc.to_vec(),
            caps: Some(caps(
                "27Q",
                &[
                    (0x10, &[]),
                    (0x12, &[]),
                    (0x14, &[0x01, 0x05, 0x06, 0x08, 0x0B]),
                    (0x16, &[]),
                    (0x18, &[]),
                    (0x1A, &[]),
                    (0x60, &[0x0F, 0x11, 0x12, 0x1B]),
                    (0x62, &[]),
                    (0xD6, &[0x01, 0x04, 0x05]),
                    (0xDF, &[]),
                ],
            )),
            edid: Some(edid(
                "ARL",
                "Arcline",
                0x27a1,
                "Arcline 27Q",
                2023,
                (60, 34),
                (2560, 1440, 144.0),
                10,
            )),
            note: None,
        },
        Monitor {
            key: KEY_SIDE.into(),
            number: 3,
            name: "Brightwell 24P".into(),
            device: "\\\\.\\DISPLAY3".into(),
            connection: Connection::DdcCi,
            rect: PxRect::new(4480, 0, 1080, 1920),
            primary: false,
            features: vec![
                Feature::Brightness,
                Feature::Contrast,
                Feature::InputSource,
                Feature::PowerMode,
            ],
            caps: Some(caps(
                "24P",
                &[
                    (0x10, &[]),
                    (0x12, &[]),
                    (0x60, &[0x01, 0x03, 0x11]),
                    (0xD6, &[0x01, 0x05]),
                ],
            )),
            edid: Some(edid(
                "BWL",
                "Brightwell",
                0x2402,
                "Brightwell 24P",
                2021,
                (53, 30),
                (1920, 1080, 60.0),
                8,
            )),
            note: None,
        },
        Monitor {
            key: KEY_TV.into(),
            number: 4,
            name: "Living room TV".into(),
            device: "\\\\.\\DISPLAY4".into(),
            connection: Connection::SoftwareOnly,
            rect: PxRect::new(-3840, 0, 3840, 2160),
            primary: false,
            features: Vec::new(),
            caps: None,
            edid: Some(edid(
                "OVT",
                "Oakvale",
                0x5501,
                "Oakvale 55 TV",
                2022,
                (121, 68),
                (3840, 2160, 60.0),
                10,
            )),
            note: Some(
                "This screen did not answer over DDC/CI, so its own brightness cannot be changed from Windows. Many TVs do not support it. DisplayForge can still dim it in software and apply night light.".into(),
            ),
        },
    ]
}

/// The display settings that go with [`monitors`].
pub fn displays() -> Vec<DisplayState> {
    let mut list = vec![
        MockDisplays::display(
            "\\\\.\\DISPLAY1",
            "Built-in display",
            (1920, 1200),
            &[60, 48],
            0,
            false,
            150,
        ),
        MockDisplays::display(
            "\\\\.\\DISPLAY2",
            "Arcline 27Q",
            (2560, 1440),
            &[144, 120, 60],
            1920,
            true,
            125,
        ),
        MockDisplays::display(
            "\\\\.\\DISPLAY3",
            "Brightwell 24P",
            (1920, 1080),
            &[60],
            4480,
            false,
            100,
        ),
        MockDisplays::display(
            "\\\\.\\DISPLAY4",
            "Living room TV",
            (3840, 2160),
            &[60, 30],
            -3840,
            false,
            200,
        ),
    ];
    for (d, m) in list.iter_mut().zip(monitors()) {
        d.rect = m.rect;
    }
    // The side monitor stands upright.
    list[2].orientation = crate::model::Orientation::Portrait;
    list[1].hdr = HdrState {
        supported: true,
        enabled: false,
        sdr_white_nits: None,
    };
    list[3].hdr = HdrState {
        supported: true,
        enabled: true,
        sdr_white_nits: Some(240.0),
    };
    list
}

/// Demo settings: a location, the default schedule switched on, a few
/// renamed monitors and app profiles.
pub fn settings() -> Settings {
    let mut s = Settings {
        location: Some(Location {
            name: "Erbil, Iraq".into(),
            lat: 36.191,
            lon: 44.009,
        }),
        night_light: true,
        kelvin: 4500,
        link_all: true,
        below_zero: true,
        ..Settings::default()
    };
    s.schedule.enabled = true;
    s.idle.enabled = true;
    s.prefs_mut(KEY_SIDE).min = 10;
    s.prefs_mut(KEY_SIDE).max = 90;
    s.prefs_mut(KEY_TV).linked = false;
    s.prefs_mut(KEY_TV).dim_method = crate::settings::DimMethod::Overlay;
    s.profiles = vec![
        AppProfile {
            exe: "vlc.exe".into(),
            trigger: Trigger::Fullscreen,
            brightness: Some(100),
            contrast: None,
            kelvin: Some(KELVIN_NEUTRAL),
            ..AppProfile::default()
        },
        AppProfile {
            exe: "gimp.exe".into(),
            trigger: Trigger::Foreground,
            brightness: Some(80),
            contrast: Some(60),
            kelvin: Some(KELVIN_NEUTRAL),
            only_its_monitor: true,
            ..AppProfile::default()
        },
        AppProfile {
            enabled: false,
            exe: "game.exe".into(),
            trigger: Trigger::Fullscreen,
            brightness: Some(90),
            contrast: Some(70),
            kelvin: None,
            ..AppProfile::default()
        },
    ];
    for h in s.hotkeys.iter_mut().take(5) {
        h.enabled = true;
    }
    s
}

/// Levels the demo monitors start with (raw, max).
fn levels(key: &str) -> Vec<(Feature, Level)> {
    let l = |value, max| Level { value, max };
    match key {
        KEY_LAPTOP => vec![(Feature::Brightness, l(55, 100))],
        KEY_MAIN => vec![
            (Feature::Brightness, l(55, 100)),
            (Feature::Contrast, l(70, 100)),
            (Feature::Volume, l(25, 100)),
            (Feature::InputSource, l(0x0F, 0)),
            (Feature::PowerMode, l(1, 5)),
            (Feature::ColorPreset, l(0x05, 0)),
            (Feature::RedGain, l(50, 100)),
            (Feature::GreenGain, l(48, 100)),
            (Feature::BlueGain, l(46, 100)),
        ],
        KEY_SIDE => vec![
            (Feature::Brightness, l(54, 100)),
            (Feature::Contrast, l(65, 100)),
            (Feature::InputSource, l(0x11, 0)),
            (Feature::PowerMode, l(1, 5)),
        ],
        _ => Vec::new(),
    }
}

/// A mock backend holding the demo monitors and their levels.
pub fn backend() -> MockBackend {
    let b = MockBackend::new(monitors());
    for m in monitors() {
        for (f, level) in levels(&m.key) {
            b.put(&m.key, f, level.value, level.max);
        }
    }
    b
}

/// The state the window shows before the engine has reported anything.
pub fn live_state() -> LiveState {
    let s = settings();
    let monitors = monitors()
        .into_iter()
        .map(|m| {
            let levels: BTreeMap<Feature, Level> = levels(&m.key).into_iter().collect();
            let brightness = match m.key.as_str() {
                KEY_TV => -20,
                _ => 55,
            };
            let prefs = s.prefs(&m.key);
            MonitorView {
                display_name: if prefs.rename.is_empty() {
                    m.name.clone()
                } else {
                    prefs.rename.clone()
                },
                monitor: m,
                levels,
                brightness,
                fading: false,
                error: None,
            }
        })
        .collect();
    let day_start = now() - ((19 * 60 + 40) * 60) as i64;
    LiveState {
        ready: true,
        monitors,
        displays: displays(),
        night_on: true,
        kelvin: 4500,
        active_profile: None,
        schedule: ScheduleStatus {
            enabled: true,
            following: true,
            current: Some("Since 16:21 (sunset - 30 min)".into()),
            next: Some((day_start + (22 * 60 + 30) * 60, "22:30".into())),
            sunrise: Some(day_start + (6 * 60 + 33) * 60),
            sunset: Some(day_start + (16 * 60 + 51) * 60),
        },
        idle_dimmed: false,
        running_apps: vec![
            "code.exe".into(),
            "explorer.exe".into(),
            "firefox.exe".into(),
            "gimp.exe".into(),
            "obs64.exe".into(),
            "spotify.exe".into(),
            "vlc.exe".into(),
        ],
        last_preset: Some("Evening".into()),
        display_result: None,
        vcp_dump: None,
        notice: None,
    }
}

/// A `Shared` for the demo: invented settings and state, a fixed clock and
/// a data folder that is never written.
pub fn shared() -> Arc<Shared> {
    let mut shared = Shared::new(
        PathBuf::from("C:\\Users\\Alex\\AppData\\Local\\DisplayForge"),
        settings(),
        tz(),
        true,
    );
    shared.fixed_now = Some(now());
    if let Ok(mut live) = shared.live.lock() {
        *live = live_state();
    }
    Arc::new(shared)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn demo_is_consistent() {
        let m = monitors();
        let d = displays();
        assert_eq!(m.len(), d.len());
        for (m, d) in m.iter().zip(&d) {
            assert_eq!(m.device, d.device);
            assert!(m.edid.is_some());
        }
        assert_eq!(m.iter().filter(|m| m.primary).count(), 1);
        assert_eq!(d.iter().filter(|d| d.primary).count(), 1);
        assert_eq!(crate::timeutil::fmt_hm(now(), &FixedTz(OFFSET)), "19:40");
        let live = live_state();
        assert!(live.ready && live.monitors.len() == 4);
    }
}
