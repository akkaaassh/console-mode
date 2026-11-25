//! Power mode and screen brightness.

use crate::util::{powershell, run_hidden, wide};
use windows_sys::core::GUID;
use windows_sys::Win32::System::LibraryLoader::{GetProcAddress, LoadLibraryW};

// Windows 10/11 power-mode overlays (the slider in Settings > Power).
const OVERLAY_SAVER: GUID = GUID::from_u128(0x961cc777_2547_4f9d_8174_7d86181b8a7a);
const OVERLAY_BALANCED: GUID = GUID::from_u128(0);
const OVERLAY_PERF: GUID = GUID::from_u128(0xded574b5_45a0_4f42_8737_46345c09c238);

// Classic power plans, used when the overlay isn't available.
const PLAN_SAVER: &str = "a1841308-3541-4fab-bc81-f71556f20b4a";
const PLAN_BALANCED: &str = "381b4222-f694-41f0-9685-ff5bb260df2e";
const PLAN_PERF: &str = "8c5e7fda-e8bf-4a96-9a85-a6e23a8c635c";

type SetOverlayFn = unsafe extern "system" fn(*const GUID) -> u32;

/// mode: "saver", "balanced" or "perf".
pub fn set_mode(mode: &str) -> bool {
    let (overlay, plan) = match mode {
        "saver" => (OVERLAY_SAVER, PLAN_SAVER),
        "perf" => (OVERLAY_PERF, PLAN_PERF),
        _ => (OVERLAY_BALANCED, PLAN_BALANCED),
    };
    unsafe {
        let lib = LoadLibraryW(wide("powrprof.dll").as_ptr());
        if !lib.is_null() {
            if let Some(f) = GetProcAddress(lib, b"PowerSetActiveOverlayScheme\0".as_ptr()) {
                let set: SetOverlayFn = std::mem::transmute::<unsafe extern "system" fn() -> isize, SetOverlayFn>(f);
                if set(&overlay) == 0 {
                    return true;
                }
            }
        }
    }
    run_hidden("powercfg.exe", &["/setactive", plan])
}

/// 0–100. Works on laptop panels and monitors that expose brightness to Windows.
pub fn set_brightness(value: u32) -> bool {
    let v = value.min(100);
    powershell(&format!(
        "$m = Get-CimInstance -Namespace root/WMI -ClassName WmiMonitorBrightnessMethods -ErrorAction Stop; \
         Invoke-CimMethod -InputObject $m -MethodName WmiSetBrightness -Arguments @{{Timeout=1;Brightness={v}}} -ErrorAction Stop | Out-Null; 'ok'"
    ))
    .is_some_and(|s| s.contains("ok"))
}

pub fn get_brightness() -> Option<u32> {
    powershell(
        "(Get-CimInstance -Namespace root/WMI -ClassName WmiMonitorBrightness -ErrorAction Stop | Select-Object -First 1).CurrentBrightness",
    )
    .and_then(|s| s.trim().parse().ok())
}
