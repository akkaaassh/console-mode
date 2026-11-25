//! Wi-Fi through the Windows WLAN API: list networks, connect (with a password if needed), disconnect.

use crate::util::{from_wide, wide};
use serde::Serialize;
use std::ptr::{null, null_mut};
use windows_sys::core::GUID;
use windows_sys::Win32::Foundation::HANDLE;
use windows_sys::Win32::NetworkManagement::WiFi::*;

#[derive(Serialize, Clone, Debug)]
pub struct Network {
    pub ssid: String,
    /// 0–100.
    pub signal: u32,
    pub secure: bool,
    pub connected: bool,
    /// Windows already knows the password.
    pub saved: bool,
    /// "open", "wpa2", "wpa3", "wpa" or "other".
    pub kind: String,
}

struct Wlan {
    h: HANDLE,
    iface: Option<GUID>,
}

impl Wlan {
    fn open() -> Option<Wlan> {
        unsafe {
            let mut ver = 0u32;
            let mut h: HANDLE = std::mem::zeroed();
            if WlanOpenHandle(2, null(), &mut ver, &mut h) != 0 {
                return None;
            }
            let mut list: *mut WLAN_INTERFACE_INFO_LIST = null_mut();
            let mut iface = None;
            if WlanEnumInterfaces(h, null(), &mut list) == 0 && !list.is_null() {
                if (*list).dwNumberOfItems > 0 {
                    let first = (*list).InterfaceInfo.as_ptr();
                    iface = Some((*first).InterfaceGuid);
                }
                WlanFreeMemory(list as *const _);
            }
            Some(Wlan { h, iface })
        }
    }
}

impl Drop for Wlan {
    fn drop(&mut self) {
        unsafe {
            WlanCloseHandle(self.h, null());
        }
    }
}

/// True when the PC has a Wi-Fi adapter.
pub fn available() -> bool {
    Wlan::open().is_some_and(|w| w.iface.is_some())
}

/// Asks the adapter to look for networks (results arrive a few seconds later).
pub fn scan() -> bool {
    let Some(w) = Wlan::open() else { return false };
    let Some(g) = w.iface else { return false };
    unsafe { WlanScan(w.h, &g, null(), null(), null()) == 0 }
}

pub fn networks() -> Vec<Network> {
    let Some(w) = Wlan::open() else { return vec![] };
    let Some(g) = w.iface else { return vec![] };
    let mut out: Vec<Network> = Vec::new();
    unsafe {
        let mut list: *mut WLAN_AVAILABLE_NETWORK_LIST = null_mut();
        if WlanGetAvailableNetworkList(w.h, &g, 0, null(), &mut list) != 0 || list.is_null() {
            return out;
        }
        let n = (*list).dwNumberOfItems as usize;
        let first = (*list).Network.as_ptr();
        for i in 0..n {
            let net = &*first.add(i);
            let len = (net.dot11Ssid.uSSIDLength as usize).min(32);
            let ssid = String::from_utf8_lossy(&net.dot11Ssid.ucSSID[..len]).to_string();
            if ssid.is_empty() {
                continue; // hidden network
            }
            let connected = net.dwFlags & WLAN_AVAILABLE_NETWORK_CONNECTED != 0;
            let saved = net.dwFlags & WLAN_AVAILABLE_NETWORK_HAS_PROFILE != 0;
            let kind = match net.dot11DefaultAuthAlgorithm {
                DOT11_AUTH_ALGO_80211_OPEN => "open",
                DOT11_AUTH_ALGO_RSNA_PSK => "wpa2",
                DOT11_AUTH_ALGO_WPA3 | DOT11_AUTH_ALGO_WPA3_SAE => "wpa3",
                DOT11_AUTH_ALGO_WPA_PSK => "wpa",
                _ => "other",
            };
            let item = Network {
                ssid,
                signal: net.wlanSignalQuality.min(100),
                secure: net.bSecurityEnabled != 0,
                connected,
                saved,
                kind: kind.to_string(),
            };
            // The same SSID can appear twice (with and without a saved profile); keep the best.
            if let Some(e) = out.iter_mut().find(|e| e.ssid == item.ssid) {
                e.connected |= item.connected;
                e.saved |= item.saved;
                e.signal = e.signal.max(item.signal);
            } else {
                out.push(item);
            }
        }
        WlanFreeMemory(list as *const _);
    }
    out.sort_by(|a, b| b.connected.cmp(&a.connected).then(b.signal.cmp(&a.signal)));
    out
}

fn xml_escape(s: &str) -> String {
    s.replace('&', "&amp;").replace('<', "&lt;").replace('>', "&gt;").replace('"', "&quot;").replace('\'', "&apos;")
}

/// Windows profile XML for a personal (password) or open network.
pub fn profile_xml(ssid: &str, kind: &str, password: Option<&str>) -> String {
    let name = xml_escape(ssid);
    let hex: String = ssid.bytes().map(|b| format!("{b:02X}")).collect();
    let (auth, enc, key) = match (kind, password) {
        ("open", _) | (_, None) => ("open", "none", String::new()),
        ("wpa3", Some(p)) => ("WPA3SAE", "AES", p.to_string()),
        ("wpa", Some(p)) => ("WPAPSK", "AES", p.to_string()),
        (_, Some(p)) => ("WPA2PSK", "AES", p.to_string()),
    };
    let shared = if key.is_empty() {
        String::new()
    } else {
        format!(
            "<sharedKey><keyType>passPhrase</keyType><protected>false</protected><keyMaterial>{}</keyMaterial></sharedKey>",
            xml_escape(&key)
        )
    };
    format!(
        "<?xml version=\"1.0\"?><WLANProfile xmlns=\"http://www.microsoft.com/networking/WLAN/profile/v1\"><name>{name}</name><SSIDConfig><SSID><hex>{hex}</hex><name>{name}</name></SSID></SSIDConfig><connectionType>ESS</connectionType><connectionMode>auto</connectionMode><MSM><security><authEncryption><authentication>{auth}</authentication><encryption>{enc}</encryption><useOneX>false</useOneX></authEncryption>{shared}</security></MSM></WLANProfile>"
    )
}

/// Connects to a network. A password is needed only for secured networks Windows doesn't know yet.
pub fn connect(ssid: &str, password: Option<&str>) -> Result<(), String> {
    let w = Wlan::open().ok_or("Wi-Fi isn't available")?;
    let g = w.iface.ok_or("No Wi-Fi adapter found")?;
    let net = networks().into_iter().find(|n| n.ssid == ssid).ok_or("That network is out of range")?;
    unsafe {
        if !net.saved || password.is_some() {
            if net.secure && password.is_none() {
                return Err("This network needs a password".into());
            }
            let xml = wide(&profile_xml(ssid, &net.kind, password));
            let mut reason = 0u32;
            let r = WlanSetProfile(w.h, &g, 0, xml.as_ptr(), null(), 1, null(), &mut reason);
            if r != 0 {
                return Err(format!("Windows didn't accept the network settings (code {r}, reason {reason})"));
            }
        }
        let profile = wide(ssid);
        let params = WLAN_CONNECTION_PARAMETERS {
            wlanConnectionMode: wlan_connection_mode_profile,
            strProfile: profile.as_ptr(),
            pDot11Ssid: null_mut(),
            pDesiredBssidList: null_mut(),
            dot11BssType: dot11_BSS_type_infrastructure,
            dwFlags: 0,
        };
        let r = WlanConnect(w.h, &g, &params, null());
        if r != 0 {
            return Err(format!("Couldn't connect (code {r})"));
        }
    }
    Ok(())
}

pub fn disconnect() -> bool {
    let Some(w) = Wlan::open() else { return false };
    let Some(g) = w.iface else { return false };
    unsafe { WlanDisconnect(w.h, &g, null()) == 0 }
}

/// Name of the adapter, for display.
pub fn adapter_name() -> Option<String> {
    unsafe {
        let w = Wlan::open()?;
        let mut list: *mut WLAN_INTERFACE_INFO_LIST = null_mut();
        if WlanEnumInterfaces(w.h, null(), &mut list) != 0 || list.is_null() {
            return None;
        }
        let name = if (*list).dwNumberOfItems > 0 {
            Some(from_wide(&(*(*list).InterfaceInfo.as_ptr()).strInterfaceDescription))
        } else {
            None
        };
        WlanFreeMemory(list as *const _);
        name
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn profile_has_password_and_escapes() {
        let x = profile_xml("Café & Co", "wpa2", Some("p<w>d"));
        assert!(x.contains("<authentication>WPA2PSK</authentication>"));
        assert!(x.contains("<keyMaterial>p&lt;w&gt;d</keyMaterial>"));
        assert!(x.contains("<name>Café &amp; Co</name>"));
        let o = profile_xml("Free", "open", None);
        assert!(o.contains("<authentication>open</authentication>") && !o.contains("sharedKey"));
    }
}
