//! Small registry readers.

use crate::util::{from_wide, wide};
use std::ptr::null_mut;
use windows_sys::Win32::System::Registry::*;

pub use windows_sys::Win32::System::Registry::{HKEY_CURRENT_USER, HKEY_LOCAL_MACHINE};

pub fn read_str(root: HKEY, subkey: &str, value: &str) -> Option<String> {
    let mut buf = vec![0u16; 4096];
    let mut size = (buf.len() * 2) as u32;
    let sub = wide(subkey);
    let val = wide(value);
    let r = unsafe {
        RegGetValueW(
            root,
            sub.as_ptr(),
            val.as_ptr(),
            RRF_RT_REG_SZ | RRF_RT_REG_EXPAND_SZ,
            null_mut(),
            buf.as_mut_ptr() as *mut _,
            &mut size,
        )
    };
    if r != 0 {
        return None;
    }
    let s = from_wide(&buf[..(size as usize / 2).min(buf.len())]);
    if s.is_empty() { None } else { Some(s) }
}

/// Names of the direct subkeys of a key.
pub fn subkeys(root: HKEY, subkey: &str) -> Vec<String> {
    let mut out = Vec::new();
    let sub = wide(subkey);
    let mut hk: HKEY = unsafe { std::mem::zeroed() };
    if unsafe { RegOpenKeyExW(root, sub.as_ptr(), 0, KEY_READ, &mut hk) } != 0 {
        return out;
    }
    let mut i = 0u32;
    loop {
        let mut name = [0u16; 512];
        let mut len = name.len() as u32;
        let r = unsafe {
            RegEnumKeyExW(hk, i, name.as_mut_ptr(), &mut len, null_mut(), null_mut(), null_mut(), null_mut())
        };
        if r != 0 {
            break;
        }
        out.push(from_wide(&name[..len as usize]));
        i += 1;
    }
    unsafe {
        RegCloseKey(hk);
    }
    out
}
