//! A dark, click-through, topmost window over one monitor (dimming below zero).
use crate::model::PxRect;
use std::sync::OnceLock;
use windows::core::{w, PCWSTR};
use windows::Win32::Foundation::{COLORREF, HWND, LPARAM, LRESULT, WPARAM};
use windows::Win32::Graphics::Gdi::{GetStockObject, BLACK_BRUSH, HBRUSH};
use windows::Win32::System::LibraryLoader::GetModuleHandleW;
use windows::Win32::UI::WindowsAndMessaging::{
    CreateWindowExW, DefWindowProcW, DestroyWindow, RegisterClassW, SetLayeredWindowAttributes,
    SetWindowPos, ShowWindow, HWND_TOPMOST, LWA_ALPHA, SWP_NOACTIVATE, SWP_SHOWWINDOW, SW_HIDE,
    WNDCLASSW, WS_EX_LAYERED, WS_EX_NOACTIVATE, WS_EX_TOOLWINDOW, WS_EX_TOPMOST, WS_EX_TRANSPARENT,
    WS_POPUP,
};

const CLASS_NAME: PCWSTR = w!("DisplayForgeDimOverlay");

pub struct Overlay {
    hwnd: HWND,
    rect: PxRect,
    alpha: u8,
}

unsafe extern "system" fn wnd_proc(hwnd: HWND, msg: u32, wp: WPARAM, lp: LPARAM) -> LRESULT {
    // SAFETY: forwarded unchanged to the default window procedure.
    unsafe { DefWindowProcW(hwnd, msg, wp, lp) }
}

/// Registers the window class once per process.
fn register_class() -> bool {
    static REGISTERED: OnceLock<bool> = OnceLock::new();
    *REGISTERED.get_or_init(|| {
        // SAFETY: the class name is static and the window procedure lives for
        // the whole program.
        unsafe {
            let Ok(instance) = GetModuleHandleW(None) else {
                return false;
            };
            let class = WNDCLASSW {
                lpfnWndProc: Some(wnd_proc),
                hInstance: instance.into(),
                lpszClassName: CLASS_NAME,
                hbrBackground: HBRUSH(GetStockObject(BLACK_BRUSH).0),
                ..Default::default()
            };
            RegisterClassW(&class) != 0
        }
    })
}

impl Overlay {
    /// Creates the overlay (hidden). Must be called on a thread with a message loop.
    pub fn new(rect: PxRect) -> Option<Overlay> {
        if !register_class() {
            return None;
        }
        // SAFETY: creates a window of the class registered above; all strings are static.
        let hwnd = unsafe {
            let instance = GetModuleHandleW(None).ok()?;
            CreateWindowExW(
                WS_EX_LAYERED
                    | WS_EX_TRANSPARENT
                    | WS_EX_TOPMOST
                    | WS_EX_TOOLWINDOW
                    | WS_EX_NOACTIVATE,
                CLASS_NAME,
                w!(""),
                WS_POPUP,
                rect.x,
                rect.y,
                rect.w,
                rect.h,
                HWND::default(),
                None,
                instance,
                None,
            )
            .ok()?
        };
        Some(Overlay {
            hwnd,
            rect,
            alpha: 0,
        })
    }

    /// 0 = invisible (hidden), 255 = black.
    pub fn set_alpha(&mut self, alpha: u8) {
        self.alpha = alpha;
        // SAFETY: the window belongs to this overlay and is alive until drop.
        unsafe {
            if alpha == 0 {
                let _ = ShowWindow(self.hwnd, SW_HIDE);
                return;
            }
            let _ = SetLayeredWindowAttributes(self.hwnd, COLORREF(0), alpha, LWA_ALPHA);
        }
        self.place(true);
    }

    pub fn set_rect(&mut self, rect: PxRect) {
        self.rect = rect;
        self.place(self.alpha > 0);
    }

    fn place(&self, show: bool) {
        let r = self.rect;
        let flags = if show {
            SWP_NOACTIVATE | SWP_SHOWWINDOW
        } else {
            SWP_NOACTIVATE
        };
        // SAFETY: the window belongs to this overlay and is alive until drop.
        unsafe {
            let _ = SetWindowPos(self.hwnd, HWND_TOPMOST, r.x, r.y, r.w, r.h, flags);
        }
    }
}

impl Drop for Overlay {
    fn drop(&mut self) {
        // SAFETY: the window was created by this overlay and is destroyed once.
        unsafe {
            let _ = DestroyWindow(self.hwnd);
        }
    }
}
