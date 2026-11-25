//! Foreground-window helpers.

use windows_sys::Win32::Foundation::HWND;
use windows_sys::Win32::UI::WindowsAndMessaging::*;

fn hwnd(h: isize) -> HWND {
    h as HWND
}

pub fn foreground() -> isize {
    unsafe { GetForegroundWindow() as isize }
}

pub fn exists(h: isize) -> bool {
    h != 0 && unsafe { IsWindow(hwnd(h)) != 0 }
}

/// Brings a window to the front, restoring it if minimised.
pub fn focus(h: isize) {
    if !exists(h) {
        return;
    }
    unsafe {
        if IsIconic(hwnd(h)) != 0 {
            ShowWindow(hwnd(h), SW_RESTORE);
        }
        crate::keys::alt_nudge();
        SetForegroundWindow(hwnd(h));
    }
}

/// Politely asks a window to close (same as clicking its X).
pub fn close(h: isize) -> bool {
    if !exists(h) {
        return false;
    }
    unsafe { PostMessageW(hwnd(h), WM_CLOSE, 0, 0) != 0 }
}
