//! Entertainment apps from the Start menu (Store apps, web apps, desktop programs).

use crate::util::powershell;
use serde::{Deserialize, Serialize};

#[derive(Serialize, Clone, Debug)]
pub struct App {
    pub name: String,
    pub app_id: String,
}

#[derive(Deserialize)]
struct StartApp {
    #[serde(rename = "Name")]
    name: Option<String>,
    #[serde(rename = "AppID")]
    app_id: Option<String>,
}

/// Lower-case name fragments of streaming, music and media apps.
const KNOWN: [&str; 30] = [
    "netflix", "prime video", "amazon prime", "spotify", "youtube", "hotstar", "jiocinema",
    "vlc", "plex", "apple tv", "apple music", "twitch", "disney", "crunchyroll", "zee5",
    "sonyliv", "sony liv", "mx player", "kodi", "stremio", "amazon music", "jiosaavn", "hulu",
    "peacock", "tidal", "deezer", "soundcloud", "paramount+", "mubi", "jellyfin",
];
const SKIP: [&str; 6] = ["uninstall", "help", "readme", "website", "update", "settings"];

pub fn is_entertainment(name: &str) -> bool {
    let n = name.to_lowercase();
    KNOWN.iter().any(|k| n.contains(k)) && !SKIP.iter().any(|s| n.contains(s))
}

pub fn scan() -> Vec<App> {
    let Some(json) = powershell("Get-StartApps | Select-Object Name,AppID | ConvertTo-Json -Compress") else {
        return vec![];
    };
    parse(&json)
}

fn parse(json: &str) -> Vec<App> {
    // ConvertTo-Json gives an object for one result and an array for many.
    let items: Vec<StartApp> = serde_json::from_str::<Vec<StartApp>>(json)
        .or_else(|_| serde_json::from_str::<StartApp>(json).map(|a| vec![a]))
        .unwrap_or_default();
    let mut out: Vec<App> = Vec::new();
    for it in items {
        let (Some(name), Some(app_id)) = (it.name, it.app_id) else { continue };
        if !is_entertainment(&name) || out.iter().any(|a| a.name.eq_ignore_ascii_case(&name)) {
            continue;
        }
        out.push(App { name, app_id });
    }
    out.sort_by(|a, b| a.name.to_lowercase().cmp(&b.name.to_lowercase()));
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn filters_and_parses() {
        let j = r#"[{"Name":"Netflix","AppID":"4DF9E0F8.Netflix_mcm4njqhnhss8!Netflix.App"},{"Name":"Calculator","AppID":"calc"},{"Name":"Spotify","AppID":"Spotify.exe"},{"Name":"Uninstall VLC","AppID":"x"},{"Name":"VLC media player","AppID":"vlc"}]"#;
        let v = parse(j);
        let names: Vec<_> = v.iter().map(|a| a.name.as_str()).collect();
        assert_eq!(names, vec!["Netflix", "Spotify", "VLC media player"]);
        assert_eq!(parse(r#"{"Name":"Plex","AppID":"p"}"#).len(), 1);
    }
}
