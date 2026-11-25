//! In-app updates from the public GitHub releases of this project.

use serde::Serialize;
use serde_json::Value;
use std::io::Read;
use std::time::Duration;

const REPO: &str = "akkaaassh/console-mode";

#[derive(Serialize, Clone, Debug)]
pub struct Release {
    /// Build number from the "build-N" tag.
    pub build: u32,
    pub name: String,
    pub notes: String,
    pub url: String,
    pub size: u64,
    pub published: String,
}

fn agent() -> ureq::Agent {
    ureq::AgentBuilder::new().timeout(Duration::from_secs(20)).user_agent("ConsoleMode-updater").build()
}

/// The newest published build, if any.
pub fn latest() -> Result<Release, String> {
    let v: Value = agent()
        .get(&format!("https://api.github.com/repos/{REPO}/releases/latest"))
        .set("Accept", "application/vnd.github+json")
        .call()
        .map_err(|e| format!("Couldn't reach GitHub ({e})"))?
        .into_json()
        .map_err(|e| e.to_string())?;
    let tag = v.get("tag_name").and_then(|t| t.as_str()).unwrap_or("");
    let build = tag.trim_start_matches("build-").parse::<u32>().map_err(|_| format!("Unexpected release tag {tag}"))?;
    let asset = v
        .get("assets")
        .and_then(|a| a.as_array())
        .and_then(|a| a.iter().find(|x| x.get("name").and_then(|n| n.as_str()).is_some_and(|n| n.ends_with(".exe"))))
        .ok_or("The newest release has no installer")?;
    Ok(Release {
        build,
        name: v.get("name").and_then(|n| n.as_str()).unwrap_or(tag).to_string(),
        notes: v.get("body").and_then(|n| n.as_str()).unwrap_or("").to_string(),
        url: asset.get("browser_download_url").and_then(|u| u.as_str()).unwrap_or("").to_string(),
        size: asset.get("size").and_then(|s| s.as_u64()).unwrap_or(0),
        published: v.get("published_at").and_then(|p| p.as_str()).unwrap_or("").to_string(),
    })
}

/// Downloads the installer to the temp folder and returns its path.
pub fn download(url: &str, progress: impl Fn(u64, u64)) -> Result<std::path::PathBuf, String> {
    if !url.starts_with("https://github.com/") && !url.starts_with("https://objects.githubusercontent.com/") {
        return Err("Refusing to download from an unexpected address".into());
    }
    let resp = agent().get(url).call().map_err(|e| format!("Download failed ({e})"))?;
    let total: u64 = resp.header("Content-Length").and_then(|v| v.parse().ok()).unwrap_or(0);
    let mut reader = resp.into_reader();
    let path = std::env::temp_dir().join("ConsoleMode-update-setup.exe");
    let mut file = std::fs::File::create(&path).map_err(|e| e.to_string())?;
    let mut buf = [0u8; 64 * 1024];
    let mut done = 0u64;
    loop {
        let n = reader.read(&mut buf).map_err(|e| e.to_string())?;
        if n == 0 {
            break;
        }
        std::io::Write::write_all(&mut file, &buf[..n]).map_err(|e| e.to_string())?;
        done += n as u64;
        progress(done, total);
    }
    if done < 100_000 {
        return Err("The download looks incomplete".into());
    }
    Ok(path)
}

/// Runs the installer silently, then starts Console Mode again. The caller should exit right after.
pub fn install_and_restart(installer: &std::path::Path) -> Result<(), String> {
    let app = std::env::current_exe().map_err(|e| e.to_string())?;
    let script = format!(
        "Start-Sleep -Seconds 2; Start-Process -FilePath '{}' -ArgumentList '/S' -Wait; Start-Process -FilePath '{}'",
        installer.display().to_string().replace('\'', "''"),
        app.display().to_string().replace('\'', "''")
    );
    let mut cmd = std::process::Command::new("powershell.exe");
    cmd.args(["-NoProfile", "-WindowStyle", "Hidden", "-ExecutionPolicy", "Bypass", "-Command", &script]);
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        cmd.creation_flags(0x0800_0000 | 0x0000_0008); // no window, detached
    }
    cmd.spawn().map(|_| ()).map_err(|e| format!("Couldn't start the installer ({e})"))
}
