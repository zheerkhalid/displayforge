//! Display settings: modes, orientation, primary display, scaling and HDR status.
use crate::model::{
    DisplayChange, DisplayConfig, DisplayMode, DisplayState, HdrState, HwError, Orientation, PxRect,
};
use windows::core::PCWSTR;
use windows::Win32::Devices::Display::{
    DisplayConfigGetDeviceInfo, GetDisplayConfigBufferSizes, QueryDisplayConfig,
    DISPLAYCONFIG_DEVICE_INFO_GET_ADVANCED_COLOR_INFO,
    DISPLAYCONFIG_DEVICE_INFO_GET_SDR_WHITE_LEVEL, DISPLAYCONFIG_DEVICE_INFO_GET_SOURCE_NAME,
    DISPLAYCONFIG_DEVICE_INFO_GET_TARGET_NAME, DISPLAYCONFIG_DEVICE_INFO_HEADER,
    DISPLAYCONFIG_DEVICE_INFO_TYPE, DISPLAYCONFIG_GET_ADVANCED_COLOR_INFO, DISPLAYCONFIG_MODE_INFO,
    DISPLAYCONFIG_OUTPUT_TECHNOLOGY_DISPLAYPORT_EMBEDDED, DISPLAYCONFIG_OUTPUT_TECHNOLOGY_INTERNAL,
    DISPLAYCONFIG_OUTPUT_TECHNOLOGY_UDI_EMBEDDED, DISPLAYCONFIG_PATH_INFO,
    DISPLAYCONFIG_SDR_WHITE_LEVEL, DISPLAYCONFIG_SOURCE_DEVICE_NAME,
    DISPLAYCONFIG_TARGET_DEVICE_NAME, QDC_ONLY_ACTIVE_PATHS,
};
use windows::Win32::Foundation::{ERROR_INSUFFICIENT_BUFFER, ERROR_SUCCESS, HWND, LUID, POINTL};
use windows::Win32::Graphics::Gdi::{
    ChangeDisplaySettingsExW, EnumDisplayDevicesW, EnumDisplaySettingsExW, CDS_NORESET,
    CDS_SET_PRIMARY, CDS_TEST, CDS_TYPE, CDS_UPDATEREGISTRY, DEVMODEW, DEVMODE_DISPLAY_ORIENTATION,
    DISPLAY_DEVICEW, DISPLAY_DEVICE_ACTIVE, DISPLAY_DEVICE_PRIMARY_DEVICE, DISP_CHANGE,
    DISP_CHANGE_RESTART, DISP_CHANGE_SUCCESSFUL, DM_BITSPERPEL, DM_DISPLAYFREQUENCY,
    DM_DISPLAYORIENTATION, DM_PELSHEIGHT, DM_PELSWIDTH, DM_POSITION, ENUM_CURRENT_SETTINGS,
    ENUM_DISPLAY_SETTINGS_FLAGS, ENUM_DISPLAY_SETTINGS_MODE,
};
use windows::Win32::UI::HiDpi::{GetDpiForMonitor, MDT_EFFECTIVE_DPI};

/// One active display path as DisplayConfig reports it.
#[derive(Clone, Debug)]
pub struct PathInfo {
    /// GDI device name, `\\.\DISPLAY1`.
    pub gdi: String,
    /// The monitor's friendly name ("DELL U2720Q"); may be empty.
    pub friendly: String,
    /// Device interface path of the monitor (`\\?\DISPLAY#...`).
    pub device_path: String,
    /// A built-in panel (internal, embedded DisplayPort or UDI).
    pub internal: bool,
    adapter: LUID,
    target: u32,
}

fn header(
    kind: DISPLAYCONFIG_DEVICE_INFO_TYPE,
    size: usize,
    adapter: LUID,
    id: u32,
) -> DISPLAYCONFIG_DEVICE_INFO_HEADER {
    DISPLAYCONFIG_DEVICE_INFO_HEADER {
        r#type: kind,
        size: size as u32,
        adapterId: adapter,
        id,
    }
}

/// The active display paths, with names and connection types.
pub fn paths() -> Vec<PathInfo> {
    let mut paths: Vec<DISPLAYCONFIG_PATH_INFO> = Vec::new();
    let mut modes: Vec<DISPLAYCONFIG_MODE_INFO> = Vec::new();
    for _ in 0..4 {
        let (mut np, mut nm) = (0u32, 0u32);
        // SAFETY: valid out pointers.
        if unsafe { GetDisplayConfigBufferSizes(QDC_ONLY_ACTIVE_PATHS, &mut np, &mut nm) }
            != ERROR_SUCCESS
        {
            return Vec::new();
        }
        paths = vec![DISPLAYCONFIG_PATH_INFO::default(); np as usize];
        modes = vec![DISPLAYCONFIG_MODE_INFO::default(); nm as usize];
        // SAFETY: the arrays have the sizes passed with them.
        let r = unsafe {
            QueryDisplayConfig(
                QDC_ONLY_ACTIVE_PATHS,
                &mut np,
                paths.as_mut_ptr(),
                &mut nm,
                modes.as_mut_ptr(),
                None,
            )
        };
        if r == ERROR_SUCCESS {
            paths.truncate(np as usize);
            break;
        }
        paths.clear();
        if r != ERROR_INSUFFICIENT_BUFFER {
            break;
        }
    }
    let _ = modes;
    paths
        .iter()
        .map(|p| {
            let mut source = DISPLAYCONFIG_SOURCE_DEVICE_NAME {
                header: header(
                    DISPLAYCONFIG_DEVICE_INFO_GET_SOURCE_NAME,
                    std::mem::size_of::<DISPLAYCONFIG_SOURCE_DEVICE_NAME>(),
                    p.sourceInfo.adapterId,
                    p.sourceInfo.id,
                ),
                ..Default::default()
            };
            let mut target = DISPLAYCONFIG_TARGET_DEVICE_NAME {
                header: header(
                    DISPLAYCONFIG_DEVICE_INFO_GET_TARGET_NAME,
                    std::mem::size_of::<DISPLAYCONFIG_TARGET_DEVICE_NAME>(),
                    p.targetInfo.adapterId,
                    p.targetInfo.id,
                ),
                ..Default::default()
            };
            // SAFETY: each packet starts with a header giving its own type and size.
            let (src_ok, tgt_ok) = unsafe {
                (
                    DisplayConfigGetDeviceInfo(&mut source.header) == 0,
                    DisplayConfigGetDeviceInfo(&mut target.header) == 0,
                )
            };
            let tech = if tgt_ok {
                target.outputTechnology
            } else {
                p.targetInfo.outputTechnology
            };
            PathInfo {
                gdi: if src_ok {
                    super::from_wide(&source.viewGdiDeviceName)
                } else {
                    String::new()
                },
                friendly: if tgt_ok {
                    super::from_wide(&target.monitorFriendlyDeviceName)
                } else {
                    String::new()
                },
                device_path: if tgt_ok {
                    super::from_wide(&target.monitorDevicePath)
                } else {
                    String::new()
                },
                internal: tech == DISPLAYCONFIG_OUTPUT_TECHNOLOGY_INTERNAL
                    || tech == DISPLAYCONFIG_OUTPUT_TECHNOLOGY_DISPLAYPORT_EMBEDDED
                    || tech == DISPLAYCONFIG_OUTPUT_TECHNOLOGY_UDI_EMBEDDED,
                adapter: p.targetInfo.adapterId,
                target: p.targetInfo.id,
            }
        })
        .collect()
}

/// HDR status and SDR white level of a path.
fn hdr_state(path: &PathInfo) -> HdrState {
    let mut info = DISPLAYCONFIG_GET_ADVANCED_COLOR_INFO {
        header: header(
            DISPLAYCONFIG_DEVICE_INFO_GET_ADVANCED_COLOR_INFO,
            std::mem::size_of::<DISPLAYCONFIG_GET_ADVANCED_COLOR_INFO>(),
            path.adapter,
            path.target,
        ),
        ..Default::default()
    };
    // SAFETY: the packet starts with a header giving its own type and size.
    if unsafe { DisplayConfigGetDeviceInfo(&mut info.header) } != 0 {
        return HdrState::default();
    }
    // SAFETY: the union is a plain bit field; reading it as u32 is always valid.
    let bits = unsafe { info.Anonymous.value };
    let supported = bits & 0x1 != 0;
    let enabled = bits & 0x2 != 0;
    let mut sdr_white_nits = None;
    if enabled {
        let mut white = DISPLAYCONFIG_SDR_WHITE_LEVEL {
            header: header(
                DISPLAYCONFIG_DEVICE_INFO_GET_SDR_WHITE_LEVEL,
                std::mem::size_of::<DISPLAYCONFIG_SDR_WHITE_LEVEL>(),
                path.adapter,
                path.target,
            ),
            ..Default::default()
        };
        // SAFETY: as above.
        if unsafe { DisplayConfigGetDeviceInfo(&mut white.header) } == 0 {
            sdr_white_nits = Some(sdr_nits(white.SDRWhiteLevel));
        }
    }
    HdrState {
        supported,
        enabled,
        sdr_white_nits,
    }
}

/// SDR white level as DisplayConfig reports it (1000 = 80 nits) in nits.
pub fn sdr_nits(level: u32) -> f32 {
    level as f32 / 1000.0 * 80.0
}

pub fn orientation_from_dmdo(v: u32) -> Orientation {
    match v {
        1 => Orientation::Portrait,
        2 => Orientation::LandscapeFlipped,
        3 => Orientation::PortraitFlipped,
        _ => Orientation::Landscape,
    }
}

pub fn dmdo_from_orientation(o: Orientation) -> u32 {
    match o {
        Orientation::Landscape => 0,
        Orientation::Portrait => 1,
        Orientation::LandscapeFlipped => 2,
        Orientation::PortraitFlipped => 3,
    }
}

fn is_portrait(o: Orientation) -> bool {
    matches!(o, Orientation::Portrait | Orientation::PortraitFlipped)
}

/// The desktop size after turning a display from one orientation to another:
/// width and height swap when going between portrait and landscape.
pub fn oriented_size(w: u32, h: u32, from: Orientation, to: Orientation) -> (u32, u32) {
    if is_portrait(from) != is_portrait(to) {
        (h, w)
    } else {
        (w, h)
    }
}

/// Removes duplicates and keeps 32-bit modes (all modes when none is 32-bit),
/// sorted largest first.
pub fn tidy_modes(mut modes: Vec<DisplayMode>) -> Vec<DisplayMode> {
    if modes.iter().any(|m| m.bits == 32) {
        modes.retain(|m| m.bits == 32);
    }
    modes.retain(|m| m.width > 0 && m.height > 0);
    modes.sort_by(|a, b| b.cmp(a));
    modes.dedup();
    modes
}

/// New desktop positions when `new_primary` becomes the primary display:
/// it moves to (0, 0) and every other display shifts by the same amount.
pub fn primary_layout(displays: &[(String, PxRect)], new_primary: &str) -> Vec<(String, i32, i32)> {
    let (dx, dy) = displays
        .iter()
        .find(|(d, _)| d == new_primary)
        .map(|(_, r)| (r.x, r.y))
        .unwrap_or((0, 0));
    displays
        .iter()
        .map(|(d, r)| (d.clone(), r.x - dx, r.y - dy))
        .collect()
}

/// What to apply to undo `change` on a display now in `state`. For a new
/// primary display, `make_primary` is set: the caller applies it to the
/// display that was primary before.
pub fn before_change(state: &DisplayState, change: &DisplayChange) -> DisplayChange {
    DisplayChange {
        mode: change.mode.map(|_| state.current),
        orientation: change.orientation.map(|_| state.orientation),
        make_primary: change.make_primary && !state.primary,
    }
}

fn read_mode(device: &[u16], index: ENUM_DISPLAY_SETTINGS_MODE) -> Option<DEVMODEW> {
    let mut dm = DEVMODEW {
        dmSize: std::mem::size_of::<DEVMODEW>() as u16,
        ..Default::default()
    };
    // SAFETY: `device` is NUL-terminated; `dm` is sized via dmSize.
    let ok = unsafe {
        EnumDisplaySettingsExW(
            PCWSTR(device.as_ptr()),
            index,
            &mut dm,
            ENUM_DISPLAY_SETTINGS_FLAGS(0),
        )
    };
    ok.as_bool().then_some(dm)
}

fn dm_orientation(dm: &DEVMODEW) -> Orientation {
    // SAFETY: display devices fill the display variant of this union.
    orientation_from_dmdo(unsafe { dm.Anonymous1.Anonymous2.dmDisplayOrientation.0 })
}

fn dm_position(dm: &DEVMODEW) -> POINTL {
    // SAFETY: as above.
    unsafe { dm.Anonymous1.Anonymous2.dmPosition }
}

fn dm_mode(dm: &DEVMODEW) -> DisplayMode {
    DisplayMode {
        width: dm.dmPelsWidth,
        height: dm.dmPelsHeight,
        refresh: dm.dmDisplayFrequency,
        bits: dm.dmBitsPerPel,
    }
}

/// The GDI display devices attached to the desktop: (name, adapter description, primary).
fn attached_devices() -> Vec<(String, String, bool)> {
    let mut out = Vec::new();
    for i in 0..32 {
        let mut dd = DISPLAY_DEVICEW {
            cb: std::mem::size_of::<DISPLAY_DEVICEW>() as u32,
            ..Default::default()
        };
        // SAFETY: `dd` is sized via cb.
        if !unsafe { EnumDisplayDevicesW(PCWSTR::null(), i, &mut dd, 0) }.as_bool() {
            break;
        }
        if dd.StateFlags & DISPLAY_DEVICE_ACTIVE != 0 {
            out.push((
                super::from_wide(&dd.DeviceName),
                super::from_wide(&dd.DeviceString),
                dd.StateFlags & DISPLAY_DEVICE_PRIMARY_DEVICE != 0,
            ));
        }
    }
    out
}

fn scale_percent(hmonitor: Option<isize>) -> u32 {
    let Some(h) = hmonitor else { return 100 };
    let (mut x, mut y) = (0u32, 0u32);
    // SAFETY: valid out pointers; a stale handle only makes the call fail.
    let ok = unsafe {
        GetDpiForMonitor(
            windows::Win32::Graphics::Gdi::HMONITOR(h as *mut _),
            MDT_EFFECTIVE_DPI,
            &mut x,
            &mut y,
        )
    };
    if ok.is_ok() && x > 0 {
        (x * 100 + 48) / 96
    } else {
        100
    }
}

fn change_error(code: DISP_CHANGE) -> HwError {
    if code == DISP_CHANGE_RESTART {
        HwError("Windows needs a restart to use this setting.".into())
    } else {
        HwError("The display did not accept this setting. Choose another resolution, refresh rate or orientation.".into())
    }
}

fn change(device: Option<&[u16]>, dm: Option<&DEVMODEW>, flags: CDS_TYPE) -> DISP_CHANGE {
    // SAFETY: `device` is NUL-terminated when given and `dm` outlives the call.
    unsafe {
        ChangeDisplaySettingsExW(
            device.map(|d| PCWSTR(d.as_ptr())).unwrap_or(PCWSTR::null()),
            dm.map(|d| d as *const DEVMODEW),
            HWND::default(),
            flags,
            None,
        )
    }
}

/// Reads and changes the display settings of this PC.
#[derive(Default)]
pub struct WinDisplays {}

impl WinDisplays {
    pub fn new() -> Self {
        WinDisplays {}
    }
}

impl DisplayConfig for WinDisplays {
    fn displays(&mut self) -> Vec<DisplayState> {
        let paths = paths();
        let monitors = super::monitors();
        let mut out = Vec::new();
        for (name, adapter, primary) in attached_devices() {
            let wide = super::wide(&name);
            let Some(cur) = read_mode(&wide, ENUM_CURRENT_SETTINGS) else {
                continue;
            };
            let orientation = dm_orientation(&cur);
            let mut modes = Vec::new();
            for i in 0..2000u32 {
                let Some(dm) = read_mode(&wide, ENUM_DISPLAY_SETTINGS_MODE(i)) else {
                    break;
                };
                let mut m = dm_mode(&dm);
                (m.width, m.height) =
                    oriented_size(m.width, m.height, dm_orientation(&dm), orientation);
                modes.push(m);
            }
            let current = dm_mode(&cur);
            if !modes.contains(&current) {
                modes.push(current);
            }
            let pos = dm_position(&cur);
            let path = paths.iter().find(|p| p.gdi.eq_ignore_ascii_case(&name));
            let monitor = monitors
                .iter()
                .find(|m| m.device.eq_ignore_ascii_case(&name));
            let display_name = path
                .map(|p| p.friendly.clone())
                .filter(|f| !f.is_empty())
                .unwrap_or_else(|| {
                    if adapter.is_empty() {
                        name.clone()
                    } else {
                        adapter.clone()
                    }
                });
            out.push(DisplayState {
                device: name.clone(),
                name: display_name,
                current,
                modes: tidy_modes(modes),
                orientation,
                primary: primary || monitor.is_some_and(|m| m.primary),
                rect: PxRect::new(pos.x, pos.y, current.width as i32, current.height as i32),
                scale_percent: scale_percent(monitor.map(|m| m.handle)),
                hdr: path.map(hdr_state).unwrap_or_default(),
            });
        }
        out
    }

    fn apply(
        &mut self,
        device: &str,
        change_req: &DisplayChange,
    ) -> Result<DisplayChange, HwError> {
        let all = self.displays();
        let state = all
            .iter()
            .find(|d| d.device.eq_ignore_ascii_case(device))
            .ok_or_else(|| HwError("That display is not connected any more.".into()))?;
        let before = before_change(state, change_req);
        if change_req.is_empty() {
            return Ok(before);
        }
        let wide = super::wide(&state.device);
        let mut dm = read_mode(&wide, ENUM_CURRENT_SETTINGS)
            .ok_or_else(|| HwError("Could not read this display's current settings.".into()))?;
        dm.dmFields = Default::default();
        let mut target_size = None;
        if let Some(mode) = change_req.mode {
            dm.dmPelsWidth = mode.width;
            dm.dmPelsHeight = mode.height;
            dm.dmDisplayFrequency = mode.refresh;
            dm.dmBitsPerPel = mode.bits;
            dm.dmFields |= DM_PELSWIDTH | DM_PELSHEIGHT | DM_DISPLAYFREQUENCY | DM_BITSPERPEL;
            target_size = Some((mode.width, mode.height));
        }
        if let Some(o) = change_req.orientation {
            let (w, h) = target_size.unwrap_or((state.current.width, state.current.height));
            let (w, h) = oriented_size(w, h, state.orientation, o);
            dm.dmPelsWidth = w;
            dm.dmPelsHeight = h;
            // Display devices use the display variant of this union.
            dm.Anonymous1.Anonymous2.dmDisplayOrientation =
                DEVMODE_DISPLAY_ORIENTATION(dmdo_from_orientation(o));
            dm.dmFields |= DM_DISPLAYORIENTATION | DM_PELSWIDTH | DM_PELSHEIGHT;
        }

        // Test first: nothing changes if the driver says no.
        if dm.dmFields.0 != 0 {
            let r = change(Some(&wide), Some(&dm), CDS_TEST);
            if r != DISP_CHANGE_SUCCESSFUL {
                return Err(change_error(r));
            }
        }

        let stage = CDS_UPDATEREGISTRY | CDS_NORESET;
        if before.make_primary {
            let rects: Vec<(String, PxRect)> =
                all.iter().map(|d| (d.device.clone(), d.rect)).collect();
            for (dev, x, y) in primary_layout(&rects, &state.device) {
                let is_target = dev.eq_ignore_ascii_case(&state.device);
                let dev_wide = super::wide(&dev);
                let mut d = if is_target {
                    dm
                } else {
                    let Some(mut d) = read_mode(&dev_wide, ENUM_CURRENT_SETTINGS) else {
                        continue;
                    };
                    d.dmFields = Default::default();
                    d
                };
                // Display devices use the display variant of this union.
                d.Anonymous1.Anonymous2.dmPosition = POINTL { x, y };
                d.dmFields |= DM_POSITION;
                let flags = if is_target {
                    stage | CDS_SET_PRIMARY
                } else {
                    stage
                };
                let r = change(Some(&dev_wide), Some(&d), flags);
                if r != DISP_CHANGE_SUCCESSFUL {
                    return Err(change_error(r));
                }
            }
        } else if dm.dmFields.0 != 0 {
            let r = change(Some(&wide), Some(&dm), stage);
            if r != DISP_CHANGE_SUCCESSFUL {
                return Err(change_error(r));
            }
        }
        // Commit everything staged above in one go.
        let r = change(None, None, CDS_TYPE(0));
        if r != DISP_CHANGE_SUCCESSFUL {
            return Err(change_error(r));
        }
        Ok(before)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn mode(w: u32, h: u32, hz: u32, bits: u32) -> DisplayMode {
        DisplayMode {
            width: w,
            height: h,
            refresh: hz,
            bits,
        }
    }

    #[test]
    fn modes_are_deduped_and_sorted() {
        let m = tidy_modes(vec![
            mode(1920, 1080, 60, 32),
            mode(2560, 1440, 144, 32),
            mode(1920, 1080, 60, 32),
            mode(1920, 1080, 60, 16),
            mode(2560, 1440, 60, 32),
            mode(0, 0, 0, 32),
        ]);
        assert_eq!(
            m,
            vec![
                mode(2560, 1440, 144, 32),
                mode(2560, 1440, 60, 32),
                mode(1920, 1080, 60, 32)
            ]
        );
        // No 32-bit modes at all: keep what there is.
        assert_eq!(tidy_modes(vec![mode(800, 600, 60, 16)]).len(), 1);
    }

    #[test]
    fn orientation_swaps_size() {
        use Orientation::*;
        assert_eq!(oriented_size(1920, 1080, Landscape, Portrait), (1080, 1920));
        assert_eq!(
            oriented_size(1080, 1920, PortraitFlipped, Landscape),
            (1920, 1080)
        );
        assert_eq!(
            oriented_size(1920, 1080, Landscape, LandscapeFlipped),
            (1920, 1080)
        );
        assert_eq!(
            oriented_size(1080, 1920, Portrait, PortraitFlipped),
            (1080, 1920)
        );
        for o in Orientation::ALL {
            assert_eq!(orientation_from_dmdo(dmdo_from_orientation(o)), o);
        }
    }

    #[test]
    fn new_primary_moves_to_origin() {
        let d = vec![
            ("A".to_string(), PxRect::new(0, 0, 1920, 1080)),
            ("B".to_string(), PxRect::new(1920, -200, 2560, 1440)),
            ("C".to_string(), PxRect::new(-1080, 0, 1080, 1920)),
        ];
        let l = primary_layout(&d, "B");
        assert_eq!(
            l,
            vec![
                ("A".to_string(), -1920, 200),
                ("B".to_string(), 0, 0),
                ("C".to_string(), -3000, 200)
            ]
        );
        // Unknown device: nothing moves.
        assert_eq!(primary_layout(&d, "Z")[1], ("B".to_string(), 1920, -200));
    }

    #[test]
    fn before_change_records_what_changes() {
        let state = crate::mock::MockDisplays::display(
            "D2",
            "Two",
            (2560, 1440),
            &[144, 60],
            1920,
            false,
            100,
        );
        let change = DisplayChange {
            mode: Some(mode(1920, 1080, 60, 32)),
            orientation: Some(Orientation::Portrait),
            make_primary: true,
        };
        let b = before_change(&state, &change);
        assert_eq!(b.mode, Some(state.current));
        assert_eq!(b.orientation, Some(Orientation::Landscape));
        assert!(b.make_primary);
        let b = before_change(&state, &DisplayChange::default());
        assert!(b.is_empty());
        let mut primary = state.clone();
        primary.primary = true;
        let only_primary = DisplayChange {
            make_primary: true,
            ..Default::default()
        };
        assert!(before_change(&primary, &only_primary).is_empty());
    }

    #[test]
    fn sdr_white_level_in_nits() {
        assert_eq!(sdr_nits(1000), 80.0);
        assert_eq!(sdr_nits(2500), 200.0);
    }
}
