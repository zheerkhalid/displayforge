//! The foreground app (executable name, full-screen state, position) and the
//! list of apps with visible windows. Read-only queries.

use crate::model::PxRect;
use windows::core::PWSTR;
use windows::Win32::Foundation::{CloseHandle, BOOL, HWND, LPARAM, RECT};
use windows::Win32::Graphics::Gdi::{
    GetMonitorInfoW, MonitorFromWindow, MONITORINFO, MONITOR_DEFAULTTONEAREST,
};
use windows::Win32::System::Threading::{
    OpenProcess, QueryFullProcessImageNameW, PROCESS_NAME_WIN32, PROCESS_QUERY_LIMITED_INFORMATION,
};
use windows::Win32::UI::WindowsAndMessaging::{
    EnumWindows, GetClassNameW, GetForegroundWindow, GetWindow, GetWindowLongW, GetWindowRect,
    GetWindowTextLengthW, GetWindowThreadProcessId, IsIconic, IsWindowVisible, GWL_EXSTYLE,
    GW_OWNER, WS_EX_TOOLWINDOW,
};

/// The window in front.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Front {
    /// Executable file name, lower case.
    pub exe: String,
    pub fullscreen: bool,
    /// The window's rectangle, physical pixels.
    pub rect: PxRect,
}

fn class_name(hwnd: HWND) -> String {
    let mut buf = [0u16; 128];
    // SAFETY: the buffer outlives the call.
    let n = unsafe { GetClassNameW(hwnd, &mut buf) };
    String::from_utf16_lossy(&buf[..n.max(0) as usize])
}

fn exe_of(hwnd: HWND) -> Option<String> {
    let mut pid = 0u32;
    // SAFETY: plain queries; the process handle is closed before returning.
    unsafe {
        GetWindowThreadProcessId(hwnd, Some(&mut pid));
        if pid == 0 {
            return None;
        }
        let process = OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, false, pid).ok()?;
        let mut buf = [0u16; 1024];
        let mut len = buf.len() as u32;
        let ok = QueryFullProcessImageNameW(
            process,
            PROCESS_NAME_WIN32,
            PWSTR(buf.as_mut_ptr()),
            &mut len,
        );
        let _ = CloseHandle(process);
        ok.ok()?;
        let path = String::from_utf16_lossy(&buf[..len as usize]);
        Some(file_name_lower(&path))
    }
}

/// "C:\Program Files\App\App.EXE" -> "app.exe".
pub fn file_name_lower(path: &str) -> String {
    path.rsplit(['\\', '/'])
        .next()
        .unwrap_or(path)
        .to_lowercase()
}

/// Shell windows that are never "an app in front".
fn is_shell(class: &str) -> bool {
    matches!(
        class,
        "Progman"
            | "WorkerW"
            | "Shell_TrayWnd"
            | "Shell_SecondaryTrayWnd"
            | "NotifyIconOverflowWindow"
    )
}

/// Whether a window rectangle covers a whole monitor.
pub fn covers(window: PxRect, monitor: PxRect) -> bool {
    window.x <= monitor.x
        && window.y <= monitor.y
        && window.right() >= monitor.right()
        && window.bottom() >= monitor.bottom()
}

/// The foreground window's app, or `None` for the desktop, the taskbar or
/// windows whose process cannot be queried (e.g. elevated ones).
pub fn current() -> Option<Front> {
    // SAFETY: plain queries into structures owned by this function.
    unsafe {
        let hwnd = GetForegroundWindow();
        if hwnd.0.is_null() || is_shell(&class_name(hwnd)) {
            return None;
        }
        let mut r = RECT::default();
        GetWindowRect(hwnd, &mut r).ok()?;
        let rect = PxRect::new(r.left, r.top, r.right - r.left, r.bottom - r.top);
        let mon = MonitorFromWindow(hwnd, MONITOR_DEFAULTTONEAREST);
        let mut info = MONITORINFO {
            cbSize: std::mem::size_of::<MONITORINFO>() as u32,
            ..Default::default()
        };
        let fullscreen = GetMonitorInfoW(mon, &mut info).as_bool() && {
            let m = info.rcMonitor;
            covers(
                rect,
                PxRect::new(m.left, m.top, m.right - m.left, m.bottom - m.top),
            )
        };
        let exe = exe_of(hwnd)?;
        Some(Front {
            exe,
            fullscreen,
            rect,
        })
    }
}

unsafe extern "system" fn collect(hwnd: HWND, lparam: LPARAM) -> BOOL {
    // SAFETY: lparam is the Vec passed by `running_apps`, alive during EnumWindows.
    let list = &mut *(lparam.0 as *mut Vec<HWND>);
    if IsWindowVisible(hwnd).as_bool()
        && !IsIconic(hwnd).as_bool()
        && GetWindowTextLengthW(hwnd) > 0
        && GetWindow(hwnd, GW_OWNER)
            .map(|o| o.0.is_null())
            .unwrap_or(true)
        && (GetWindowLongW(hwnd, GWL_EXSTYLE) as u32 & WS_EX_TOOLWINDOW.0) == 0
    {
        list.push(hwnd);
    }
    BOOL(1)
}

/// Executable names of apps with visible top-level windows, sorted, without
/// duplicates and without DisplayForge itself.
pub fn running_apps() -> Vec<String> {
    let mut windows: Vec<HWND> = Vec::new();
    // SAFETY: the callback only runs during this call, while `windows` lives.
    unsafe {
        let _ = EnumWindows(
            Some(collect),
            LPARAM(&mut windows as *mut Vec<HWND> as isize),
        );
    }
    let own = std::env::current_exe()
        .map(|p| file_name_lower(&p.to_string_lossy()))
        .unwrap_or_default();
    let mut names: Vec<String> = windows
        .into_iter()
        .filter(|h| !is_shell(&class_name(*h)))
        .filter_map(exe_of)
        .filter(|n| *n != own && n != "applicationframehost.exe" && n != "textinputhost.exe")
        .collect();
    names.sort();
    names.dedup();
    names
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn names_and_coverage() {
        assert_eq!(file_name_lower("C:\\Apps\\VLC\\VLC.EXE"), "vlc.exe");
        assert_eq!(file_name_lower("game.exe"), "game.exe");
        let m = PxRect::new(1920, 0, 2560, 1440);
        assert!(covers(PxRect::new(1920, 0, 2560, 1440), m));
        assert!(covers(PxRect::new(1912, -8, 2576, 1456), m));
        assert!(!covers(PxRect::new(1920, 0, 2560, 1400), m));
        assert!(is_shell("Progman") && !is_shell("Chrome_WidgetWin_1"));
    }

    #[test]
    fn queries_do_not_fail() {
        // Read-only; the result depends on the desktop, so only check it runs.
        let _ = current();
        let apps = running_apps();
        assert!(apps.windows(2).all(|w| w[0] < w[1]));
    }
}
