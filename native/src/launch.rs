//! Opening games, apps and URIs.

use crate::util::wide;
use std::ptr::{null, null_mut};
use windows_sys::Win32::UI::Shell::ShellExecuteW;
use windows_sys::Win32::UI::WindowsAndMessaging::SW_SHOWNORMAL;

/// ShellExecute "open" on a file, program or URI (steam://, com.epicgames.launcher://, discord://).
pub fn open(target: &str, args: Option<&str>, workdir: Option<&str>) -> bool {
    let verb = wide("open");
    let file = wide(target);
    let args_w = args.map(wide);
    let dir_w = workdir.map(wide);
    let r = unsafe {
        ShellExecuteW(
            null_mut(),
            verb.as_ptr(),
            file.as_ptr(),
            args_w.as_ref().map_or(null(), |v| v.as_ptr()),
            dir_w.as_ref().map_or(null(), |v| v.as_ptr()),
            SW_SHOWNORMAL,
        )
    };
    // ShellExecute reports success with a value greater than 32.
    (r as isize) > 32
}

/// Opens a Start-menu app by its AppUserModelID (Store apps, web apps, desktop shortcuts).
pub fn apps_folder(app_id: &str) -> bool {
    open("explorer.exe", Some(&format!("shell:AppsFolder\\{app_id}")), None)
}
