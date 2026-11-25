// Hide the console window in release builds.
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use cm_native as native;
use native::input::{PadEvent, Shared, MODE_CONSOLE, MODE_DESKTOP, MODE_GAME};
use std::io::{Read, Write};
use std::sync::atomic::Ordering;
use std::sync::{Arc, Mutex};
use std::time::Duration;
use tauri::{AppHandle, Emitter, Manager, PhysicalPosition, PhysicalSize, State, WebviewUrl, WebviewWindowBuilder};
use tauri_plugin_autostart::{MacosLauncher, ManagerExt};
use tauri_plugin_dialog::DialogExt;

struct AppState {
    shared: Arc<Shared>,
    stats: Mutex<native::stats::Stats>,
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
        .manage(AppState { shared: shared_state, stats: Mutex::new(native::stats::Stats::new()) })
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
            exit_app
        ])
        .run(tauri::generate_context!())
        .expect("error while running Console Mode");
}
