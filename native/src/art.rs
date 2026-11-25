//! Game artwork from the internet: Steam's official store images (for any game on Steam,
//! found by name if needed), Epic's own images for Epic games, and SteamGridDB as a fallback.
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

/// Steam's store images for a Steam app id. Asks the store for the current (hashed) asset
/// names first, which newer games need, then falls back to the classic file names.
pub fn steam(appid: &str) -> Art {
    let a = agent();
    let mut art = steam_assets(&a, appid).unwrap_or_default();
    // Only keep images that are really there.
    for f in [&mut art.cover, &mut art.hero, &mut art.logo] {
        if f.as_deref().map_or(false, |u| !exists(&a, u)) {
            *f = None;
        }
    }
    if art.cover.is_some() && art.hero.is_some() && art.logo.is_some() {
        return art;
    }
    let base = format!("https://cdn.cloudflare.steamstatic.com/steam/apps/{appid}");
    let pick = |names: &[&str]| names.iter().map(|n| format!("{base}/{n}")).find(|u| exists(&a, u));
    if art.cover.is_none() {
        art.cover = pick(&["library_600x900_2x.jpg", "library_600x900.jpg"]);
    }
    if art.hero.is_none() {
        art.hero = pick(&["library_hero.jpg"]);
    }
    if art.logo.is_none() {
        art.logo = pick(&["logo_2x.png", "logo.png"]);
    }
    art
}

/// Current library images from Steam's store service (no key needed).
fn steam_assets(a: &ureq::Agent, appid: &str) -> Option<Art> {
    let id: u64 = appid.parse().ok()?;
    let input = format!(
        r#"{{"ids":[{{"appid":{id}}}],"context":{{"language":"english","country_code":"US"}},"data_request":{{"include_assets":true}}}}"#
    );
    let url = format!("https://api.steampowered.com/IStoreBrowseService/GetItems/v1/?input_json={}", encode(&input));
    let v: Value = a.get(&url).call().ok()?.into_json().ok()?;
    let assets = v.get("response")?.get("store_items")?.as_array()?.first()?.get("assets")?;
    let fmt = assets.get("asset_url_format")?.as_str()?;
    let file = |keys: &[&str]| {
        keys.iter()
            .find_map(|k| assets.get(*k).and_then(|x| x.as_str()))
            .map(|f| format!("https://shared.akamai.steamstatic.com/store_item_assets/{}", fmt.replace("${FILENAME}", f)))
    };
    Some(Art {
        cover: file(&["library_capsule_2x", "library_capsule"]),
        hero: file(&["library_hero_2x", "library_hero"]),
        logo: file(&["library_logo_2x", "library_logo"]),
        icon: None,
    })
}

/// Squashes a title for comparing: lower case, letters and digits only.
pub fn norm(s: &str) -> String {
    s.chars().filter(|c| c.is_alphanumeric()).flat_map(|c| c.to_lowercase()).collect()
}

/// Games on the Steam store matching a name (no key needed). Works for games bought anywhere.
pub fn steam_search(query: &str) -> Vec<Match> {
    let url = format!("https://store.steampowered.com/api/storesearch/?term={}&l=english&cc=US", encode(query));
    let Ok(r) = agent().get(&url).call() else { return vec![] };
    let Ok(v) = r.into_json::<Value>() else { return vec![] };
    v.get("items")
        .and_then(|i| i.as_array())
        .map(|a| {
            a.iter()
                .filter(|x| x.get("type").and_then(|t| t.as_str()).map_or(true, |t| t == "app"))
                .take(10)
                .filter_map(|x| Some(Match { id: x.get("id")?.as_i64()?, name: x.get("name")?.as_str()?.to_string(), year: None }))
                .collect()
        })
        .unwrap_or_default()
}

/// The Steam game that best matches a name: same title, or one title containing the other.
/// Returns nothing rather than guess wrong.
pub fn steam_find(name: &str) -> Option<Match> {
    let want = norm(name);
    if want.len() < 2 {
        return None;
    }
    let list = steam_search(name);
    let skip = |n: &str| ["soundtrack", "dlc", "demo", "artbook", "season pass"].iter().any(|w| n.to_lowercase().contains(w));
    if let Some(m) = list.iter().find(|m| norm(&m.name) == want) {
        return Some(m.clone());
    }
    list.into_iter().find(|m| {
        let n = norm(&m.name);
        !skip(&m.name) && (n.ends_with(&want) || want.ends_with(&n) || (want.len() >= 6 && n.contains(&want)))
    })
}

/// Artwork Epic's launcher already has for a game you own there (from its local catalog cache).
pub fn epic(item_id: &str) -> Option<Art> {
    let base = std::env::var("ProgramData").unwrap_or_else(|_| "C:\\ProgramData".into());
    let path = std::path::Path::new(&base).join("Epic\\EpicGamesLauncher\\Data\\Catalog\\catcache.bin");
    let raw = std::fs::read(path).ok()?;
    let json = b64_decode(&raw)?;
    let v: Value = serde_json::from_slice(&json).ok()?;
    let item = v.as_array()?.iter().find(|x| x.get("id").and_then(|i| i.as_str()) == Some(item_id))?;
    let imgs = item.get("keyImages")?.as_array()?;
    let pick = |types: &[&str]| {
        types.iter().find_map(|t| {
            imgs.iter()
                .find(|i| i.get("type").and_then(|x| x.as_str()) == Some(*t))
                .and_then(|i| i.get("url")?.as_str().map(str::to_string))
        })
    };
    let art = Art {
        cover: pick(&["DieselGameBoxTall", "OfferImageTall", "Thumbnail"]),
        hero: pick(&["DieselGameBox", "OfferImageWide", "DieselStoreFrontWide"]),
        logo: pick(&["DieselGameBoxLogo"]),
        icon: None,
    };
    (art.cover.is_some() || art.hero.is_some()).then_some(art)
}

/// Plain base64 decoding (ignores whitespace).
pub fn b64_decode(input: &[u8]) -> Option<Vec<u8>> {
    let val = |c: u8| -> Option<u32> {
        Some(match c {
            b'A'..=b'Z' => c - b'A',
            b'a'..=b'z' => c - b'a' + 26,
            b'0'..=b'9' => c - b'0' + 52,
            b'+' | b'-' => 62,
            b'/' | b'_' => 63,
            _ => return None,
        } as u32)
    };
    let mut out = Vec::with_capacity(input.len() * 3 / 4);
    let (mut acc, mut bits) = (0u32, 0u32);
    for &c in input {
        if c == b'=' || c.is_ascii_whitespace() {
            continue;
        }
        acc = (acc << 6) | val(c)?;
        bits += 6;
        if bits >= 8 {
            bits -= 8;
            out.push((acc >> bits) as u8);
            acc &= (1 << bits) - 1;
        }
    }
    Some(out)
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

#[derive(Serialize, Clone, Debug)]
pub struct Match {
    pub id: i64,
    pub name: String,
    pub year: Option<i64>,
}

fn sgdb_get(key: &str, path: &str) -> Result<Value, String> {
    let url = format!("https://www.steamgriddb.com/api/v2{path}");
    match agent().get(&url).set("Authorization", &format!("Bearer {key}")).call() {
        Ok(r) => r.into_json::<Value>().map_err(|e| e.to_string()),
        Err(ureq::Error::Status(401, _)) => Err("SteamGridDB didn't accept the API key".into()),
        Err(ureq::Error::Status(404, _)) => Ok(Value::Null),
        Err(e) => Err(format!("Couldn't reach SteamGridDB ({e})")),
    }
}

/// Games on SteamGridDB matching a name, for picking the right one.
pub fn search(key: &str, query: &str) -> Result<Vec<Match>, String> {
    let v = sgdb_get(key, &format!("/search/autocomplete/{}", encode(query)))?;
    Ok(v.get("data")
        .and_then(|d| d.as_array())
        .map(|a| {
            a.iter()
                .take(12)
                .filter_map(|g| {
                    Some(Match {
                        id: g.get("id")?.as_i64()?,
                        name: g.get("name")?.as_str()?.to_string(),
                        year: g.get("release_date").and_then(|r| r.as_i64()).map(|t| 1970 + t / 31_556_952),
                    })
                })
                .collect()
        })
        .unwrap_or_default())
}

#[derive(Serialize, Clone, Debug)]
pub struct Image {
    pub url: String,
    pub thumb: String,
}

/// All images of one kind ("grids", "heroes", "logos", "icons") for a SteamGridDB game id.
pub fn images(key: &str, id: i64, kind: &str) -> Result<Vec<Image>, String> {
    let q = match kind {
        "grids" => "?dimensions=600x900&types=static",
        _ => "?types=static",
    };
    let v = sgdb_get(key, &format!("/{kind}/game/{id}{q}"))?;
    // Newest uploads first (SteamGridDB ids grow over time).
    let mut list: Vec<Value> = v.get("data").and_then(|d| d.as_array()).cloned().unwrap_or_default();
    list.sort_by_key(|x| std::cmp::Reverse(x.get("id").and_then(|i| i.as_i64()).unwrap_or(0)));
    let v = serde_json::json!({ "data": list });
    Ok(v.get("data")
        .and_then(|d| d.as_array())
        .map(|a| {
            a.iter()
                .take(40)
                .filter_map(|x| {
                    let url = x.get("url")?.as_str()?.to_string();
                    let thumb = x.get("thumb").and_then(|t| t.as_str()).unwrap_or(&url).to_string();
                    Some(Image { url, thumb })
                })
                .collect()
        })
        .unwrap_or_default())
}

#[cfg(test)]
mod tests_v4 {
    use super::*;
    #[test]
    fn base64_and_names() {
        assert_eq!(b64_decode(b"W3siaWQiOiJ4In1d").unwrap(), br#"[{"id":"x"}]"#.to_vec());
        assert_eq!(b64_decode(b"aGk=\n").unwrap(), b"hi".to_vec());
        assert_eq!(norm("Marvel’s Spider-Man 2™"), "marvelsspiderman2");
    }
}
