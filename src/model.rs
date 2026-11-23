//! Types shared by the hardware backends, the engine and the user interface.
//!
//! Everything that talks to real hardware sits behind the [`Backend`] and
//! [`DisplayConfig`] traits, so the engine, the demo, the self-test and the
//! unit tests can run against the mock implementations in `mock.rs`.

use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

/// A stable key for one monitor, used in settings. Built from the EDID
/// (manufacturer + product code + a hash of the rest) or the device path; it
/// is never shown to the user.
pub type MonitorKey = String;

/// A monitor setting that DisplayForge can read or change.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub enum Feature {
    Brightness,
    Contrast,
    Volume,
    /// Non-continuous: the value is an input code (see [`input_name`]).
    InputSource,
    /// Non-continuous: 1 on, 4 standby/off, 5 off (MCCS 0xD6).
    PowerMode,
    /// Non-continuous: a colour preset code (see [`color_preset_name`]).
    ColorPreset,
    RedGain,
    GreenGain,
    BlueGain,
}

impl Feature {
    pub const ALL: [Feature; 9] = [
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

    /// The MCCS VCP code.
    pub fn vcp(self) -> u8 {
        match self {
            Feature::Brightness => 0x10,
            Feature::Contrast => 0x12,
            Feature::Volume => 0x62,
            Feature::InputSource => 0x60,
            Feature::PowerMode => 0xD6,
            Feature::ColorPreset => 0x14,
            Feature::RedGain => 0x16,
            Feature::GreenGain => 0x18,
            Feature::BlueGain => 0x1A,
        }
    }

    pub fn from_vcp(code: u8) -> Option<Feature> {
        Feature::ALL.into_iter().find(|f| f.vcp() == code)
    }

    /// Continuous settings are set as a percentage of their maximum and may
    /// fade; the others take one of a list of codes and are set at once.
    pub fn continuous(self) -> bool {
        !matches!(
            self,
            Feature::InputSource | Feature::PowerMode | Feature::ColorPreset
        )
    }

    pub fn label(self) -> &'static str {
        match self {
            Feature::Brightness => "Brightness",
            Feature::Contrast => "Contrast",
            Feature::Volume => "Volume",
            Feature::InputSource => "Input",
            Feature::PowerMode => "Power",
            Feature::ColorPreset => "Colour preset",
            Feature::RedGain => "Red",
            Feature::GreenGain => "Green",
            Feature::BlueGain => "Blue",
        }
    }
}

/// Human name of an MCCS input source code (VCP 0x60).
pub fn input_name(code: u16) -> String {
    let name = match code & 0xff {
        0x01 => "VGA 1",
        0x02 => "VGA 2",
        0x03 => "DVI 1",
        0x04 => "DVI 2",
        0x05 => "Composite 1",
        0x06 => "Composite 2",
        0x07 => "S-Video 1",
        0x08 => "S-Video 2",
        0x09 => "Tuner 1",
        0x0A => "Tuner 2",
        0x0B => "Tuner 3",
        0x0C => "Component 1",
        0x0D => "Component 2",
        0x0E => "Component 3",
        0x0F => "DisplayPort 1",
        0x10 => "DisplayPort 2",
        0x11 => "HDMI 1",
        0x12 => "HDMI 2",
        0x1B => "USB-C",
        _ => return format!("Input 0x{:02X}", code),
    };
    name.to_string()
}

/// Human name of an MCCS colour preset code (VCP 0x14).
pub fn color_preset_name(code: u16) -> String {
    let name = match code & 0xff {
        0x01 => "sRGB",
        0x02 => "Native",
        0x03 => "4000 K",
        0x04 => "5000 K",
        0x05 => "6500 K",
        0x06 => "7500 K",
        0x07 => "8200 K",
        0x08 => "9300 K",
        0x09 => "10000 K",
        0x0A => "11500 K",
        0x0B => "User 1",
        0x0C => "User 2",
        0x0D => "User 3",
        _ => return format!("Preset 0x{:02X}", code),
    };
    name.to_string()
}

/// Human name of a power mode code (VCP 0xD6).
pub fn power_name(code: u16) -> &'static str {
    match code {
        1 => "On",
        2 => "Standby",
        3 => "Suspend",
        4 => "Off (soft)",
        5 => "Off",
        _ => "Unknown",
    }
}

/// How a monitor's brightness can be controlled.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum Connection {
    /// An external monitor that answers over DDC/CI.
    DdcCi,
    /// A built-in laptop panel, controlled through WMI.
    Internal,
    /// No hardware control (DDC/CI switched off or not supported): only
    /// software dimming (gamma or overlay) and night light are available.
    SoftwareOnly,
}

impl Connection {
    pub fn label(self) -> &'static str {
        match self {
            Connection::DdcCi => "DDC/CI",
            Connection::Internal => "Built-in",
            Connection::SoftwareOnly => "Software only",
        }
    }
}

/// A rectangle in physical desktop pixels.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct PxRect {
    pub x: i32,
    pub y: i32,
    pub w: i32,
    pub h: i32,
}

impl PxRect {
    pub fn new(x: i32, y: i32, w: i32, h: i32) -> Self {
        PxRect { x, y, w, h }
    }
    pub fn right(&self) -> i32 {
        self.x + self.w
    }
    pub fn bottom(&self) -> i32 {
        self.y + self.h
    }
    pub fn contains(&self, x: i32, y: i32) -> bool {
        x >= self.x && y >= self.y && x < self.right() && y < self.bottom()
    }
}

/// A parsed MCCS capabilities string (see `hw::caps::parse`).
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Capabilities {
    /// `prot(...)`, usually "monitor".
    pub protocol: Option<String>,
    /// `type(...)`, e.g. "LCD".
    pub kind: Option<String>,
    /// `model(...)`.
    pub model: Option<String>,
    /// `mccs_ver(...)`, e.g. "2.1".
    pub mccs_version: Option<String>,
    /// Supported VCP codes. An empty list means "continuous" (or values not
    /// listed); a non-empty list holds the allowed values of a code such as
    /// 0x60 (inputs) or 0x14 (colour presets).
    pub vcp: BTreeMap<u8, Vec<u16>>,
}

impl Capabilities {
    pub fn supports(&self, code: u8) -> bool {
        self.vcp.contains_key(&code)
    }
    pub fn values(&self, code: u8) -> &[u16] {
        self.vcp.get(&code).map(|v| v.as_slice()).unwrap_or(&[])
    }
}

/// What the EDID tells about a monitor (see `hw::edid::parse`).
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct EdidInfo {
    /// Three-letter PNP id, e.g. "DEL".
    pub manufacturer_id: String,
    /// A readable maker name when the id is known, else the id.
    pub manufacturer: String,
    pub product_code: u16,
    /// Monitor name descriptor (0xFC), if present.
    pub name: Option<String>,
    /// Serial number: the string descriptor (0xFF) if present, else the
    /// 32-bit number when non-zero. Hidden in the UI unless asked for.
    pub serial: Option<String>,
    pub week: Option<u8>,
    pub year: Option<u16>,
    /// Image size in centimetres (0 when unknown, e.g. projectors).
    pub width_cm: u8,
    pub height_cm: u8,
    /// EDID version, e.g. (1, 4).
    pub version: (u8, u8),
    pub digital: bool,
    /// Bits per colour, when the EDID says (1.4 digital).
    pub bit_depth: Option<u8>,
    /// Preferred (native) mode from the first detailed timing: width, height, refresh in Hz.
    pub native: Option<(u32, u32, f32)>,
}

impl EdidInfo {
    /// Diagonal in inches from the image size, when known.
    pub fn diagonal_inches(&self) -> Option<f32> {
        if self.width_cm == 0 || self.height_cm == 0 {
            return None;
        }
        let w = self.width_cm as f32;
        let h = self.height_cm as f32;
        Some((w * w + h * h).sqrt() / 2.54)
    }
}

/// One monitor as found by a backend.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Monitor {
    pub key: MonitorKey,
    /// 1-based number, in the order Windows numbers displays when known,
    /// else left to right. Used by `--monitor N` and "Identify".
    pub number: usize,
    /// Friendly name, e.g. "DELL U2720Q" or "Built-in display".
    pub name: String,
    /// GDI device name, e.g. `\\.\DISPLAY1`.
    pub device: String,
    pub connection: Connection,
    /// Position and size on the desktop, physical pixels.
    pub rect: PxRect,
    pub primary: bool,
    /// Hardware features that can be read and set on this monitor.
    pub features: Vec<Feature>,
    pub caps: Option<Capabilities>,
    pub edid: Option<EdidInfo>,
    /// A plain explanation when something is not available, e.g. why
    /// DDC/CI did not answer.
    pub note: Option<String>,
}

impl Monitor {
    pub fn supports(&self, f: Feature) -> bool {
        self.features.contains(&f)
    }
}

/// A current value and its maximum, as read from the monitor.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default, Serialize, Deserialize)]
pub struct Level {
    pub value: u32,
    pub max: u32,
}

impl Level {
    pub fn percent(&self) -> u32 {
        if self.max == 0 {
            0
        } else {
            ((self.value as u64 * 100 + self.max as u64 / 2) / self.max as u64).min(100) as u32
        }
    }
    /// The raw value for a percentage of this level's maximum.
    pub fn raw_for(&self, percent: u32) -> u32 {
        let max = if self.max == 0 { 100 } else { self.max };
        ((percent.min(100) as u64 * max as u64 + 50) / 100) as u32
    }
}

/// An error from a backend, in words a user can act on.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct HwError(pub String);

impl std::fmt::Display for HwError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}

impl std::error::Error for HwError {}

impl From<&str> for HwError {
    fn from(s: &str) -> Self {
        HwError(s.to_string())
    }
}

/// A 256-entry gamma ramp per channel, 0..=65535 (the SetDeviceGammaRamp layout).
#[derive(Clone, PartialEq, Eq)]
pub struct GammaRamp {
    pub r: [u16; 256],
    pub g: [u16; 256],
    pub b: [u16; 256],
}

impl GammaRamp {
    pub fn identity() -> Self {
        let mut r = [0u16; 256];
        for (i, v) in r.iter_mut().enumerate() {
            *v = (i as u16) << 8 | i as u16;
        }
        GammaRamp { r, g: r, b: r }
    }
    pub fn is_identity(&self) -> bool {
        *self == GammaRamp::identity()
    }
}

impl std::fmt::Debug for GammaRamp {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "GammaRamp(r255={}, g255={}, b255={})",
            self.r[255], self.g[255], self.b[255]
        )
    }
}

/// Monitor hardware access. Implemented by `hw::WinBackend` (DDC/CI, WMI,
/// gamma) and by `mock::MockBackend`.
///
/// Calls may be slow (DDC/CI takes tens of milliseconds per request) and are
/// made from the controller's worker thread only.
pub trait Backend: Send {
    /// Finds the connected monitors and what each supports. May be slow
    /// (reads capabilities strings); called at start and after display changes.
    fn enumerate(&mut self) -> Vec<Monitor>;
    /// Reads a feature's current value and maximum.
    fn get(&mut self, key: &str, feature: Feature) -> Result<Level, HwError>;
    /// Writes a raw value (for continuous features, 0..=max of [`Backend::get`]).
    fn set(&mut self, key: &str, feature: Feature, value: u32) -> Result<(), HwError>;
    /// Reads any VCP code (for the read-only "all settings" list).
    fn read_vcp(&mut self, key: &str, code: u8) -> Result<Level, HwError>;
    /// Sets the monitor's gamma ramp; `None` restores the identity ramp.
    fn set_gamma(&mut self, key: &str, ramp: Option<&GammaRamp>) -> Result<(), HwError>;
    /// Smallest pause between two writes to the same monitor, in milliseconds.
    fn min_interval_ms(&self, key: &str) -> u64 {
        let _ = key;
        50
    }
}

/// A display mode.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub struct DisplayMode {
    pub width: u32,
    pub height: u32,
    /// Refresh rate in Hz (whole numbers, as Windows lists them).
    pub refresh: u32,
    pub bits: u32,
}

impl DisplayMode {
    pub fn label(&self) -> String {
        format!("{} x {}", self.width, self.height)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum Orientation {
    Landscape,
    Portrait,
    LandscapeFlipped,
    PortraitFlipped,
}

impl Orientation {
    pub const ALL: [Orientation; 4] = [
        Orientation::Landscape,
        Orientation::Portrait,
        Orientation::LandscapeFlipped,
        Orientation::PortraitFlipped,
    ];
    pub fn label(self) -> &'static str {
        match self {
            Orientation::Landscape => "Landscape",
            Orientation::Portrait => "Portrait",
            Orientation::LandscapeFlipped => "Landscape (flipped)",
            Orientation::PortraitFlipped => "Portrait (flipped)",
        }
    }
}

/// HDR state of a display, read-only.
#[derive(Clone, Copy, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct HdrState {
    pub supported: bool,
    pub enabled: bool,
    /// The "SDR content brightness" white level in nits, when HDR is on.
    pub sdr_white_nits: Option<f32>,
}

/// The current state of one display output (GDI device).
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct DisplayState {
    /// GDI device name, e.g. `\\.\DISPLAY1` (matches [`Monitor::device`]).
    pub device: String,
    pub name: String,
    pub current: DisplayMode,
    /// All modes the driver lists, sorted largest first.
    pub modes: Vec<DisplayMode>,
    pub orientation: Orientation,
    pub primary: bool,
    /// Position and size on the desktop, physical pixels.
    pub rect: PxRect,
    /// Effective scaling in percent (100, 125, 150 ...).
    pub scale_percent: u32,
    pub hdr: HdrState,
}

/// A change to one display. Fields left `None` / `false` stay as they are.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct DisplayChange {
    pub mode: Option<DisplayMode>,
    pub orientation: Option<Orientation>,
    pub make_primary: bool,
}

impl DisplayChange {
    pub fn is_empty(&self) -> bool {
        self.mode.is_none() && self.orientation.is_none() && !self.make_primary
    }
}

/// Display settings (resolution, refresh rate, orientation, primary).
/// Implemented by `hw::display::WinDisplays` and `mock::MockDisplays`.
pub trait DisplayConfig: Send {
    fn displays(&mut self) -> Vec<DisplayState>;
    /// Applies a change to one display and returns what it was before, so
    /// that it can be reverted.
    fn apply(&mut self, device: &str, change: &DisplayChange) -> Result<DisplayChange, HwError>;
}

/// Maps a user brightness (0..=100) onto a monitor's calibrated range, so
/// that monitors with different panels look alike at the same number.
pub fn calibrate(percent: u32, min: u32, max: u32) -> u32 {
    let (lo, hi) = if min <= max { (min, max) } else { (max, min) };
    let hi = hi.min(100);
    let lo = lo.min(hi);
    lo + ((hi - lo) * percent.min(100) + 50) / 100
}

/// The inverse of [`calibrate`]: which user brightness a hardware percentage shows as.
pub fn uncalibrate(hw: u32, min: u32, max: u32) -> u32 {
    let (lo, hi) = if min <= max { (min, max) } else { (max, min) };
    let hi = hi.min(100);
    let lo = lo.min(hi);
    if hi == lo {
        return 100;
    }
    let hw = hw.clamp(lo, hi);
    ((hw - lo) * 100 + (hi - lo) / 2) / (hi - lo)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn levels_and_percentages() {
        let l = Level {
            value: 50,
            max: 100,
        };
        assert_eq!(l.percent(), 50);
        let l = Level {
            value: 128,
            max: 255,
        };
        assert_eq!(l.percent(), 50);
        assert_eq!(l.raw_for(100), 255);
        assert_eq!(l.raw_for(0), 0);
        assert_eq!(Level { value: 3, max: 0 }.percent(), 0);
    }

    #[test]
    fn calibration_round_trip() {
        assert_eq!(calibrate(0, 10, 90), 10);
        assert_eq!(calibrate(100, 10, 90), 90);
        assert_eq!(calibrate(50, 10, 90), 50);
        assert_eq!(calibrate(50, 0, 100), 50);
        for p in [0, 13, 50, 77, 100] {
            assert_eq!(uncalibrate(calibrate(p, 0, 100), 0, 100), p);
        }
        assert_eq!(uncalibrate(5, 10, 90), 0);
        assert_eq!(calibrate(40, 90, 10), calibrate(40, 10, 90));
    }

    #[test]
    fn feature_codes() {
        for f in Feature::ALL {
            assert_eq!(Feature::from_vcp(f.vcp()), Some(f));
        }
        assert!(!Feature::InputSource.continuous());
        assert!(Feature::Brightness.continuous());
        assert_eq!(input_name(0x0F), "DisplayPort 1");
        assert_eq!(input_name(0x99), "Input 0x99");
    }

    #[test]
    fn gamma_identity() {
        let g = GammaRamp::identity();
        assert!(g.is_identity());
        assert_eq!(g.r[0], 0);
        assert_eq!(g.r[255], 65535);
    }
}
