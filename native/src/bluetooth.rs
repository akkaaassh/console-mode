//! Bluetooth: list, pair, connect, disconnect and forget devices (classic Bluetooth API).

use crate::util::from_wide;
use serde::Serialize;
use std::mem::size_of;
use std::ptr::null_mut;
use windows_sys::core::GUID;
use windows_sys::Win32::Devices::Bluetooth::*;
use windows_sys::Win32::Foundation::{CloseHandle, HANDLE};

#[derive(Serialize, Clone, Debug)]
pub struct Device {
    pub name: String,
    pub address: String,
    pub connected: bool,
    pub paired: bool,
    /// "audio", "input", "phone", "computer" or "other".
    pub kind: String,
}

/// Bluetooth SIG short UUIDs on the standard base.
const fn sig(short: u32) -> GUID {
    GUID::from_u128(((short as u128) << 96) | 0x0000_1000_8000_0080_5F9B_34FB)
}
const AUDIO_SINK: GUID = sig(0x110B);
const AV_REMOTE: GUID = sig(0x110E);
const HANDSFREE: GUID = sig(0x111E);
const HEADSET: GUID = sig(0x1108);
const HID: GUID = sig(0x1124);
const SERVICES: [GUID; 5] = [AUDIO_SINK, AV_REMOTE, HANDSFREE, HEADSET, HID];

struct Radio(HANDLE);
impl Radio {
    fn first() -> Option<Radio> {
        unsafe {
            let p = BLUETOOTH_FIND_RADIO_PARAMS { dwSize: size_of::<BLUETOOTH_FIND_RADIO_PARAMS>() as u32 };
            let mut h: HANDLE = std::mem::zeroed();
            let f = BluetoothFindFirstRadio(&p, &mut h);
            if f.is_null() {
                return None;
            }
            BluetoothFindRadioClose(f);
            Some(Radio(h))
        }
    }
}
impl Drop for Radio {
    fn drop(&mut self) {
        unsafe {
            CloseHandle(self.0);
        }
    }
}

/// True when the PC has a working Bluetooth radio that is switched on.
pub fn available() -> bool {
    Radio::first().is_some()
}

fn addr_string(a: &BLUETOOTH_ADDRESS) -> String {
    let b = unsafe { a.Anonymous.rgBytes };
    format!("{:02X}:{:02X}:{:02X}:{:02X}:{:02X}:{:02X}", b[5], b[4], b[3], b[2], b[1], b[0])
}

fn parse_addr(s: &str) -> Option<u64> {
    let hex: String = s.chars().filter(|c| c.is_ascii_hexdigit()).collect();
    if hex.len() != 12 {
        return None;
    }
    u64::from_str_radix(&hex, 16).ok()
}

fn kind_of(cod: u32) -> &'static str {
    match (cod >> 8) & 0x1F {
        0x04 => "audio",
        0x05 => "input",
        0x02 => "phone",
        0x01 => "computer",
        _ => "other",
    }
}

fn search(radio: &Radio, inquire: bool) -> Vec<BLUETOOTH_DEVICE_INFO> {
    let mut out = Vec::new();
    unsafe {
        let p = BLUETOOTH_DEVICE_SEARCH_PARAMS {
            dwSize: size_of::<BLUETOOTH_DEVICE_SEARCH_PARAMS>() as u32,
            fReturnAuthenticated: 1,
            fReturnRemembered: 1,
            fReturnUnknown: if inquire { 1 } else { 0 },
            fReturnConnected: 1,
            fIssueInquiry: if inquire { 1 } else { 0 },
            cTimeoutMultiplier: if inquire { 6 } else { 0 }, // 6 × 1.28 s
            hRadio: radio.0,
        };
        let mut info: BLUETOOTH_DEVICE_INFO = std::mem::zeroed();
        info.dwSize = size_of::<BLUETOOTH_DEVICE_INFO>() as u32;
        let f = BluetoothFindFirstDevice(&p, &mut info);
        if f.is_null() {
            return out;
        }
        loop {
            out.push(info);
            info = std::mem::zeroed();
            info.dwSize = size_of::<BLUETOOTH_DEVICE_INFO>() as u32;
            if BluetoothFindNextDevice(f, &mut info) == 0 {
                break;
            }
        }
        BluetoothFindDeviceClose(f);
    }
    out
}

/// Paired devices; with `scan`, also looks for new devices nearby (takes about 8 seconds).
pub fn devices(scan: bool) -> Vec<Device> {
    let Some(r) = Radio::first() else { return vec![] };
    let mut out: Vec<Device> = search(&r, scan)
        .iter()
        .map(|d| Device {
            name: {
                let n = from_wide(&d.szName);
                if n.is_empty() { addr_string(&d.Address) } else { n }
            },
            address: addr_string(&d.Address),
            connected: d.fConnected != 0,
            paired: d.fAuthenticated != 0 || d.fRemembered != 0,
            kind: kind_of(d.ulClassofDevice).to_string(),
        })
        .collect();
    out.sort_by(|a, b| b.connected.cmp(&a.connected).then(b.paired.cmp(&a.paired)).then(a.name.cmp(&b.name)));
    out.dedup_by(|a, b| a.address == b.address);
    out
}

fn find(radio: &Radio, address: &str) -> Option<BLUETOOTH_DEVICE_INFO> {
    let want = parse_addr(address)?;
    search(radio, false)
        .into_iter()
        .find(|d| unsafe { d.Address.Anonymous.ullLong } & 0xFFFF_FFFF_FFFF == want)
        .or_else(|| {
            let mut info: BLUETOOTH_DEVICE_INFO = unsafe { std::mem::zeroed() };
            info.dwSize = size_of::<BLUETOOTH_DEVICE_INFO>() as u32;
            info.Address.Anonymous.ullLong = want;
            Some(info)
        })
}

fn set_services(radio: &Radio, info: &BLUETOOTH_DEVICE_INFO, on: bool) -> bool {
    let mut any = false;
    for g in SERVICES {
        let r = unsafe { BluetoothSetServiceState(radio.0, info, &g, if on { BLUETOOTH_SERVICE_ENABLE } else { 0 }) };
        any |= r == 0;
    }
    any
}

/// Pairs with a nearby device. Windows shows its own prompt if the device needs a PIN.
pub fn pair(address: &str) -> Result<(), String> {
    let r = Radio::first().ok_or("Bluetooth is off or missing")?;
    let mut info = find(&r, address).ok_or("Device not found")?;
    let code = unsafe { BluetoothAuthenticateDeviceEx(null_mut(), r.0, &mut info, std::ptr::null(), MITMProtectionNotRequired) };
    if code != 0 && code != 183 {
        // 183 = already paired
        return Err(format!("Pairing didn't finish (code {code})"));
    }
    set_services(&r, &info, true);
    Ok(())
}

/// Connects a paired device by switching its audio/input services on.
pub fn connect(address: &str) -> bool {
    let Some(r) = Radio::first() else { return false };
    let Some(info) = find(&r, address) else { return false };
    set_services(&r, &info, true)
}

/// Disconnects a device by switching its services off. It stays paired; Connect brings it back.
pub fn disconnect(address: &str) -> bool {
    let Some(r) = Radio::first() else { return false };
    let Some(info) = find(&r, address) else { return false };
    set_services(&r, &info, false)
}

/// Removes ("forgets") a paired device.
pub fn forget(address: &str) -> bool {
    let Some(a) = parse_addr(address) else { return false };
    let mut addr: BLUETOOTH_ADDRESS = unsafe { std::mem::zeroed() };
    addr.Anonymous.ullLong = a;
    unsafe { BluetoothRemoveDevice(&addr) == 0 }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn addresses_and_kinds() {
        assert_eq!(parse_addr("AA:BB:CC:DD:EE:FF"), Some(0xAABBCCDDEEFF));
        assert_eq!(parse_addr("bad"), None);
        assert_eq!(kind_of(0x240404), "audio");
        assert_eq!(kind_of(0x002508), "input");
        let g = GUID::from_u128(0x0000110b_0000_1000_8000_00805f9b34fb);
        assert_eq!((AUDIO_SINK.data1, AUDIO_SINK.data2, AUDIO_SINK.data3, AUDIO_SINK.data4), (g.data1, g.data2, g.data3, g.data4));
    }
}
