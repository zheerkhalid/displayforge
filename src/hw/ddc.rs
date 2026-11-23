//! DDC/CI through dxva2 (physical monitor handles, VCP get/set, capabilities).
//!
//! Handles are kept as `isize` so that the backend that owns them stays
//! `Send`; dxva2 handles may be used from any thread.
use std::time::Duration;
use windows::Win32::Devices::Display::{
    CapabilitiesRequestAndCapabilitiesReply, DestroyPhysicalMonitors, GetCapabilitiesStringLength,
    GetNumberOfPhysicalMonitorsFromHMONITOR, GetPhysicalMonitorsFromHMONITOR,
    GetVCPFeatureAndVCPFeatureReply, SetVCPFeature, PHYSICAL_MONITOR,
};
use windows::Win32::Foundation::HANDLE;
use windows::Win32::Graphics::Gdi::HMONITOR;

/// Pause before the one retry of a failed DDC/CI request.
const RETRY_PAUSE: Duration = Duration::from_millis(40);

/// A physical monitor handle and the description Windows gives it.
#[derive(Clone, Debug)]
pub struct PhysicalMonitor {
    pub handle: isize,
    pub description: String,
}

fn handle(h: isize) -> HANDLE {
    HANDLE(h as *mut core::ffi::c_void)
}

/// The physical monitors behind one HMONITOR (usually one; more when
/// displays are duplicated). Their handles must be released with [`destroy`].
pub fn physical_monitors(hmonitor: isize) -> Vec<PhysicalMonitor> {
    let hmon = HMONITOR(hmonitor as *mut core::ffi::c_void);
    let mut count = 0u32;
    // SAFETY: `count` is a valid out pointer; a stale HMONITOR only makes the call fail.
    if unsafe { GetNumberOfPhysicalMonitorsFromHMONITOR(hmon, &mut count) }.is_err() || count == 0 {
        return Vec::new();
    }
    let mut list = vec![PHYSICAL_MONITOR::default(); count.min(16) as usize];
    // SAFETY: the slice is sized for the count the API asked for.
    if unsafe { GetPhysicalMonitorsFromHMONITOR(hmon, &mut list) }.is_err() {
        return Vec::new();
    }
    list.iter()
        .map(|p| {
            // The struct is packed: copy the fields out before borrowing them.
            let h = p.hPhysicalMonitor;
            let description = p.szPhysicalMonitorDescription;
            PhysicalMonitor {
                handle: h.0 as isize,
                description: super::from_wide(&description),
            }
        })
        .collect()
}

/// Releases physical monitor handles from [`physical_monitors`].
pub fn destroy(handles: &[isize]) {
    if handles.is_empty() {
        return;
    }
    let list: Vec<PHYSICAL_MONITOR> = handles
        .iter()
        .map(|&h| PHYSICAL_MONITOR {
            hPhysicalMonitor: handle(h),
            szPhysicalMonitorDescription: [0; 128],
        })
        .collect();
    // SAFETY: each handle came from GetPhysicalMonitorsFromHMONITOR and is released once.
    unsafe {
        let _ = DestroyPhysicalMonitors(&list);
    }
}

/// Reads the MCCS capabilities string (slow: up to a second or two).
pub fn capabilities_string(h: isize) -> Option<String> {
    let attempt = || -> Option<String> {
        let mut len = 0u32;
        // SAFETY: `len` is a valid out pointer.
        if unsafe { GetCapabilitiesStringLength(handle(h), &mut len) } == 0 || len == 0 {
            return None;
        }
        let mut buf = vec![0u8; len.min(64 * 1024) as usize];
        // SAFETY: the buffer is as long as the length passed with it.
        if unsafe { CapabilitiesRequestAndCapabilitiesReply(handle(h), &mut buf) } == 0 {
            return None;
        }
        let end = buf.iter().position(|&b| b == 0).unwrap_or(buf.len());
        Some(String::from_utf8_lossy(&buf[..end]).into_owned())
    };
    attempt().or_else(|| {
        std::thread::sleep(RETRY_PAUSE);
        attempt()
    })
}

/// Reads a VCP code: (current value, maximum). One retry after a short pause.
pub fn get_vcp(h: isize, code: u8) -> Option<(u32, u32)> {
    let attempt = || {
        let (mut current, mut max) = (0u32, 0u32);
        // SAFETY: the out pointers are valid locals.
        let ok = unsafe {
            GetVCPFeatureAndVCPFeatureReply(handle(h), code, None, &mut current, Some(&mut max))
        };
        (ok != 0).then_some((current, max))
    };
    attempt().or_else(|| {
        std::thread::sleep(RETRY_PAUSE);
        attempt()
    })
}

/// Writes a VCP code. One retry after a short pause.
pub fn set_vcp(h: isize, code: u8, value: u32) -> bool {
    // SAFETY: plain call with a handle from GetPhysicalMonitorsFromHMONITOR.
    let attempt = || unsafe { SetVCPFeature(handle(h), code, value) } != 0;
    attempt() || {
        std::thread::sleep(RETRY_PAUSE);
        attempt()
    }
}
