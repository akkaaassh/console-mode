//! Websites as apps: opens a URL in its own full-screen browser window without tabs or bars.

use crate::reg;

/// Edge or Chrome (whichever is installed), for app-style windows.
fn browser() -> Option<String> {
    for exe in ["msedge.exe", "chrome.exe", "brave.exe"] {
        let key = format!("SOFTWARE\\Microsoft\\Windows\\CurrentVersion\\App Paths\\{exe}");
        for root in [reg::HKEY_LOCAL_MACHINE, reg::HKEY_CURRENT_USER] {
            if let Some(p) = reg::read_str(root, &key, "") {
                if std::path::Path::new(&p).is_file() {
                    return Some(p);
                }
            }
        }
    }
    None
}

pub fn open(url: &str, fullscreen: bool) -> bool {
    let url = if url.contains("://") { url.to_string() } else { format!("https://{url}") };
    if !(url.starts_with("https://") || url.starts_with("http://")) {
        return false;
    }
    match browser() {
        Some(b) => {
            let args = format!("--app=\"{url}\"{}", if fullscreen { " --start-fullscreen" } else { "" });
            crate::launch::open(&b, Some(&args), None)
        }
        None => crate::launch::open(&url, None, None),
    }
}
