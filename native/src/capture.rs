//! Pictures of windows and icons, as PNG data: URLs, for the app switcher and app tiles.

use crate::util::from_wide;
use crate::winrt::base64;
use serde::Serialize;
use std::mem::{size_of, zeroed};
use std::ptr::null_mut;
use windows_sys::Win32::Foundation::{HWND, LPARAM, RECT};
use windows_sys::Win32::Graphics::Dwm::{DwmGetWindowAttribute, DWMWA_CLOAKED};
use windows_sys::Win32::Graphics::Gdi::*;
use windows_sys::Win32::Storage::Xps::PrintWindow;
use windows_sys::Win32::UI::WindowsAndMessaging::*;

/// Encodes RGBA pixels as a PNG data: URL.
pub fn png_url(w: u32, h: u32, rgba: &[u8]) -> Option<String> {
    let mut out = Vec::new();
    {
        let mut enc = png::Encoder::new(&mut out, w, h);
        enc.set_color(png::ColorType::Rgba);
        enc.set_depth(png::BitDepth::Eight);
        let mut wr = enc.write_header().ok()?;
        wr.write_image_data(rgba).ok()?;
    }
    Some(format!("data:image/png;base64,{}", base64(&out)))
}

/// Reads a bitmap's pixels top-down as RGBA. `opaque` forces alpha to 255 (screenshots).
pub fn bitmap_rgba(hbm: HBITMAP, opaque: bool) -> Option<(u32, u32, Vec<u8>)> {
    unsafe {
        let mut bm: BITMAP = zeroed();
        if GetObjectW(hbm as HGDIOBJ, size_of::<BITMAP>() as i32, &mut bm as *mut _ as *mut _) == 0 {
            return None;
        }
        let (w, h) = (bm.bmWidth, bm.bmHeight.abs());
        if w <= 0 || h <= 0 || w > 8192 || h > 8192 {
            return None;
        }
        let mut bi: BITMAPINFO = zeroed();
        bi.bmiHeader.biSize = size_of::<BITMAPINFOHEADER>() as u32;
        bi.bmiHeader.biWidth = w;
        bi.bmiHeader.biHeight = -h; // top-down
        bi.bmiHeader.biPlanes = 1;
        bi.bmiHeader.biBitCount = 32;
        bi.bmiHeader.biCompression = BI_RGB;
        let mut px = vec![0u8; (w * h * 4) as usize];
        let dc = GetDC(null_mut());
        let lines = GetDIBits(dc, hbm, 0, h as u32, px.as_mut_ptr() as *mut _, &mut bi, DIB_RGB_COLORS);
        ReleaseDC(null_mut(), dc);
        if lines == 0 {
            return None;
        }
        let any_alpha = !opaque && px.chunks(4).any(|p| p[3] != 0);
        for p in px.chunks_mut(4) {
            p.swap(0, 2); // BGRA -> RGBA
            if !any_alpha {
                p[3] = 255;
            }
        }
        Some((w as u32, h as u32, px))
    }
}

/// Converts an icon handle to a PNG data: URL.
pub fn icon_url(hicon: HICON) -> Option<String> {
    if hicon.is_null() {
        return None;
    }
    unsafe {
        let mut ii: ICONINFO = zeroed();
        if GetIconInfo(hicon, &mut ii) == 0 {
            return None;
        }
        let out = if !ii.hbmColor.is_null() {
            bitmap_rgba(ii.hbmColor, false).and_then(|(w, h, px)| png_url(w, h, &px))
        } else {
            None
        };
        if !ii.hbmColor.is_null() {
            DeleteObject(ii.hbmColor as HGDIOBJ);
        }
        if !ii.hbmMask.is_null() {
            DeleteObject(ii.hbmMask as HGDIOBJ);
        }
        out
    }
}

/// The icon a window shows in its title bar / the taskbar.
pub fn window_icon(h: isize) -> Option<String> {
    unsafe {
        let hw = h as HWND;
        let mut res: usize = 0;
        SendMessageTimeoutW(hw, WM_GETICON, ICON_BIG as usize, 0, SMTO_ABORTIFHUNG, 60, &mut res);
        if res == 0 {
            SendMessageTimeoutW(hw, WM_GETICON, 2 /* ICON_SMALL2 */, 0, SMTO_ABORTIFHUNG, 60, &mut res);
        }
        if res == 0 {
            res = GetClassLongPtrW(hw, GCLP_HICON);
        }
        icon_url(res as HICON)
    }
}

/// A small picture of a window's current contents (not for minimised windows).
pub fn window_thumb(h: isize, max_w: i32) -> Option<String> {
    unsafe {
        let hw = h as HWND;
        if IsIconic(hw) != 0 {
            return None;
        }
        let mut r: RECT = zeroed();
        if GetWindowRect(hw, &mut r) == 0 {
            return None;
        }
        let (w, hgt) = (r.right - r.left, r.bottom - r.top);
        if w < 50 || hgt < 50 {
            return None;
        }
        let screen = GetDC(null_mut());
        let full_dc = CreateCompatibleDC(screen);
        let full = CreateCompatibleBitmap(screen, w, hgt);
        let old = SelectObject(full_dc, full as HGDIOBJ);
        let ok = PrintWindow(hw, full_dc, 2 /* PW_RENDERFULLCONTENT */) != 0;
        let tw = max_w.min(w);
        let th = (hgt as i64 * tw as i64 / w as i64) as i32;
        let small_dc = CreateCompatibleDC(screen);
        let small = CreateCompatibleBitmap(screen, tw, th);
        let old2 = SelectObject(small_dc, small as HGDIOBJ);
        SetStretchBltMode(small_dc, HALFTONE);
        StretchBlt(small_dc, 0, 0, tw, th, full_dc, 0, 0, w, hgt, SRCCOPY);
        SelectObject(small_dc, old2);
        SelectObject(full_dc, old);
        let out = if ok { bitmap_rgba(small, true).and_then(|(a, b, px)| png_url(a, b, &px)) } else { None };
        DeleteObject(small as HGDIOBJ);
        DeleteObject(full as HGDIOBJ);
        DeleteDC(small_dc);
        DeleteDC(full_dc);
        ReleaseDC(null_mut(), screen);
        out
    }
}

#[derive(Serialize, Clone, Debug)]
pub struct Win {
    pub hwnd: isize,
    pub title: String,
    pub process: String,
    pub icon: Option<String>,
}

fn is_cloaked(h: HWND) -> bool {
    let mut v: u32 = 0;
    unsafe { DwmGetWindowAttribute(h, DWMWA_CLOAKED as u32, &mut v as *mut u32 as *mut _, 4) == 0 && v != 0 }
}

unsafe extern "system" fn collect(h: HWND, lp: LPARAM) -> i32 {
    let out = &mut *(lp as *mut Vec<isize>);
    if IsWindowVisible(h) == 0 || !GetWindow(h, GW_OWNER).is_null() || is_cloaked(h) {
        return 1;
    }
    let ex = GetWindowLongPtrW(h, GWL_EXSTYLE) as u32;
    if ex & WS_EX_TOOLWINDOW != 0 && ex & WS_EX_APPWINDOW == 0 {
        return 1;
    }
    if GetWindowTextLengthW(h) == 0 {
        return 1;
    }
    out.push(h as isize);
    1
}

/// Open windows in Alt+Tab order (front-most first), without icons.
pub fn windows(exclude: &[isize]) -> Vec<Win> {
    let mut hs: Vec<isize> = Vec::new();
    unsafe {
        EnumWindows(Some(collect), &mut hs as *mut Vec<isize> as LPARAM);
    }
    hs.into_iter()
        .filter(|h| !exclude.contains(h) && !crate::window::is_shell(*h))
        .map(|h| {
            let mut buf = [0u16; 512];
            let n = unsafe { GetWindowTextW(h as HWND, buf.as_mut_ptr(), buf.len() as i32) };
            Win {
                hwnd: h,
                title: from_wide(&buf[..n.max(0) as usize]),
                process: crate::window::process_name(h),
                icon: None,
            }
        })
        .filter(|w| !matches!(w.title.as_str(), "Program Manager" | "Windows Input Experience" | "Settings"))
        .collect()
}

