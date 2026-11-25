//! Windows Runtime / COM features:
//! - the system media session (what's playing in any app, with artwork and controls),
//! - the system volume,
//! - checking whether the focused control is a text field (UI Automation).
//!
//! These need the `windows` crate and only build for Windows; other targets get stubs.

use serde::Serialize;

#[derive(Serialize, Clone, Debug, Default)]
pub struct MediaInfo {
    pub title: String,
    pub artist: String,
    pub album: String,
    /// App that is playing, e.g. "Spotify", "Browser".
    pub source: String,
    pub playing: bool,
    /// Seconds.
    pub position: f64,
    pub duration: f64,
    /// data: URL of the cover art, when asked for and available.
    pub art: Option<String>,
}

#[cfg(windows)]
mod imp {
    use super::MediaInfo;
    use windows::core::{Interface, Result};
    use windows::Media::Control::*;
    use windows::Storage::Streams::DataReader;
    use windows::Win32::Media::Audio::Endpoints::IAudioEndpointVolume;
    use windows::Win32::Media::Audio::{eConsole, eRender, IMMDeviceEnumerator, MMDeviceEnumerator};
    use windows::Win32::System::Com::{CoCreateInstance, CoInitializeEx, CLSCTX_ALL, CLSCTX_INPROC_SERVER, COINIT_MULTITHREADED};
    use windows::Win32::UI::Accessibility::*;
    use windows_future::IAsyncOperation;

    fn com() {
        unsafe {
            let _ = CoInitializeEx(None, COINIT_MULTITHREADED);
        }
    }

    fn session() -> Result<GlobalSystemMediaTransportControlsSession> {
        com();
        GlobalSystemMediaTransportControlsSessionManager::RequestAsync()?.join()?.GetCurrentSession()
    }

    fn source_name(app_id: &str) -> String {
        let a = app_id.to_lowercase();
        let named = [
            ("spotify", "Spotify"), ("chrome", "Chrome"), ("msedge", "Edge"), ("firefox", "Firefox"),
            ("brave", "Brave"), ("vlc", "VLC"), ("zunemusic", "Media Player"), ("music", "Music"),
            ("netflix", "Netflix"), ("primevideo", "Prime Video"), ("youtube", "YouTube"),
        ];
        for (k, v) in named {
            if a.contains(k) {
                return v.to_string();
            }
        }
        app_id.rsplit(['\\', '!']).next().unwrap_or("").trim_end_matches(".exe").to_string()
    }

    /// 100-ns ticks since 1601 (Windows DateTime) for "now".
    fn now_ticks() -> i64 {
        let unix = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_nanos() as i64 / 100)
            .unwrap_or(0);
        unix + 116_444_736_000_000_000
    }

    pub fn now_playing(want_art: bool) -> Option<MediaInfo> {
        let s = session().ok()?;
        let props = s.TryGetMediaPropertiesAsync().ok()?.join().ok()?;
        let title = props.Title().map(|h| h.to_string()).unwrap_or_default();
        if title.is_empty() {
            return None;
        }
        let playing = s
            .GetPlaybackInfo()
            .and_then(|p| p.PlaybackStatus())
            .map(|st| st == GlobalSystemMediaTransportControlsSessionPlaybackStatus::Playing)
            .unwrap_or(false);
        let (mut position, mut duration) = (0.0, 0.0);
        if let Ok(t) = s.GetTimelineProperties() {
            let pos = t.Position().map(|x| x.Duration).unwrap_or(0);
            let end = t.EndTime().map(|x| x.Duration).unwrap_or(0);
            let updated = t.LastUpdatedTime().map(|x| x.UniversalTime).unwrap_or(0);
            let mut p = pos;
            // Position is reported as of the last update; move it on while playing.
            if playing && updated > 0 {
                p += (now_ticks() - updated).max(0);
            }
            if end > 0 {
                p = p.min(end);
            }
            position = p as f64 / 1e7;
            duration = end as f64 / 1e7;
        }
        let art = if want_art { thumbnail(&props).ok().flatten() } else { None };
        Some(MediaInfo {
            title,
            artist: props.Artist().map(|h| h.to_string()).unwrap_or_default(),
            album: props.AlbumTitle().map(|h| h.to_string()).unwrap_or_default(),
            source: s.SourceAppUserModelId().map(|h| source_name(&h.to_string())).unwrap_or_default(),
            playing,
            position,
            duration,
            art,
        })
    }

    fn thumbnail(props: &GlobalSystemMediaTransportControlsSessionMediaProperties) -> Result<Option<String>> {
        let Ok(r) = props.Thumbnail() else { return Ok(None) };
        let stream = r.OpenReadAsync()?.join()?;
        let size = stream.Size()? as u32;
        if size == 0 || size > 8_000_000 {
            return Ok(None);
        }
        let mime = stream.ContentType().map(|h| h.to_string()).unwrap_or_default();
        let reader = DataReader::CreateDataReader(&stream)?;
        let op: IAsyncOperation<u32> = reader.LoadAsync(size)?.cast()?;
        op.join()?;
        let mut buf = vec![0u8; size as usize];
        reader.ReadBytes(&mut buf)?;
        let mime = if mime.starts_with("image/") { mime } else { "image/jpeg".to_string() };
        Ok(Some(format!("data:{mime};base64,{}", super::base64(&buf))))
    }

    pub fn control(cmd: &str) -> bool {
        let Ok(s) = session() else { return false };
        let op = match cmd {
            "play_pause" | "play" | "pause" => s.TryTogglePlayPauseAsync(),
            "next" => s.TrySkipNextAsync(),
            "prev" => s.TrySkipPreviousAsync(),
            _ => return false,
        };
        op.and_then(|o| o.join()).unwrap_or(false)
    }

    fn endpoint() -> Result<IAudioEndpointVolume> {
        com();
        unsafe {
            let en: IMMDeviceEnumerator = CoCreateInstance(&MMDeviceEnumerator, None, CLSCTX_ALL)?;
            let dev = en.GetDefaultAudioEndpoint(eRender, eConsole)?;
            dev.Activate(CLSCTX_ALL, None)
        }
    }

    pub fn get_volume() -> Option<(u32, bool)> {
        let v = endpoint().ok()?;
        unsafe {
            let level = v.GetMasterVolumeLevelScalar().ok()?;
            let muted = v.GetMute().map(|b| b.as_bool()).unwrap_or(false);
            Some(((level * 100.0).round() as u32, muted))
        }
    }

    pub fn set_volume(percent: u32) -> bool {
        let Ok(v) = endpoint() else { return false };
        unsafe {
            let _ = v.SetMute(false, std::ptr::null());
            v.SetMasterVolumeLevelScalar(percent.min(100) as f32 / 100.0, std::ptr::null()).is_ok()
        }
    }

    pub fn focused_is_text() -> bool {
        com();
        unsafe {
            let Ok(uia) = CoCreateInstance::<_, IUIAutomation>(&CUIAutomation, None, CLSCTX_INPROC_SERVER) else {
                return false;
            };
            let Ok(el) = uia.GetFocusedElement() else { return false };
            let Ok(ct) = el.CurrentControlType() else { return false };
            if ct == UIA_EditControlTypeId {
                return true;
            }
            if ct == UIA_ComboBoxControlTypeId {
                // Editable combo boxes (like address bars) expose a value pattern that isn't read-only.
                if let Ok(p) = el.GetCurrentPatternAs::<IUIAutomationValuePattern>(UIA_ValuePatternId) {
                    return p.CurrentIsReadOnly().map(|b| !b.as_bool()).unwrap_or(false);
                }
            }
            false
        }
    }
}

#[cfg(not(windows))]
mod imp {
    use super::MediaInfo;
    pub fn now_playing(_want_art: bool) -> Option<MediaInfo> {
        None
    }
    pub fn control(_cmd: &str) -> bool {
        false
    }
    pub fn get_volume() -> Option<(u32, bool)> {
        None
    }
    pub fn set_volume(_percent: u32) -> bool {
        false
    }
    pub fn focused_is_text() -> bool {
        false
    }
}

pub use imp::{control, focused_is_text, get_volume, now_playing, set_volume};

/// Standard base64 (for data: URLs).
pub fn base64(data: &[u8]) -> String {
    const T: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut out = String::with_capacity(data.len().div_ceil(3) * 4);
    for chunk in data.chunks(3) {
        let b = [chunk[0], *chunk.get(1).unwrap_or(&0), *chunk.get(2).unwrap_or(&0)];
        let n = ((b[0] as u32) << 16) | ((b[1] as u32) << 8) | b[2] as u32;
        out.push(T[(n >> 18) as usize & 63] as char);
        out.push(T[(n >> 12) as usize & 63] as char);
        out.push(if chunk.len() > 1 { T[(n >> 6) as usize & 63] as char } else { '=' });
        out.push(if chunk.len() > 2 { T[n as usize & 63] as char } else { '=' });
    }
    out
}

#[cfg(test)]
mod tests {
    #[test]
    fn base64_matches_reference() {
        assert_eq!(super::base64(b"Man"), "TWFu");
        assert_eq!(super::base64(b"Ma"), "TWE=");
        assert_eq!(super::base64(b"M"), "TQ==");
        assert_eq!(super::base64(b"hello world"), "aGVsbG8gd29ybGQ=");
    }
}
