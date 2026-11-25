// Hide the console window in release builds.
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use cm_native as native;
use native::input::{PadEvent, Shared, MODE_CONSOLE, MODE_DESKTOP, MODE_GAME};
use std::io::{Read, Write};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;
use tauri::menu::{Menu, MenuItem};
use tauri::tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent};
use tauri::{AppHandle, Emitter, Manager, PhysicalPosition, PhysicalSize, State, WebviewUrl, WebviewWindowBuilder};
use tauri_plugin_autostart::{MacosLauncher, ManagerExt};
use tauri_plugin_dialog::DialogExt;

struct AppState {
    shared: Arc<Shared>,
    stats: Mutex<native::stats::Stats>,
    /// Windows shown in the app switcher, and the selected one.
    sw: Mutex<Vec<native::capture::Win>>,
    sw_sel: AtomicUsize,
    sw_hwnd: std::sync::atomic::AtomicIsize,
}

/// This build's number (set by the GitHub build), for update checks.
const BUILD: u32 = match option_env!("CM_BUILD") {
    Some(_) => parse_build(),
    None => 0,
};
const fn parse_build() -> u32 {
    let b = match option_env!("CM_BUILD") {
        Some(s) => s.as_bytes(),
        None => b"0",
    };
    let mut n = 0u32;
    let mut i = 0;
    while i < b.len() {
        if b[i] >= b'0' && b[i] <= b'9' {
            n = n * 10 + (b[i] - b'0') as u32;
        }
        i += 1;
    }
    n
}

fn shared(app: &AppHandle) -> Arc<Shared> {
    app.state::<AppState>().shared.clone()
}

/// Shows the console full screen and gives it focus.
fn bring_front(app: &AppHandle) {
    hide_osk(app);
    if let Some(w) = app.get_webview_window("main") {
        let _ = w.unminimize();
        let _ = w.show();
        let _ = w.set_fullscreen(true);
        native::window::focus(shared(app).our_hwnd.load(Ordering::SeqCst));
        let _ = w.set_focus();
    }
}

/// The game (or other window) to go back to from the quick menu.
fn target_window(s: &Shared) -> isize {
    let prev = s.prev_fg.load(Ordering::SeqCst);
    if native::window::exists(prev) {
        return prev;
    }
    let game = s.game_hwnd.load(Ordering::SeqCst);
    if native::window::exists(game) {
        game
    } else {
        0
    }
}

// ---------- on-screen keyboard window ----------

fn show_osk(app: &AppHandle) {
    let s = shared(app);
    let h = s.osk_hwnd.load(Ordering::SeqCst);
    if h == 0 {
        return;
    }
    if let Some(w) = app.get_webview_window("osk") {
        let (mx, my, mw, mh) = native::window::monitor_rect(native::window::foreground());
        let kh = ((mh as f64) * 0.36).clamp(280.0, 460.0) as i32;
        let _ = w.set_size(PhysicalSize::new(mw as u32, kh as u32));
        let _ = w.set_position(PhysicalPosition::new(mx, my + mh - kh));
    }
    native::window::show_no_activate(h);
    s.osk_open.store(true, Ordering::SeqCst);
    let _ = app.emit_to("osk", "osk-open", ());
}

fn hide_osk(app: &AppHandle) {
    let s = shared(app);
    if s.osk_open.swap(false, Ordering::SeqCst) {
        native::window::hide(s.osk_hwnd.load(Ordering::SeqCst));
    }
}

#[tauri::command]
fn osk_key(key: String) {
    match key.as_str() {
        "Backspace" => native::keys::tap(native::keys::VK_BACK),
        "Enter" => native::keys::tap(native::keys::VK_RETURN),
        "Tab" => native::keys::tap(native::keys::VK_TAB),
        "Left" => native::keys::tap(native::keys::VK_LEFT),
        "Right" => native::keys::tap(native::keys::VK_RIGHT),
        _ => native::keys::type_text(&key),
    }
}

#[tauri::command]
fn osk_hide(app: AppHandle) {
    hide_osk(&app);
}

// ---------- library ----------

#[tauri::command]
async fn scan_games() -> Vec<native::games::Game> {
    native::games::scan()
}

#[tauri::command]
async fn scan_apps() -> Vec<native::apps::App> {
    native::apps::scan()
}

#[tauri::command]
fn launch(state: State<AppState>, kind: String, target: String, workdir: Option<String>) -> bool {
    let ok = match kind.as_str() {
        "uri" => native::launch::open(&target, None, None),
        "exe" => {
            let dir = workdir.or_else(|| {
                std::path::Path::new(&target).parent().map(|p| p.to_string_lossy().into_owned())
            });
            native::launch::open(&target, None, dir.as_deref())
        }
        "appid" => native::launch::apps_folder(&target),
        _ => false,
    };
    if ok {
        state.shared.prev_fg.store(0, Ordering::SeqCst);
        state.shared.game_hwnd.store(0, Ordering::SeqCst);
    }
    ok
}

#[tauri::command]
fn open_url(url: String) -> bool {
    native::launch::open(&url, None, None)
}

#[tauri::command]
fn open_folder(path: String) -> bool {
    native::launch::open("explorer.exe", Some(&format!("\"{path}\"")), None)
}

#[tauri::command]
async fn pick_exe(app: AppHandle) -> Option<String> {
    app.dialog()
        .file()
        .set_title("Add a game")
        .add_filter("Games and shortcuts", &["exe", "lnk", "url", "bat"])
        .blocking_pick_file()
        .map(|p| p.to_string())
}

// ---------- console / game / desktop ----------

#[tauri::command]
fn set_mode(state: State<AppState>, mode: String) {
    let m = match mode.as_str() {
        "game" => MODE_GAME,
        "desktop" => MODE_DESKTOP,
        _ => MODE_CONSOLE,
    };
    state.shared.mode.store(m, Ordering::SeqCst);
}

#[tauri::command]
fn go_desktop(app: AppHandle) {
    if let Some(w) = app.get_webview_window("main") {
        let _ = w.set_fullscreen(false);
        let _ = w.minimize();
    }
}

#[tauri::command]
fn resume_game(app: AppHandle, state: State<AppState>) -> bool {
    let h = target_window(&state.shared);
    if h == 0 {
        return false;
    }
    if let Some(w) = app.get_webview_window("main") {
        let _ = w.minimize();
    }
    native::window::focus(h);
    true
}

#[tauri::command]
fn quit_game(state: State<AppState>) -> bool {
    let h = target_window(&state.shared);
    state.shared.prev_fg.store(0, Ordering::SeqCst);
    state.shared.game_hwnd.store(0, Ordering::SeqCst);
    native::window::close(h)
}

#[tauri::command]
async fn record_clip(app: AppHandle) -> bool {
    let h = target_window(&shared(&app));
    if h == 0 {
        return false;
    }
    if let Some(w) = app.get_webview_window("main") {
        let _ = w.minimize();
    }
    native::window::focus(h);
    std::thread::sleep(Duration::from_millis(600));
    native::keys::record_clip();
    true
}

#[tauri::command]
fn set_cursor(state: State<AppState>, speed: u32, left_stick: bool) {
    state.shared.cursor_speed.store(speed.clamp(25, 300), Ordering::SeqCst);
    state.shared.left_stick_mouse.store(left_stick, Ordering::SeqCst);
}

// ---------- music ----------

#[tauri::command]
async fn media_info(art: bool) -> Option<native::winrt::MediaInfo> {
    native::winrt::now_playing(art).or_else(|| {
        native::media::now_playing().filter(|n| !n.title.is_empty()).map(|n| native::winrt::MediaInfo {
            title: n.title,
            artist: n.artist,
            source: n.source,
            playing: n.playing,
            ..Default::default()
        })
    })
}

#[tauri::command]
async fn media_control(cmd: String) -> bool {
    native::winrt::control(&cmd) || native::media::key(&cmd)
}

#[tauri::command]
async fn get_volume() -> Option<(u32, bool)> {
    native::winrt::get_volume()
}

#[tauri::command]
async fn set_volume(percent: u32) -> bool {
    native::winrt::set_volume(percent) || {
        native::media::key(if percent > 50 { "vol_up" } else { "vol_down" });
        false
    }
}

/// Waits (up to 5 minutes) for Spotify to send the browser back to us after sign-in.
#[tauri::command]
fn spotify_listen(app: AppHandle, port: u16) -> bool {
    let listener = match std::net::TcpListener::bind(("127.0.0.1", port)) {
        Ok(l) => l,
        // Already listening from an earlier attempt; that listener will answer.
        Err(_) => return true,
    };
    std::thread::spawn(move || {
        let _ = listener.set_nonblocking(true);
        let deadline = std::time::Instant::now() + Duration::from_secs(300);
        while std::time::Instant::now() < deadline {
            let Ok((mut s, _)) = listener.accept() else {
                std::thread::sleep(Duration::from_millis(150));
                continue;
            };
            let _ = s.set_nonblocking(false);
            let _ = s.set_read_timeout(Some(Duration::from_secs(5)));
            let mut buf = [0u8; 8192];
            let n = s.read(&mut buf).unwrap_or(0);
            let req = String::from_utf8_lossy(&buf[..n]).to_string();
            let path = req.lines().next().unwrap_or("").split_whitespace().nth(1).unwrap_or("").to_string();
            if !path.starts_with("/callback") {
                let _ = s.write_all(b"HTTP/1.1 404 Not Found\r\nContent-Length: 0\r\nConnection: close\r\n\r\n");
                continue;
            }
            let query = path.split_once('?').map(|x| x.1).unwrap_or("");
            let (mut code, mut st, mut err) = (String::new(), String::new(), String::new());
            for kv in query.split('&') {
                if let Some((k, v)) = kv.split_once('=') {
                    match k {
                        "code" => code = v.to_string(),
                        "state" => st = v.to_string(),
                        "error" => err = v.to_string(),
                        _ => {}
                    }
                }
            }
            let msg = if code.is_empty() {
                "Spotify didn't connect. Close this tab and try again from Console Mode."
            } else {
                "Spotify is connected. You can close this tab and go back to Console Mode."
            };
            let html = format!(
                "<!doctype html><meta charset=utf-8><title>Console Mode</title><body style=\"margin:0;font:18px system-ui,sans-serif;background:#05070d;color:#fff;display:grid;place-items:center;height:100vh\"><p>{msg}</p></body>"
            );
            let _ = s.write_all(
                format!(
                    "HTTP/1.1 200 OK\r\nContent-Type: text/html; charset=utf-8\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
                    html.len(),
                    html
                )
                .as_bytes(),
            );
            let _ = app.emit("spotify_code", serde_json::json!({ "code": code, "state": st, "error": err }));
            bring_front(&app);
            break;
        }
    });
    true
}

// ---------- TV & power ----------

#[tauri::command]
async fn set_power(mode: String) -> bool {
    native::power::set_mode(&mode)
}

#[tauri::command]
async fn set_brightness(value: u32) -> bool {
    native::power::set_brightness(value)
}

#[tauri::command]
async fn get_brightness() -> Option<u32> {
    native::power::get_brightness()
}

#[tauri::command]
fn display_modes() -> native::display::Modes {
    native::display::modes()
}

#[tauri::command]
fn set_display(w: u32, h: u32, hz: u32) -> bool {
    native::display::set(w, h, hz)
}

#[tauri::command]
fn stats(state: State<AppState>) -> native::stats::Sample {
    state.stats.lock().unwrap().sample()
}

// ---------- startup ----------

#[tauri::command]
fn autostart_get(app: AppHandle) -> bool {
    app.autolaunch().is_enabled().unwrap_or(false)
}

#[tauri::command]
fn autostart_set(app: AppHandle, on: bool) -> bool {
    let a = app.autolaunch();
    if on {
        native::reg::remove_startup_delay();
        a.enable().is_ok()
    } else {
        a.disable().is_ok()
    }
}

#[tauri::command]
fn open_keyboard(app: AppHandle) {
    show_osk(&app);
}

// ---------- power, connectivity, artwork, home screen ----------

#[tauri::command]
async fn power_action(what: String) -> bool {
    native::power::action(&what)
}

#[tauri::command]
async fn wifi_list(scan: bool) -> Vec<native::wifi::Network> {
    if scan && native::wifi::scan() {
        std::thread::sleep(Duration::from_millis(3500));
    }
    native::wifi::networks()
}

#[tauri::command]
async fn wifi_connect(ssid: String, password: Option<String>) -> Result<(), String> {
    native::wifi::connect(&ssid, password.as_deref().filter(|p| !p.is_empty()))
}

#[tauri::command]
async fn wifi_disconnect() -> bool {
    native::wifi::disconnect()
}

#[tauri::command]
async fn radio_get(kind: String) -> Option<bool> {
    native::winrt::radio(&kind)
}

#[tauri::command]
async fn radio_set(kind: String, on: bool) -> bool {
    native::winrt::set_radio(&kind, on)
}

#[tauri::command]
async fn bt_list(scan: bool) -> Vec<native::bluetooth::Device> {
    native::bluetooth::devices(scan)
}

#[tauri::command]
async fn bt_pair(address: String) -> Result<(), String> {
    native::bluetooth::pair(&address)
}

#[tauri::command]
async fn bt_connect(address: String) -> bool {
    native::bluetooth::connect(&address)
}

#[tauri::command]
async fn bt_disconnect(address: String) -> bool {
    native::bluetooth::disconnect(&address)
}

#[tauri::command]
async fn bt_forget(address: String) -> bool {
    native::bluetooth::forget(&address)
}

#[tauri::command]
async fn art_steam(appid: String) -> native::art::Art {
    native::art::steam(&appid)
}

#[tauri::command]
async fn art_sgdb(key: String, name: String, appid: Option<String>) -> Result<native::art::Art, String> {
    native::art::steamgriddb(&key, &name, appid.as_deref())
}

/// Home-screen mode: the console behaves like the desktop itself (no taskbar button,
/// not in Alt+Tab, sits behind apps you open) instead of being one more window.
#[tauri::command]
fn set_home_screen(app: AppHandle, on: bool) {
    if let Some(w) = app.get_webview_window("main") {
        let _ = w.set_skip_taskbar(on);
    }
    native::window::set_home_screen(shared(&app).our_hwnd.load(Ordering::SeqCst), on);
}

// ---------- app switcher ----------

fn sw_emit(app: &AppHandle) {
    let st = app.state::<AppState>();
    let list = st.sw.lock().unwrap().clone();
    let sel = st.sw_sel.load(Ordering::SeqCst);
    let _ = app.emit_to("switcher", "sw-state", serde_json::json!({ "list": list, "sel": sel }));
}

fn sw_open(app: &AppHandle) {
    let s = shared(app);
    if s.switcher_open.swap(true, Ordering::SeqCst) {
        return;
    }
    hide_osk(app);
    let st = app.state::<AppState>();
    let sw_h = st.sw_hwnd.load(Ordering::SeqCst);
    let ours = s.our_hwnd.load(Ordering::SeqCst);
    let mut list = native::capture::windows(&[sw_h, s.osk_hwnd.load(Ordering::SeqCst)]);
    // The console itself is hidden from Alt+Tab in home-screen mode; keep it in our switcher.
    if native::window::is_visible(ours) && !list.iter().any(|w| w.hwnd == ours) {
        let fg = native::window::foreground();
        let pos = if fg == ours { 0 } else { list.len().min(1) };
        list.insert(pos, native::capture::Win { hwnd: ours, title: "Console Mode".into(), process: "console-mode.exe".into(), icon: None });
    }
    let sel = if list.len() > 1 { 1 } else { 0 };
    *st.sw.lock().unwrap() = list.clone();
    st.sw_sel.store(sel, Ordering::SeqCst);
    if let Some(w) = app.get_webview_window("switcher") {
        let (mx, my, mw, mh) = native::window::monitor_rect(native::window::foreground());
        let _ = w.set_size(PhysicalSize::new(mw as u32, mh as u32));
        let _ = w.set_position(PhysicalPosition::new(mx, my));
    }
    native::window::show_no_activate(sw_h);
    sw_emit(app);
    let app2 = app.clone();
    std::thread::spawn(move || {
        for w in list {
            let icon = if w.hwnd == ours { None } else { native::capture::window_icon(w.hwnd) };
            let thumb = native::capture::window_thumb(w.hwnd, 360);
            let _ = app2.emit_to("switcher", "sw-item", serde_json::json!({ "hwnd": w.hwnd, "icon": icon, "thumb": thumb }));
        }
    });
}

fn sw_close(app: &AppHandle) {
    let st = app.state::<AppState>();
    shared(app).switcher_open.store(false, Ordering::SeqCst);
    native::window::hide(st.sw_hwnd.load(Ordering::SeqCst));
}

fn sw_key(app: &AppHandle, key: &str) {
    let st = app.state::<AppState>();
    match key {
        "open" => sw_open(app),
        "cancel" => sw_close(app),
        "commit" => {
            let list = st.sw.lock().unwrap().clone();
            let sel = st.sw_sel.load(Ordering::SeqCst);
            sw_close(app);
            if let Some(w) = list.get(sel) {
                if w.hwnd == shared(app).our_hwnd.load(Ordering::SeqCst) {
                    bring_front(app);
                } else {
                    native::window::focus(w.hwnd);
                }
            }
        }
        "X" => {
            let mut list = st.sw.lock().unwrap();
            let sel = st.sw_sel.load(Ordering::SeqCst);
            let ours = shared(app).our_hwnd.load(Ordering::SeqCst);
            if let Some(w) = list.get(sel).cloned() {
                if w.hwnd != ours && native::window::close(w.hwnd) {
                    list.remove(sel);
                    st.sw_sel.store(sel.min(list.len().saturating_sub(1)), Ordering::SeqCst);
                }
            }
            drop(list);
            sw_emit(app);
        }
        k => {
            let n = st.sw.lock().unwrap().len().max(1);
            let sel = st.sw_sel.load(Ordering::SeqCst);
            let next = match k {
                "Left" | "Up" | "LB" => (sel + n - 1) % n,
                _ => (sel + 1) % n,
            };
            st.sw_sel.store(next, Ordering::SeqCst);
            sw_emit(app);
        }
    }
}

#[tauri::command]
fn switcher_pick(app: AppHandle, index: usize) {
    app.state::<AppState>().sw_sel.store(index, Ordering::SeqCst);
    sw_key(&app, "commit");
}

#[tauri::command]
fn switcher_open(app: AppHandle) {
    sw_open(&app);
}

// ---------- extra buttons, controller test ----------

fn run_action(app: &AppHandle, action: &str) {
    match action {
        "switcher" => {
            if shared(app).switcher_open.load(Ordering::SeqCst) {
                sw_key(app, "commit")
            } else {
                sw_open(app)
            }
        }
        "quickmenu" => handle_pad(app, PadEvent::Guide),
        "home" => handle_pad(app, PadEvent::Desktop("home")),
        "desktop" => {
            if let Some(w) = app.get_webview_window("main") {
                let _ = w.minimize();
            }
        }
        "keyboard" => handle_pad(app, PadEvent::Desktop("keyboard")),
        "screenshot" => native::keys::screenshot(),
        "record" => native::keys::record_clip(),
        "mute" => {
            native::media::key("mute");
        }
        "playpause" => {
            if !native::winrt::control("play_pause") {
                native::media::key("play_pause");
            }
        }
        _ => {}
    }
}

#[tauri::command]
fn controller_debug() -> serde_json::Value {
    serde_json::json!({ "xinput": native::input::xinput_snapshot(), "raw": native::winrt::raw_pads() })
}

#[tauri::command]
fn set_extra_buttons(state: State<AppState>, map: Vec<(usize, String)>) {
    *state.shared.extra.lock().unwrap() = map;
}

// ---------- updates ----------

#[tauri::command]
async fn update_check() -> Result<serde_json::Value, String> {
    let r = native::update::latest()?;
    Ok(serde_json::json!({ "current": BUILD, "latest": r, "newer": r.build > BUILD }))
}

#[tauri::command]
async fn update_install(app: AppHandle, url: String) -> Result<(), String> {
    let a2 = app.clone();
    let path = native::update::download(&url, move |done, total| {
        let _ = a2.emit_to("main", "update-progress", serde_json::json!({ "done": done, "total": total }));
    })?;
    native::update::install_and_restart(&path)?;
    std::thread::sleep(Duration::from_millis(400));
    app.exit(0);
    Ok(())
}

#[tauri::command]
fn app_build() -> u32 {
    BUILD
}

// ---------- icons, web apps, artwork picking, tray ----------

#[tauri::command]
async fn app_icon(app_id: String) -> Option<String> {
    native::winrt::shell_icon(&format!("shell:AppsFolder\\{app_id}"), 128)
}

/// Icon of the app that is playing media (by its Windows app id).
#[tauri::command]
async fn source_icon(app_id: String) -> Option<String> {
    if let Some(i) = native::winrt::shell_icon(&format!("shell:AppsFolder\\{app_id}"), 96) {
        return Some(i);
    }
    let want = app_id.to_lowercase();
    let want = if want.ends_with(".exe") { want } else { format!("{want}.exe") };
    let alias = match want.as_str() {
        "msedge.exe" | "microsoftedge.exe" => "msedge.exe".to_string(),
        w => w.to_string(),
    };
    for w in native::capture::windows(&[]) {
        if w.process == alias {
            if let Some(p) = native::window::process_path(w.hwnd) {
                if let Some(i) = native::winrt::shell_icon(&p, 96) {
                    return Some(i);
                }
            }
            return native::capture::window_icon(w.hwnd);
        }
    }
    None
}

#[tauri::command]
fn open_webapp(url: String, fullscreen: bool) -> bool {
    native::webapp::open(&url, fullscreen)
}

#[tauri::command]
async fn art_search(key: String, query: String) -> Result<Vec<native::art::Match>, String> {
    native::art::search(&key, &query)
}

#[tauri::command]
async fn art_images(key: String, id: i64, kind: String) -> Result<Vec<native::art::Image>, String> {
    native::art::images(&key, id, &kind)
}

#[tauri::command]
async fn pick_image(app: AppHandle) -> Option<String> {
    app.dialog()
        .file()
        .set_title("Choose an image")
        .add_filter("Images", &["png", "jpg", "jpeg", "webp", "gif"])
        .blocking_pick_file()
        .map(|p| p.to_string())
}

/// Closes the console but keeps Console Mode running in the system tray:
/// the controller pointer, keyboard and Guide-to-reopen keep working.
#[tauri::command]
fn hide_to_tray(app: AppHandle) {
    if let Some(w) = app.get_webview_window("main") {
        let _ = w.set_fullscreen(false);
        let _ = w.hide();
    }
}

#[tauri::command]
fn exit_app(app: AppHandle) {
    app.exit(0);
}

fn handle_pad(app: &AppHandle, ev: PadEvent) {
    match ev {
        PadEvent::Guide => {
            bring_front(app);
            let _ = app.emit_to("main", "guide", ());
        }
        PadEvent::Desktop("home") => {
            bring_front(app);
            let _ = app.emit_to("main", "home", ());
        }
        PadEvent::Desktop(_) => {
            if shared(app).osk_open.load(Ordering::SeqCst) {
                hide_osk(app);
            } else {
                show_osk(app);
            }
        }
        PadEvent::Osk(b) => {
            let _ = app.emit_to("osk", "osk-pad", b);
        }
        PadEvent::Clicked => {
            let app = app.clone();
            let (x, y) = native::window::cursor_pos();
            std::thread::spawn(move || {
                std::thread::sleep(Duration::from_millis(250));
                let s = shared(&app);
                if !s.osk_open.load(Ordering::SeqCst)
                    && s.mode.load(Ordering::SeqCst) == MODE_DESKTOP
                    && native::winrt::focused_is_text(x, y)
                {
                    show_osk(&app);
                }
            });
        }
        PadEvent::Switcher(k) => sw_key(app, k),
        PadEvent::Extra(a) => run_action(app, &a),
        PadEvent::GameEnded => {
            let _ = app.emit_to("main", "game_ended", ());
        }
        PadEvent::Button(b) => {
            let _ = app.emit_to("main", "pad", b);
        }
        PadEvent::Status(c) => {
            let _ = app.emit("padstatus", c);
        }
    }
}

fn main() {
    let shared_state = Arc::new(Shared::default());
    let setup_shared = shared_state.clone();

    tauri::Builder::default()
        .plugin(tauri_plugin_single_instance::init(|app, _args, _cwd| bring_front(app)))
        .plugin(tauri_plugin_autostart::init(MacosLauncher::LaunchAgent, Some(vec!["--autostart"])))
        .plugin(tauri_plugin_dialog::init())
        .manage(AppState {
            shared: shared_state,
            stats: Mutex::new(native::stats::Stats::new()),
            sw: Mutex::new(Vec::new()),
            sw_sel: AtomicUsize::new(0),
            sw_hwnd: std::sync::atomic::AtomicIsize::new(0),
        })
        .setup(move |app| {
            #[cfg(windows)]
            if let Some(w) = app.get_webview_window("main") {
                if let Ok(h) = w.hwnd() {
                    setup_shared.our_hwnd.store(h.0 as isize, Ordering::SeqCst);
                }
            }

            // The on-screen keyboard lives in its own window that never takes focus,
            // so typed keys go to the app underneath.
            let osk = WebviewWindowBuilder::new(app, "osk", WebviewUrl::App("osk.html".into()))
                .title("Keyboard")
                .decorations(false)
                .always_on_top(true)
                .skip_taskbar(true)
                .resizable(false)
                .focused(false)
                .visible(false)
                .inner_size(1280.0, 340.0)
                .build()?;
            #[cfg(windows)]
            if let Ok(h) = osk.hwnd() {
                let h = h.0 as isize;
                native::window::make_no_activate(h);
                setup_shared.osk_hwnd.store(h, Ordering::SeqCst);
            }
            let _ = osk;

            // The app switcher: a see-through, never-focused window over everything.
            let sw = WebviewWindowBuilder::new(app, "switcher", WebviewUrl::App("switcher.html".into()))
                .title("Switch apps")
                .decorations(false)
                .transparent(true)
                .always_on_top(true)
                .skip_taskbar(true)
                .resizable(false)
                .focused(false)
                .visible(false)
                .inner_size(1280.0, 720.0)
                .build()?;
            #[cfg(windows)]
            if let Ok(h) = sw.hwnd() {
                let h = h.0 as isize;
                native::window::make_no_activate(h);
                app.state::<AppState>().sw_hwnd.store(h, Ordering::SeqCst);
            }
            let _ = sw;

            // Tray icon: reopen the console, or quit completely.
            let open_i = MenuItem::with_id(app, "open", "Open Console Mode", true, None::<&str>)?;
            let quit_i = MenuItem::with_id(app, "quit", "Quit completely", true, None::<&str>)?;
            let menu = Menu::with_items(app, &[&open_i, &quit_i])?;
            let mut tray = TrayIconBuilder::with_id("main")
                .tooltip("Console Mode")
                .menu(&menu)
                .show_menu_on_left_click(false)
                .on_menu_event(|app, e| match e.id.as_ref() {
                    "open" => bring_front(app),
                    "quit" => app.exit(0),
                    _ => {}
                })
                .on_tray_icon_event(|tray, e| {
                    if let TrayIconEvent::Click { button: MouseButton::Left, button_state: MouseButtonState::Up, .. } = e {
                        bring_front(tray.app_handle());
                    }
                });
            if let Some(icon) = app.default_window_icon() {
                tray = tray.icon(icon.clone());
            }
            tray.build(app)?;

            // Startup apps open sooner when Windows' startup delay is off.
            if app.autolaunch().is_enabled().unwrap_or(false) {
                native::reg::remove_startup_delay();
            }

            let handle = app.handle().clone();
            native::input::spawn(setup_shared.clone(), move |ev| handle_pad(&handle, ev));
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            scan_games,
            scan_apps,
            launch,
            open_url,
            open_folder,
            pick_exe,
            set_mode,
            go_desktop,
            resume_game,
            quit_game,
            record_clip,
            set_cursor,
            media_info,
            media_control,
            get_volume,
            set_volume,
            spotify_listen,
            set_power,
            set_brightness,
            get_brightness,
            display_modes,
            set_display,
            stats,
            autostart_get,
            autostart_set,
            open_keyboard,
            osk_key,
            osk_hide,
            power_action,
            wifi_list,
            wifi_connect,
            wifi_disconnect,
            radio_get,
            radio_set,
            bt_list,
            bt_pair,
            bt_connect,
            bt_disconnect,
            bt_forget,
            art_steam,
            art_sgdb,
            set_home_screen,
            switcher_pick,
            switcher_open,
            controller_debug,
            set_extra_buttons,
            update_check,
            update_install,
            app_build,
            app_icon,
            source_icon,
            open_webapp,
            art_search,
            art_images,
            pick_image,
            hide_to_tray,
            exit_app
        ])
        .run(tauri::generate_context!())
        .expect("error while running Console Mode");
}
