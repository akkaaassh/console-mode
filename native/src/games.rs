//! Installed-game scanning: Steam, Epic Games and GOG.

use crate::reg;
use crate::util::created_secs;
use serde::Serialize;
use std::fs;
use std::path::{Path, PathBuf};

#[derive(Serialize, Clone, Debug)]
pub struct Game {
    pub id: String,
    pub name: String,
    pub source: String,
    /// "uri" (ShellExecute a URI) or "exe" (run a program).
    pub launch_kind: String,
    pub launch_target: String,
    pub workdir: Option<String>,
    /// Install folder, when known (for "Open install folder").
    pub folder: Option<String>,
    /// Unix seconds; 0 when unknown.
    pub last_played: u64,
    /// Unix seconds; 0 when unknown.
    pub installed: u64,
    pub size_gb: Option<f64>,
    /// Local artwork paths (portrait cover and wide hero image), when found.
    pub cover: Option<String>,
    pub hero: Option<String>,
    /// The game's title logo, when found locally.
    pub logo: Option<String>,
    /// Steam app id, for fetching Steam's artwork.
    pub steam_appid: Option<String>,
}

pub fn scan() -> Vec<Game> {
    let mut v = steam();
    v.extend(epic());
    v.extend(gog());
    v.sort_by(|a, b| a.name.to_lowercase().cmp(&b.name.to_lowercase()));
    v.dedup_by(|a, b| a.id == b.id);
    v
}

/// Key/value pairs from a Valve KeyValues (.vdf/.acf) file, at any depth.
fn vdf_pairs(text: &str) -> Vec<(String, String)> {
    let mut tokens: Vec<Option<String>> = Vec::new(); // None marks a brace
    let mut chars = text.chars().peekable();
    while let Some(c) = chars.next() {
        match c {
            '"' => {
                let mut s = String::new();
                while let Some(c) = chars.next() {
                    match c {
                        '\\' => {
                            if let Some(n) = chars.next() {
                                s.push(n);
                            }
                        }
                        '"' => break,
                        _ => s.push(c),
                    }
                }
                tokens.push(Some(s));
            }
            '{' | '}' => tokens.push(None),
            _ => {}
        }
    }
    let mut out = Vec::new();
    let mut i = 0;
    while i + 1 < tokens.len() {
        if let (Some(k), Some(v)) = (&tokens[i], &tokens[i + 1]) {
            out.push((k.clone(), v.clone()));
            i += 2;
        } else {
            i += 1;
        }
    }
    out
}

fn first<'a>(pairs: &'a [(String, String)], key: &str) -> Option<&'a str> {
    pairs.iter().find(|(k, _)| k.eq_ignore_ascii_case(key)).map(|(_, v)| v.as_str())
}

const STEAM_SKIP_IDS: [&str; 6] = ["228980", "1070560", "1391110", "1493710", "1628350", "1826330"];
const STEAM_SKIP_WORDS: [&str; 8] = [
    "redistributable", "steamworks", "proton", "steam linux runtime", "sdk",
    "dedicated server", "soundtrack", "steamvr",
];

fn steam() -> Vec<Game> {
    let Some(root) = reg::read_str(reg::HKEY_CURRENT_USER, "Software\\Valve\\Steam", "SteamPath")
        .or_else(|| reg::read_str(reg::HKEY_LOCAL_MACHINE, "SOFTWARE\\WOW6432Node\\Valve\\Steam", "InstallPath"))
    else {
        return vec![];
    };
    let root = PathBuf::from(root.replace('/', "\\"));
    let mut libs = vec![root.clone()];
    for f in ["steamapps\\libraryfolders.vdf", "config\\libraryfolders.vdf"] {
        if let Ok(t) = fs::read_to_string(root.join(f)) {
            for (k, v) in vdf_pairs(&t) {
                if k.eq_ignore_ascii_case("path") {
                    let p = PathBuf::from(v.replace('/', "\\"));
                    if !libs.contains(&p) {
                        libs.push(p);
                    }
                }
            }
        }
    }
    let cache = root.join("appcache").join("librarycache");
    let mut out = Vec::new();
    for lib in libs {
        let Ok(rd) = fs::read_dir(lib.join("steamapps")) else { continue };
        for e in rd.flatten() {
            let fname = e.file_name().to_string_lossy().to_string();
            if !(fname.starts_with("appmanifest_") && fname.ends_with(".acf")) {
                continue;
            }
            let Ok(t) = fs::read_to_string(e.path()) else { continue };
            let p = vdf_pairs(&t);
            let (Some(appid), Some(name)) = (first(&p, "appid"), first(&p, "name")) else { continue };
            let lname = name.to_lowercase();
            if STEAM_SKIP_IDS.contains(&appid) || STEAM_SKIP_WORDS.iter().any(|w| lname.contains(w)) {
                continue;
            }
            let size_gb = first(&p, "SizeOnDisk")
                .and_then(|s| s.parse::<f64>().ok())
                .map(|b| (b / 1_073_741_824.0 * 10.0).round() / 10.0);
            out.push(Game {
                id: format!("steam:{appid}"),
                name: name.to_string(),
                source: "Steam".into(),
                launch_kind: "uri".into(),
                launch_target: format!("steam://rungameid/{appid}"),
                workdir: None,
                folder: first(&p, "installdir")
                    .map(|d| lib.join("steamapps").join("common").join(d))
                    .filter(|d| d.is_dir())
                    .map(|d| d.to_string_lossy().into_owned()),
                last_played: first(&p, "LastPlayed").and_then(|s| s.parse().ok()).unwrap_or(0),
                installed: created_secs(&e.path()),
                size_gb,
                cover: steam_art(&cache, appid, "library_600x900"),
                hero: steam_art(&cache, appid, "library_hero"),
                logo: steam_art(&cache, appid, "logo"),
                steam_appid: Some(appid.to_string()),
            });
        }
    }
    out
}

/// Finds Steam's cached artwork. Older clients use `<appid>_<kind>.jpg`;
/// newer ones use `<appid>/<kind>.jpg` or `<appid>/<hash>/<kind>.jpg`.
fn steam_art(cache: &Path, appid: &str, kind: &str) -> Option<String> {
    for ext in ["jpg", "png"] {
        let p = cache.join(format!("{appid}_{kind}.{ext}"));
        if p.is_file() {
            return Some(p.to_string_lossy().into_owned());
        }
    }
    let dir = cache.join(appid);
    let mut stack = vec![(dir, 0)];
    while let Some((d, depth)) = stack.pop() {
        let Ok(rd) = fs::read_dir(&d) else { continue };
        for e in rd.flatten() {
            let path = e.path();
            if path.is_dir() {
                if depth < 2 {
                    stack.push((path, depth + 1));
                }
                continue;
            }
            let n = e.file_name().to_string_lossy().to_lowercase();
            if n.starts_with(kind) && !n.contains("blur") && (n.ends_with(".jpg") || n.ends_with(".png")) {
                return Some(path.to_string_lossy().into_owned());
            }
        }
    }
    None
}

fn epic() -> Vec<Game> {
    let base = std::env::var("ProgramData").unwrap_or_else(|_| "C:\\ProgramData".into());
    let dir = Path::new(&base).join("Epic\\EpicGamesLauncher\\Data\\Manifests");
    let Ok(rd) = fs::read_dir(&dir) else { return vec![] };
    let mut out = Vec::new();
    for e in rd.flatten() {
        if e.path().extension().and_then(|x| x.to_str()) != Some("item") {
            continue;
        }
        let Ok(t) = fs::read_to_string(e.path()) else { continue };
        let Ok(j) = serde_json::from_str::<serde_json::Value>(&t) else { continue };
        let s = |k: &str| j.get(k).and_then(|v| v.as_str()).unwrap_or("").to_string();
        let (name, app, ns, item) = (s("DisplayName"), s("AppName"), s("CatalogNamespace"), s("CatalogItemId"));
        if name.is_empty() || app.is_empty() {
            continue;
        }
        if let Some(cats) = j.get("AppCategories").and_then(|v| v.as_array()) {
            if !cats.iter().any(|c| c.as_str() == Some("games")) {
                continue;
            }
        }
        let size_gb = j.get("InstallSize").and_then(|v| v.as_f64()).map(|b| (b / 1_073_741_824.0 * 10.0).round() / 10.0);
        out.push(Game {
            id: format!("epic:{app}"),
            name,
            source: "Epic".into(),
            launch_kind: "uri".into(),
            launch_target: format!("com.epicgames.launcher://apps/{ns}%3A{item}%3A{app}?action=launch&silent=true"),
            workdir: None,
            folder: Some(s("InstallLocation")).filter(|x| !x.is_empty()),
            last_played: 0,
            installed: created_secs(&e.path()),
            size_gb,
            cover: None,
            hero: None,
            logo: None,
            steam_appid: None,
        });
    }
    out
}

fn gog() -> Vec<Game> {
    let key = "SOFTWARE\\WOW6432Node\\GOG.com\\Games";
    let mut out = Vec::new();
    for id in reg::subkeys(reg::HKEY_LOCAL_MACHINE, key) {
        let sub = format!("{key}\\{id}");
        let name = reg::read_str(reg::HKEY_LOCAL_MACHINE, &sub, "gameName");
        let exe = reg::read_str(reg::HKEY_LOCAL_MACHINE, &sub, "exe");
        let (Some(name), Some(exe)) = (name, exe) else { continue };
        let workdir = reg::read_str(reg::HKEY_LOCAL_MACHINE, &sub, "workingDir")
            .or_else(|| reg::read_str(reg::HKEY_LOCAL_MACHINE, &sub, "path"));
        out.push(Game {
            id: format!("gog:{id}"),
            name,
            source: "GOG".into(),
            launch_kind: "exe".into(),
            installed: created_secs(Path::new(&exe)),
            launch_target: exe,
            folder: workdir.clone(),
            workdir,
            last_played: 0,
            size_gb: None,
            cover: None,
            hero: None,
            logo: None,
            steam_appid: None,
        });
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn parses_acf() {
        let t = r#""AppState" { "appid" "1091500" "name" "Cyberpunk 2077" "LastPlayed" "1700000000" "InstalledDepots" { "1" { "manifest" "2" } } }"#;
        let p = vdf_pairs(t);
        assert_eq!(first(&p, "appid"), Some("1091500"));
        assert_eq!(first(&p, "name"), Some("Cyberpunk 2077"));
        assert_eq!(first(&p, "LastPlayed"), Some("1700000000"));
    }
    #[test]
    fn parses_library_paths() {
        let t = r#""libraryfolders" { "0" { "path" "C:\\Program Files (x86)\\Steam" } "1" { "path" "D:\\SteamLibrary" } }"#;
        let paths: Vec<_> = vdf_pairs(t).into_iter().filter(|(k, _)| k == "path").map(|(_, v)| v).collect();
        assert_eq!(paths, vec!["C:\\Program Files (x86)\\Steam", "D:\\SteamLibrary"]);
    }
}
