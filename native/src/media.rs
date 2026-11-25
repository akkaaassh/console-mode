//! Music controls (media keys) and Spotify's now-playing title.

use crate::keys;
use crate::util::from_wide;
use serde::Serialize;
use windows_sys::Win32::Foundation::{CloseHandle, HWND, LPARAM};
use windows_sys::Win32::System::Threading::{
    OpenProcess, QueryFullProcessImageNameW, PROCESS_QUERY_LIMITED_INFORMATION,
};
use windows_sys::Win32::UI::WindowsAndMessaging::{EnumWindows, GetWindowTextW, GetWindowThreadProcessId};

/// Sends a media key. Works with Spotify, browsers and most players.
pub fn key(cmd: &str) -> bool {
    let vk = match cmd {
        "play" | "pause" | "play_pause" => keys::VK_PLAY_PAUSE,
        "next" => keys::VK_NEXT_TRACK,
        "prev" => keys::VK_PREV_TRACK,
        "vol_up" => keys::VK_VOL_UP,
        "vol_down" => keys::VK_VOL_DOWN,
        "mute" => keys::VK_VOL_MUTE,
        _ => return false,
    };
    keys::tap(vk);
    true
}

#[derive(Serialize, Clone, Debug, PartialEq)]
pub struct NowPlaying {
    pub source: String,
    pub title: String,
    pub artist: String,
    pub playing: bool,
}

/// Reads the Spotify desktop app's window title, which is "Artist - Song"
/// while playing and "Spotify Free/Premium" when paused.
pub fn now_playing() -> Option<NowPlaying> {
    let mut titles: Vec<String> = Vec::new();
    unsafe {
        EnumWindows(Some(collect), &mut titles as *mut Vec<String> as LPARAM);
    }
    let mut paused = false;
    for t in &titles {
        if let Some(np) = parse_title(t) {
            return Some(np);
        }
        if t.starts_with("Spotify") {
            paused = true;
        }
    }
    paused.then(|| NowPlaying {
        source: "Spotify".into(),
        title: String::new(),
        artist: String::new(),
        playing: false,
    })
}

fn parse_title(t: &str) -> Option<NowPlaying> {
    if t.starts_with("Spotify") {
        return None;
    }
    let (artist, title) = t.split_once(" - ")?;
    Some(NowPlaying {
        source: "Spotify".into(),
        title: title.trim().to_string(),
        artist: artist.trim().to_string(),
        playing: true,
    })
}

unsafe extern "system" fn collect(hwnd: HWND, lparam: LPARAM) -> i32 {
    let titles = &mut *(lparam as *mut Vec<String>);
    let mut buf = [0u16; 512];
    let n = GetWindowTextW(hwnd, buf.as_mut_ptr(), buf.len() as i32);
    if n <= 0 {
        return 1;
    }
    let mut pid = 0u32;
    GetWindowThreadProcessId(hwnd, &mut pid);
    if pid == 0 || !is_spotify(pid) {
        return 1;
    }
    titles.push(from_wide(&buf[..n as usize]));
    1
}

fn is_spotify(pid: u32) -> bool {
    unsafe {
        let h = OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, 0, pid);
        if h.is_null() {
            return false;
        }
        let mut buf = [0u16; 1024];
        let mut len = buf.len() as u32;
        let ok = QueryFullProcessImageNameW(h, 0, buf.as_mut_ptr(), &mut len) != 0;
        CloseHandle(h);
        ok && from_wide(&buf[..len as usize]).to_lowercase().ends_with("\\spotify.exe")
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn titles() {
        let np = parse_title("Some Artist - Some Song - Live").unwrap();
        assert_eq!(np.artist, "Some Artist");
        assert_eq!(np.title, "Some Song - Live");
        assert!(parse_title("Spotify Premium").is_none());
        assert!(parse_title("Spotify").is_none());
    }
}
