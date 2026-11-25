//! Game artwork from the internet: Steam's own images for Steam games, and
//! SteamGridDB (free account + API key) for everything else.
//! Returns image URLs; the interface loads them directly.

use serde::Serialize;
use serde_json::Value;
use std::time::Duration;

#[derive(Serialize, Clone, Debug, Default)]
pub struct Art {
    /// Portrait cover (600×900).
    pub cover: Option<String>,
    /// Wide background.
    pub hero: Option<String>,
    /// The game's title logo (transparent).
    pub logo: Option<String>,
    /// Square icon.
    pub icon: Option<String>,
}

fn agent() -> ureq::Agent {
    ureq::AgentBuilder::new()
        .timeout(Duration::from_secs(12))
        .user_agent("ConsoleMode/0.3")
        .build()
}

fn exists(a: &ureq::Agent, url: &str) -> bool {
    a.head(url).call().map(|r| r.status() == 200).unwrap_or(false)
}

/// Steam's store images for a Steam app id (only the ones that exist).
pub fn steam(appid: &str) -> Art {
    let a = agent();
    let base = format!("https://cdn.cloudflare.steamstatic.com/steam/apps/{appid}");
    let pick = |names: &[&str]| names.iter().map(|n| format!("{base}/{n}")).find(|u| exists(&a, u));
    Art {
        cover: pick(&["library_600x900_2x.jpg", "library_600x900.jpg"]),
        hero: pick(&["library_hero.jpg"]),
        logo: pick(&["logo_2x.png", "logo.png"]),
        icon: None,
    }
}

fn encode(s: &str) -> String {
    let mut out = String::new();
    for b in s.bytes() {
        match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => out.push(b as char),
            _ => out.push_str(&format!("%{b:02X}")),
        }
    }
    out
}

fn first_url(v: &Value) -> Option<String> {
    v.get("data")?.as_array()?.first()?.get("url")?.as_str().map(str::to_string)
}

/// Looks a game or app up on SteamGridDB by name (or Steam app id) and returns its best images.
pub fn steamgriddb(key: &str, name: &str, steam_appid: Option<&str>) -> Result<Art, String> {
    let a = agent();
    let get = |path: &str| -> Result<Value, String> {
        let url = format!("https://www.steamgriddb.com/api/v2{path}");
        match a.get(&url).set("Authorization", &format!("Bearer {key}")).call() {
            Ok(r) => r.into_json::<Value>().map_err(|e| e.to_string()),
            Err(ureq::Error::Status(401, _)) => Err("SteamGridDB didn't accept the API key".into()),
            Err(ureq::Error::Status(404, _)) => Ok(Value::Null),
            Err(e) => Err(format!("Couldn't reach SteamGridDB ({e})")),
        }
    };
    let id = match steam_appid {
        Some(app) => get(&format!("/games/steam/{app}"))?
            .get("data")
            .and_then(|d| d.get("id"))
            .and_then(|i| i.as_i64()),
        None => None,
    };
    let id = match id {
        Some(i) => i,
        None => get(&format!("/search/autocomplete/{}", encode(name)))?
            .get("data")
            .and_then(|d| d.as_array())
            .and_then(|a| a.first())
            .and_then(|g| g.get("id"))
            .and_then(|i| i.as_i64())
            .ok_or_else(|| format!("SteamGridDB has nothing for “{name}”"))?,
    };
    Ok(Art {
        cover: get(&format!("/grids/game/{id}?dimensions=600x900&types=static"))
            .ok()
            .as_ref()
            .and_then(first_url),
        hero: get(&format!("/heroes/game/{id}?types=static")).ok().as_ref().and_then(first_url),
        logo: get(&format!("/logos/game/{id}?types=static")).ok().as_ref().and_then(first_url),
        icon: get(&format!("/icons/game/{id}?types=static")).ok().as_ref().and_then(first_url),
    })
}

#[cfg(test)]
mod tests {
    #[test]
    fn encodes_names() {
        assert_eq!(super::encode("Baldur's Gate 3"), "Baldur%27s%20Gate%203");
    }
}
