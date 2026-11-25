//! Resolution and refresh rate of the main display.

use serde::Serialize;
use std::mem::{size_of, zeroed};
use std::ptr::{null, null_mut};
use windows_sys::Win32::Graphics::Gdi::*;

#[derive(Serialize, Clone, Copy, Debug, PartialEq, Eq)]
pub struct Mode {
    pub w: u32,
    pub h: u32,
    pub hz: u32,
}

#[derive(Serialize, Debug)]
pub struct Modes {
    pub current: Option<Mode>,
    pub all: Vec<Mode>,
}

fn blank() -> DEVMODEW {
    let mut dm: DEVMODEW = unsafe { zeroed() };
    dm.dmSize = size_of::<DEVMODEW>() as u16;
    dm
}

pub fn modes() -> Modes {
    let mut dm = blank();
    let current = if unsafe { EnumDisplaySettingsW(null(), ENUM_CURRENT_SETTINGS, &mut dm) } != 0 {
        Some(Mode { w: dm.dmPelsWidth, h: dm.dmPelsHeight, hz: dm.dmDisplayFrequency })
    } else {
        None
    };
    let mut all: Vec<Mode> = Vec::new();
    let mut i = 0u32;
    loop {
        let mut dm = blank();
        if unsafe { EnumDisplaySettingsW(null(), i, &mut dm) } == 0 {
            break;
        }
        i += 1;
        if dm.dmBitsPerPel != 32 || dm.dmPelsWidth < 1280 {
            continue;
        }
        let m = Mode { w: dm.dmPelsWidth, h: dm.dmPelsHeight, hz: dm.dmDisplayFrequency };
        if !all.contains(&m) {
            all.push(m);
        }
    }
    all.sort_by(|a, b| (b.w * b.h, b.hz).cmp(&(a.w * a.h, a.hz)));
    Modes { current, all }
}

/// Switches the main display, keeping the change after restart.
pub fn set(w: u32, h: u32, hz: u32) -> bool {
    let mut dm = blank();
    dm.dmPelsWidth = w;
    dm.dmPelsHeight = h;
    dm.dmDisplayFrequency = hz;
    dm.dmFields = DM_PELSWIDTH | DM_PELSHEIGHT | DM_DISPLAYFREQUENCY;
    let r = unsafe { ChangeDisplaySettingsExW(null(), &dm, null_mut(), CDS_UPDATEREGISTRY, null()) };
    r == DISP_CHANGE_SUCCESSFUL
}
