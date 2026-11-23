//! Real hardware access on Windows: DDC/CI (dxva2), WMI for built-in
//! panels, gamma ramps, display settings and the dimming overlay.
pub mod caps;
pub mod ddc;
pub mod display;
pub mod edid;
pub mod gamma;
pub mod overlay;
pub mod wmi;

use crate::model::{
    Backend, Capabilities, Connection, EdidInfo, Feature, GammaRamp, HwError, Level, Monitor,
    PxRect,
};
use std::collections::HashMap;
use windows::core::PCWSTR;
use windows::Win32::Foundation::{BOOL, LPARAM, RECT, WPARAM};
use windows::Win32::Graphics::Gdi::{
    EnumDisplayDevicesW, EnumDisplayMonitors, GetMonitorInfoW, DISPLAY_DEVICEW,
    DISPLAY_DEVICE_ACTIVE, HDC, HMONITOR, MONITORINFO, MONITORINFOEXW,
};
use windows::Win32::UI::WindowsAndMessaging::{
    PostMessageW, EDD_GET_DEVICE_INTERFACE_NAME, HWND_BROADCAST, MONITORINFOF_PRIMARY,
    SC_MONITORPOWER, WM_SYSCOMMAND,
};

/// A NUL-terminated UTF-16 copy of `s`.
pub(crate) fn wide(s: &str) -> Vec<u16> {
    s.encode_utf16().chain(std::iter::once(0)).collect()
}

/// A string from a fixed UTF-16 buffer, up to the first NUL.
pub(crate) fn from_wide(buf: &[u16]) -> String {
    let end = buf.iter().position(|&c| c == 0).unwrap_or(buf.len());
    String::from_utf16_lossy(&buf[..end]).trim().to_string()
}

/// One HMONITOR (a desktop area served by one GDI device).
#[derive(Clone, Debug)]
pub(crate) struct DesktopMonitor {
    pub handle: isize,
    pub device: String,
    pub rect: PxRect,
    pub primary: bool,
}

unsafe extern "system" fn collect_monitor(h: HMONITOR, _: HDC, _: *mut RECT, data: LPARAM) -> BOOL {
    // SAFETY: `data` is the Vec passed by `monitors`, alive for the whole enumeration.
    let list = unsafe { &mut *(data.0 as *mut Vec<DesktopMonitor>) };
    let mut info = MONITORINFOEXW::default();
    info.monitorInfo.cbSize = std::mem::size_of::<MONITORINFOEXW>() as u32;
    // SAFETY: `info` is a MONITORINFOEXW with cbSize set accordingly.
    if unsafe { GetMonitorInfoW(h, &mut info as *mut MONITORINFOEXW as *mut MONITORINFO) }.as_bool()
    {
        let r = info.monitorInfo.rcMonitor;
        list.push(DesktopMonitor {
            handle: h.0 as isize,
            device: from_wide(&info.szDevice),
            rect: PxRect::new(r.left, r.top, r.right - r.left, r.bottom - r.top),
            primary: info.monitorInfo.dwFlags & MONITORINFOF_PRIMARY != 0,
        });
    }
    BOOL(1)
}

/// The monitors of the desktop, as EnumDisplayMonitors lists them.
pub(crate) fn monitors() -> Vec<DesktopMonitor> {
    let mut list: Vec<DesktopMonitor> = Vec::new();
    // SAFETY: the callback only touches `list`, which outlives the call.
    unsafe {
        let _ = EnumDisplayMonitors(
            HDC::default(),
            None,
            Some(collect_monitor),
            LPARAM(&mut list as *mut Vec<DesktopMonitor> as isize),
        );
    }
    list
}

/// Device interface paths of the monitors attached to a GDI device.
fn monitor_interfaces(gdi_device: &str) -> Vec<String> {
    let name = wide(gdi_device);
    let mut out = Vec::new();
    for i in 0..8 {
        let mut dd = DISPLAY_DEVICEW {
            cb: std::mem::size_of::<DISPLAY_DEVICEW>() as u32,
            ..Default::default()
        };
        // SAFETY: `name` is NUL-terminated and `dd` is sized via cb.
        let ok = unsafe {
            EnumDisplayDevicesW(
                PCWSTR(name.as_ptr()),
                i,
                &mut dd,
                EDD_GET_DEVICE_INTERFACE_NAME,
            )
        };
        if !ok.as_bool() {
            break;
        }
        if dd.StateFlags & DISPLAY_DEVICE_ACTIVE != 0 {
            out.push(from_wide(&dd.DeviceID));
        }
    }
    out
}

/// Reads the EDID Windows stored for a monitor (read-only registry access).
fn read_edid(hardware_id: &str) -> Option<Vec<u8>> {
    use winreg::enums::{HKEY_LOCAL_MACHINE, KEY_READ};
    let path = format!(
        "SYSTEM\\CurrentControlSet\\Enum\\{}\\Device Parameters",
        hardware_id
    );
    let key = winreg::RegKey::predef(HKEY_LOCAL_MACHINE)
        .open_subkey_with_flags(path, KEY_READ)
        .ok()?;
    let value = key.get_raw_value("EDID").ok()?;
    (!value.bytes.is_empty()).then_some(value.bytes)
}

/// 32-bit FNV-1a.
pub fn fnv1a(bytes: &[u8]) -> u32 {
    bytes.iter().fold(0x811c_9dc5u32, |h, &b| {
        (h ^ b as u32).wrapping_mul(0x0100_0193)
    })
}

/// A stable key: maker + product code + a hash of the whole EDID, else a
/// hash of the device path (or GDI device name).
pub fn monitor_key(edid: Option<(&EdidInfo, &[u8])>, device_path: &str) -> String {
    match edid {
        Some((info, bytes)) => format!(
            "{}{:04X}-{:08x}",
            info.manufacturer_id,
            info.product_code,
            fnv1a(bytes)
        ),
        None => format!(
            "DEV-{:08x}",
            fnv1a(device_path.to_ascii_uppercase().as_bytes())
        ),
    }
}

/// Adds `#2`, `#3` ... to repeated keys (identical twin monitors).
pub fn dedupe_keys(keys: &mut [String]) {
    let mut seen: HashMap<String, usize> = HashMap::new();
    for k in keys.iter_mut() {
        let n = seen.entry(k.clone()).or_insert(0);
        *n += 1;
        if *n > 1 {
            *k = format!("{}#{}", k, n);
        }
    }
}

/// The number in a GDI device name: `\\.\DISPLAY3` -> 3.
pub fn display_number(device: &str) -> Option<usize> {
    let upper = device.to_ascii_uppercase();
    let rest = &upper[upper.rfind("DISPLAY")? + "DISPLAY".len()..];
    let digits: String = rest.chars().take_while(|c| c.is_ascii_digit()).collect();
    digits.parse().ok().filter(|&n| n > 0)
}

/// The features listed in a capabilities string.
pub fn features_from_caps(caps: &Capabilities) -> Vec<Feature> {
    Feature::ALL
        .into_iter()
        .filter(|f| caps.supports(f.vcp()))
        .collect()
}

const DDC_OFF_NOTE: &str = "This monitor did not answer over DDC/CI, so only software dimming is available. \
     Turn on DDC/CI in the monitor's on-screen menu. Some docks, adapters, KVM switches and TVs do not pass DDC/CI through.";
const PANEL_NOTE: &str = "Windows does not offer brightness control for this built-in screen, so only software dimming is available.";

/// What the backend keeps per monitor.
#[derive(Clone, Debug)]
struct Entry {
    name: String,
    device: String,
    connection: Connection,
    handle: Option<isize>,
    hardware_id: Option<String>,
}

/// DDC/CI + WMI + gamma for the monitors attached to this PC.
#[derive(Default)]
pub struct WinBackend {
    entries: HashMap<String, Entry>,
    /// Every physical monitor handle held, released on re-enumerate and drop.
    handles: Vec<isize>,
    /// Capabilities read this session, per monitor key (they are slow to read).
    caps_cache: HashMap<String, Capabilities>,
}

impl WinBackend {
    pub fn new() -> Self {
        WinBackend::default()
    }

    fn entry(&self, key: &str) -> Result<&Entry, HwError> {
        self.entries.get(key).ok_or_else(|| {
            HwError("This monitor is no longer connected. Look for monitors again.".into())
        })
    }

    fn ddc_handle(&self, key: &str, what: &str) -> Result<(isize, String), HwError> {
        let e = self.entry(key)?;
        match (e.connection, e.handle) {
            (Connection::DdcCi, Some(h)) => Ok((h, e.name.clone())),
            (Connection::Internal, _) => Err(HwError(format!(
                "The built-in screen does not support {what}; only brightness can be changed."
            ))),
            _ => Err(HwError(format!(
                "{} cannot be controlled over DDC/CI. Turn on DDC/CI in the monitor's on-screen menu, then look for monitors again.",
                e.name
            ))),
        }
    }

    fn release(&mut self) {
        ddc::destroy(&self.handles);
        self.handles.clear();
        self.entries.clear();
    }
}

impl Drop for WinBackend {
    fn drop(&mut self) {
        self.release();
    }
}

/// One monitor found during enumeration, before keys and numbers are final.
struct Found {
    monitor: Monitor,
    entry: Entry,
    x: i32,
}

impl Backend for WinBackend {
    fn enumerate(&mut self) -> Vec<Monitor> {
        self.release();
        let paths = display::paths();
        let mut found: Vec<Found> = Vec::new();
        for dm in monitors() {
            let physical = ddc::physical_monitors(dm.handle);
            self.handles.extend(physical.iter().map(|p| p.handle));
            let interfaces = monitor_interfaces(&dm.device);
            let count = physical.len().max(interfaces.len()).max(1);
            for i in 0..count {
                let phys = physical.get(i);
                let device_path = interfaces.get(i).cloned().unwrap_or_default();
                let hardware_id = wmi::hardware_id(&device_path);
                let edid_bytes = hardware_id.as_deref().and_then(read_edid);
                let edid = edid_bytes.as_deref().and_then(edid::parse);
                let path = paths
                    .iter()
                    .find(|p| {
                        !device_path.is_empty() && p.device_path.eq_ignore_ascii_case(&device_path)
                    })
                    .or_else(|| {
                        paths
                            .iter()
                            .find(|p| p.gdi.eq_ignore_ascii_case(&dm.device))
                    });
                let internal = path.is_some_and(|p| p.internal);
                let name = path
                    .map(|p| p.friendly.clone())
                    .filter(|n| !n.is_empty())
                    .or_else(|| edid.as_ref().and_then(|e| e.name.clone()))
                    .or_else(|| {
                        phys.map(|p| p.description.clone())
                            .filter(|d| !d.is_empty() && !d.starts_with("Generic"))
                    })
                    .unwrap_or_else(|| {
                        if internal {
                            "Built-in display".to_string()
                        } else {
                            "Display".to_string()
                        }
                    });
                let key_source = if device_path.is_empty() {
                    dm.device.as_str()
                } else {
                    device_path.as_str()
                };
                let key = monitor_key(edid.as_ref().zip(edid_bytes.as_deref()), key_source);
                let monitor = Monitor {
                    key,
                    number: display_number(&dm.device).unwrap_or(0),
                    name: name.clone(),
                    device: dm.device.clone(),
                    connection: Connection::SoftwareOnly,
                    rect: dm.rect,
                    primary: dm.primary,
                    features: Vec::new(),
                    caps: None,
                    edid,
                    note: None,
                };
                found.push(Found {
                    monitor,
                    entry: Entry {
                        name,
                        device: dm.device.clone(),
                        connection: if internal {
                            Connection::Internal
                        } else {
                            Connection::DdcCi
                        },
                        handle: phys.map(|p| p.handle),
                        hardware_id,
                    },
                    x: dm.rect.x,
                });
            }
        }

        // Numbers 1..N: in Windows' display order when known, else left to right.
        found.sort_by_key(|f| (f.monitor.number == 0, f.monitor.number, f.x));
        for (number, f) in (1..).zip(found.iter_mut()) {
            f.monitor.number = number;
        }
        let mut keys: Vec<String> = found.iter().map(|f| f.monitor.key.clone()).collect();
        dedupe_keys(&mut keys);

        let mut out = Vec::new();
        for (mut f, key) in found.into_iter().zip(keys) {
            f.monitor.key = key.clone();
            let m = &mut f.monitor;
            if f.entry.connection == Connection::Internal {
                if wmi::available(f.entry.hardware_id.as_deref()) {
                    m.connection = Connection::Internal;
                    m.features = vec![Feature::Brightness];
                } else {
                    m.connection = Connection::SoftwareOnly;
                    m.note = Some(PANEL_NOTE.into());
                    f.entry.connection = Connection::SoftwareOnly;
                }
            } else if let Some(h) = f.entry.handle {
                let caps = match self.caps_cache.get(&key) {
                    Some(c) => Some(c.clone()),
                    None => ddc::capabilities_string(h)
                        .map(|s| caps::parse(&s))
                        .filter(|c| !c.vcp.is_empty()),
                };
                if let Some(c) = &caps {
                    self.caps_cache.insert(key.clone(), c.clone());
                    m.features = features_from_caps(c);
                }
                if m.features.is_empty() {
                    // No usable capabilities: ask for brightness and contrast directly.
                    for f in [Feature::Brightness, Feature::Contrast] {
                        if ddc::get_vcp(h, f.vcp()).is_some() {
                            m.features.push(f);
                        }
                    }
                }
                m.caps = caps;
                if m.features.is_empty() {
                    m.connection = Connection::SoftwareOnly;
                    m.note = Some(DDC_OFF_NOTE.into());
                    f.entry.connection = Connection::SoftwareOnly;
                } else {
                    m.connection = Connection::DdcCi;
                }
            } else {
                m.note = Some(DDC_OFF_NOTE.into());
                f.entry.connection = Connection::SoftwareOnly;
            }
            self.entries.insert(key, f.entry);
            out.push(f.monitor);
        }
        out
    }

    fn get(&mut self, key: &str, feature: Feature) -> Result<Level, HwError> {
        let e = self.entry(key)?;
        if e.connection == Connection::Internal {
            if feature != Feature::Brightness {
                return Err(HwError(format!(
                    "The built-in screen does not support {}; only brightness can be changed.",
                    feature.label().to_lowercase()
                )));
            }
            let value = wmi::get_brightness(e.hardware_id.as_deref())?;
            return Ok(Level {
                value: value as u32,
                max: 100,
            });
        }
        self.read_vcp(key, feature.vcp())
    }

    fn set(&mut self, key: &str, feature: Feature, value: u32) -> Result<(), HwError> {
        let e = self.entry(key)?;
        if e.connection == Connection::Internal && feature == Feature::Brightness {
            return wmi::set_brightness(e.hardware_id.as_deref(), value.min(100) as u8);
        }
        let (h, name) = self.ddc_handle(key, &feature.label().to_lowercase())?;
        if ddc::set_vcp(h, feature.vcp(), value) {
            Ok(())
        } else {
            Err(HwError(format!(
                "{name} did not accept the new {}. Check that DDC/CI is on in the monitor's menu and try again.",
                feature.label().to_lowercase()
            )))
        }
    }

    fn read_vcp(&mut self, key: &str, code: u8) -> Result<Level, HwError> {
        let (h, name) = self.ddc_handle(key, &format!("code 0x{code:02X}"))?;
        ddc::get_vcp(h, code)
            .map(|(value, max)| Level { value, max })
            .ok_or_else(|| {
                HwError(format!(
                    "{name} did not answer for code 0x{code:02X}. It may not support it, or DDC/CI may be busy; try again."
                ))
            })
    }

    fn set_gamma(&mut self, key: &str, ramp: Option<&GammaRamp>) -> Result<(), HwError> {
        let device = self.entry(key)?.device.clone();
        gamma::set_device_gamma(&device, ramp)
    }

    fn min_interval_ms(&self, key: &str) -> u64 {
        match self.entries.get(key).map(|e| e.connection) {
            Some(Connection::Internal) => 0,
            _ => 50,
        }
    }
}

/// Turns all screens off (they come back on with input).
pub fn screens_off() {
    // SAFETY: a posted broadcast message with constant arguments (never sent,
    // so a hung window cannot block us).
    unsafe {
        let _ = PostMessageW(
            HWND_BROADCAST,
            WM_SYSCOMMAND,
            WPARAM(SC_MONITORPOWER as usize),
            LPARAM(2),
        );
    }
}

/// One line of the probe report for a monitor (no serials, keys or paths).
pub fn probe_line(m: &Monitor, values: &[(Feature, Result<Level, HwError>)]) -> String {
    let features: Vec<&str> = m.features.iter().map(|f| f.label()).collect();
    let mut line = format!(
        "Monitor {}: {} | {} | {}x{}{} | features: {} | capabilities: {}",
        m.number,
        m.name,
        m.connection.label(),
        m.rect.w,
        m.rect.h,
        if m.primary { " | primary" } else { "" },
        if features.is_empty() {
            "none".to_string()
        } else {
            features.join(", ")
        },
        match &m.caps {
            Some(c) => format!(
                "read ({} codes{})",
                c.vcp.len(),
                c.mccs_version
                    .as_ref()
                    .map(|v| format!(", MCCS {v}"))
                    .unwrap_or_default()
            ),
            None => "not read".to_string(),
        }
    );
    for (f, r) in values {
        match r {
            Ok(l) => line.push_str(&format!(
                " | {} {}/{}",
                f.label().to_lowercase(),
                l.value,
                l.max
            )),
            Err(e) => line.push_str(&format!(" | {} error: {}", f.label().to_lowercase(), e)),
        }
    }
    if let Some(n) = &m.note {
        line.push_str(&format!(" | note: {n}"));
    }
    line
}

/// Finds the monitors and reads brightness and contrast (read-only). The
/// lines hold no serial numbers, keys, device paths or other identifiers.
pub fn probe() -> Vec<String> {
    let mut backend = WinBackend::new();
    let monitors = backend.enumerate();
    let mut lines = vec![format!("{} monitor(s) found", monitors.len())];
    for m in &monitors {
        let mut values = Vec::new();
        for f in [Feature::Brightness, Feature::Contrast] {
            if m.supports(f) {
                values.push((f, backend.get(&m.key, f)));
            }
        }
        lines.push(probe_line(m, &values));
    }
    lines
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn keys_are_stable_and_twins_differ() {
        let bytes = edid::tests::sample("DEL", 0xA0F1, "TEST U27", None);
        let info = edid::parse(&bytes).unwrap();
        let k1 = monitor_key(Some((&info, &bytes)), "x");
        let k2 = monitor_key(Some((&info, &bytes)), "y");
        assert_eq!(k1, k2);
        assert!(k1.starts_with("DELA0F1-"));
        assert_eq!(k1.len(), "DELA0F1-".len() + 8);
        let other = edid::tests::sample("DEL", 0xA0F1, "TEST U27", Some("B"));
        let k3 = monitor_key(Some((&info, &other)), "x");
        assert_ne!(k1, k3);
        let p1 = monitor_key(None, "\\\\?\\DISPLAY#ABC#1");
        assert_eq!(p1, monitor_key(None, "\\\\?\\display#abc#1"));
        assert!(p1.starts_with("DEV-"));

        let mut keys = vec![
            "A".to_string(),
            "B".to_string(),
            "A".to_string(),
            "A".to_string(),
        ];
        dedupe_keys(&mut keys);
        assert_eq!(keys, vec!["A", "B", "A#2", "A#3"]);
    }

    #[test]
    fn fnv_known_values() {
        assert_eq!(fnv1a(b""), 0x811c9dc5);
        assert_eq!(fnv1a(b"a"), 0xe40c292c);
    }

    #[test]
    fn display_numbers() {
        assert_eq!(display_number("\\\\.\\DISPLAY1"), Some(1));
        assert_eq!(display_number("\\\\.\\DISPLAY12"), Some(12));
        assert_eq!(display_number("\\\\.\\display3"), Some(3));
        assert_eq!(display_number("\\\\.\\DISPLAY"), None);
        assert_eq!(display_number("other"), None);
    }

    #[test]
    fn features_follow_caps() {
        let c = caps::parse("(vcp(10 12 60(0F 11) 62 E0)mccs_ver(2.1))");
        assert_eq!(
            features_from_caps(&c),
            vec![
                Feature::Brightness,
                Feature::Contrast,
                Feature::Volume,
                Feature::InputSource
            ]
        );
    }

    #[test]
    fn probe_line_has_no_identifiers() {
        let m = Monitor {
            key: "SECRETKEY-1234".into(),
            number: 2,
            name: "Test Screen".into(),
            device: "\\\\.\\DISPLAY2".into(),
            connection: Connection::DdcCi,
            rect: PxRect::new(0, 0, 2560, 1440),
            primary: true,
            features: vec![Feature::Brightness],
            caps: Some(caps::parse("vcp(10)mccs_ver(2.2)")),
            edid: edid::parse(&edid::tests::sample("DEL", 1, "X", Some("SERIAL99"))),
            note: None,
        };
        let line = probe_line(
            &m,
            &[(
                Feature::Brightness,
                Ok(Level {
                    value: 40,
                    max: 100,
                }),
            )],
        );
        assert!(line.contains("Monitor 2: Test Screen | DDC/CI | 2560x1440 | primary"));
        assert!(line.contains("brightness 40/100"));
        assert!(line.contains("MCCS 2.2"));
        assert!(
            !line.contains("SECRETKEY") && !line.contains("SERIAL99") && !line.contains("DISPLAY2")
        );
    }

    /// Reads the real monitors (read-only) and prints what was found.
    /// Run with `cargo test --lib hw::tests::probe_real -- --ignored --nocapture`.
    #[test]
    #[ignore]
    fn probe_real() {
        for line in probe() {
            println!("{line}");
        }
    }
}
