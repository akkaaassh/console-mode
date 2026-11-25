//! Window helpers: foreground, focus, closing, full-screen checks.

use crate::util::from_wide;
use std::mem::{size_of, zeroed};
use windows_sys::Win32::Foundation::{CloseHandle, HWND, RECT};
use windows_sys::Win32::Graphics::Gdi::{GetMonitorInfoW, MonitorFromWindow, MONITORINFO, MONITOR_DEFAULTTONEAREST};
use windows_sys::Win32::System::Threading::{
    OpenProcess, QueryFullProcessImageNameW, PROCESS_QUERY_LIMITED_INFORMATION,
};
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

/// Shows a window without taking focus away from the current app.
pub fn show_no_activate(h: isize) {
    if exists(h) {
        unsafe {
            ShowWindow(hwnd(h), SW_SHOWNOACTIVATE);
            SetWindowPos(hwnd(h), HWND_TOPMOST, 0, 0, 0, 0, SWP_NOMOVE | SWP_NOSIZE | SWP_NOACTIVATE);
        }
    }
}

pub fn hide(h: isize) {
    if exists(h) {
        unsafe {
            ShowWindow(hwnd(h), SW_HIDE);
        }
    }
}

pub fn is_visible(h: isize) -> bool {
    exists(h) && unsafe { IsWindowVisible(hwnd(h)) != 0 }
}

/// Makes a window never take focus when shown or clicked (for the on-screen keyboard).
pub fn make_no_activate(h: isize) {
    if exists(h) {
        unsafe {
            let ex = GetWindowLongPtrW(hwnd(h), GWL_EXSTYLE);
            SetWindowLongPtrW(
                hwnd(h),
                GWL_EXSTYLE,
                ex | (WS_EX_NOACTIVATE | WS_EX_TOOLWINDOW | WS_EX_TOPMOST) as isize,
            );
        }
    }
}

/// Lower-case file name of the process that owns a window, e.g. "chrome.exe".
pub fn process_name(h: isize) -> String {
    if !exists(h) {
        return String::new();
    }
    unsafe {
        let mut pid = 0u32;
        GetWindowThreadProcessId(hwnd(h), &mut pid);
        process_name_of(pid)
    }
}

pub fn process_name_of(pid: u32) -> String {
    if pid == 0 {
        return String::new();
    }
    unsafe {
        let ph = OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, 0, pid);
        if ph.is_null() {
            return String::new();
        }
        let mut buf = [0u16; 1024];
        let mut len = buf.len() as u32;
        let ok = QueryFullProcessImageNameW(ph, 0, buf.as_mut_ptr(), &mut len) != 0;
        CloseHandle(ph);
        if !ok {
            return String::new();
        }
        let full = from_wide(&buf[..len as usize]);
        full.rsplit('\\').next().unwrap_or("").to_lowercase()
    }
}

/// True when the window covers its whole monitor (full-screen or borderless games).
pub fn is_fullscreen(h: isize) -> bool {
    if !exists(h) {
        return false;
    }
    unsafe {
        let mut r: RECT = zeroed();
        if GetWindowRect(hwnd(h), &mut r) == 0 {
            return false;
        }
        let mon = MonitorFromWindow(hwnd(h), MONITOR_DEFAULTTONEAREST);
        let mut mi: MONITORINFO = zeroed();
        mi.cbSize = size_of::<MONITORINFO>() as u32;
        if GetMonitorInfoW(mon, &mut mi) == 0 {
            return false;
        }
        let m = mi.rcMonitor;
        r.left <= m.left && r.top <= m.top && r.right >= m.right && r.bottom >= m.bottom
    }
}

/// Size and position of the monitor that holds the given window (or the primary one).
pub fn monitor_rect(h: isize) -> (i32, i32, i32, i32) {
    unsafe {
        let mon = MonitorFromWindow(hwnd(h), MONITOR_DEFAULTTONEAREST);
        let mut mi: MONITORINFO = zeroed();
        mi.cbSize = size_of::<MONITORINFO>() as u32;
        if GetMonitorInfoW(mon, &mut mi) == 0 {
            return (0, 0, 1920, 1080);
        }
        let m = mi.rcMonitor;
        (m.left, m.top, m.right - m.left, m.bottom - m.top)
    }
}

/// The desktop itself, the taskbar and other shell windows.
pub fn is_shell(h: isize) -> bool {
    if h == 0 {
        return true;
    }
    unsafe {
        let shell = GetShellWindow() as isize;
        let desk = GetDesktopWindow() as isize;
        if h == shell || h == desk {
            return true;
        }
        let mut cls = [0u16; 128];
        let n = GetClassNameW(hwnd(h), cls.as_mut_ptr(), cls.len() as i32);
        let c = from_wide(&cls[..n.max(0) as usize]);
        matches!(c.as_str(), "Progman" | "WorkerW" | "Shell_TrayWnd" | "Shell_SecondaryTrayWnd")
    }
}
