// Hide the console window in release builds.
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use cm_native as native;
use native::input::{PadEvent, Shared, MODE_CONSOLE, MODE_DESKTOP, MODE_GAME};
use std::sync::atomic::Ordering;
use std::sync::{Arc, Mutex};
use tauri::{AppHandle, Emitter, Manager, State};
use tauri_plugin_autostart::{MacosLauncher, ManagerExt};

struct AppState {
    shared: Arc<Shared>,
    stats: Mutex<native::stats::Stats>,
}

/// Shows the console full screen and gives it keyboard/controller focus.
fn bring_front(app: &AppHandle) {
    if let Some(w) = app.get_webview_window("main") {
        let _ = w.unminimize();
        let _ = w.show();
        let _ = w.set_fullscreen(true);
        let state = app.state::<AppState>();
        native::window::focus(state.shared.our_hwnd.load(Ordering::SeqCst));
        let _ = w.set_focus();
    }
}

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
        "exe" => native::launch::open(&target, None, workdir.as_deref()),
        "appid" => native::launch::apps_folder(&target),
        _ => false,
    };
    if ok {
        state.shared.prev_fg.store(0, Ordering::SeqCst);
        state.shared.mode.store(MODE_GAME, Ordering::SeqCst);
    }
    ok
}

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
fn go_desktop(app: AppHandle, state: State<AppState>) {
    state.shared.mode.store(MODE_DESKTOP, Ordering::SeqCst);
    if let Some(w) = app.get_webview_window("main") {
        let _ = w.set_fullscreen(false);
        let _ = w.minimize();
    }
}

/// Switches back to whatever was in front when Guide was pressed.
#[tauri::command]
fn resume_game(app: AppHandle, state: State<AppState>) -> bool {
    let h = state.shared.prev_fg.load(Ordering::SeqCst);
    if !native::window::exists(h) {
        return false;
    }
    state.shared.mode.store(MODE_GAME, Ordering::SeqCst);
    if let Some(w) = app.get_webview_window("main") {
        let _ = w.minimize();
    }
    native::window::focus(h);
    true
}

#[tauri::command]
fn quit_game(state: State<AppState>) -> bool {
    let h = state.shared.prev_fg.swap(0, Ordering::SeqCst);
    state.shared.mode.store(MODE_CONSOLE, Ordering::SeqCst);
    native::window::close(h)
}

#[tauri::command]
fn record_clip(app: AppHandle, state: State<AppState>) -> bool {
    let h = state.shared.prev_fg.load(Ordering::SeqCst);
    if !native::window::exists(h) {
        return false;
    }
    if let Some(w) = app.get_webview_window("main") {
        let _ = w.minimize();
    }
    state.shared.mode.store(MODE_GAME, Ordering::SeqCst);
    native::window::focus(h);
    std::thread::sleep(std::time::Duration::from_millis(500));
    native::keys::record_clip();
    true
}

#[tauri::command]
fn media(cmd: String) -> bool {
    native::media::key(&cmd)
}

#[tauri::command]
fn now_playing() -> Option<native::media::NowPlaying> {
    native::media::now_playing()
}

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

#[tauri::command]
fn autostart_get(app: AppHandle) -> bool {
    app.autolaunch().is_enabled().unwrap_or(false)
}

#[tauri::command]
fn autostart_set(app: AppHandle, on: bool) -> bool {
    let a = app.autolaunch();
    if on { a.enable().is_ok() } else { a.disable().is_ok() }
}

#[tauri::command]
fn open_keyboard() -> bool {
    native::launch::open("osk.exe", None, None)
}

#[tauri::command]
fn exit_app(app: AppHandle) {
    app.exit(0);
}

fn main() {
    let shared = Arc::new(Shared::default());
    let setup_shared = shared.clone();

    tauri::Builder::default()
        .plugin(tauri_plugin_single_instance::init(|app, _args, _cwd| bring_front(app)))
        .plugin(tauri_plugin_autostart::init(MacosLauncher::LaunchAgent, Some(vec!["--autostart"])))
        .manage(AppState { shared, stats: Mutex::new(native::stats::Stats::new()) })
        .setup(move |app| {
            #[cfg(windows)]
            if let Some(w) = app.get_webview_window("main") {
                if let Ok(h) = w.hwnd() {
                    setup_shared.our_hwnd.store(h.0 as isize, Ordering::SeqCst);
                }
            }
            let handle = app.handle().clone();
            native::input::spawn(setup_shared.clone(), move |ev| match ev {
                PadEvent::Guide => {
                    bring_front(&handle);
                    let _ = handle.emit("guide", ());
                }
                PadEvent::Desktop("home") => {
                    let st = handle.state::<AppState>();
                    st.shared.mode.store(MODE_CONSOLE, Ordering::SeqCst);
                    bring_front(&handle);
                    let _ = handle.emit("home", ());
                }
                PadEvent::Desktop(_) => {
                    native::launch::open("osk.exe", None, None);
                }
                PadEvent::Button(b) => {
                    let _ = handle.emit("pad", b);
                }
                PadEvent::Status(c) => {
                    let _ = handle.emit("padstatus", c);
                }
            });
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            scan_games,
            scan_apps,
            launch,
            set_mode,
            go_desktop,
            resume_game,
            quit_game,
            record_clip,
            media,
            now_playing,
            set_power,
            set_brightness,
            get_brightness,
            display_modes,
            set_display,
            stats,
            autostart_get,
            autostart_set,
            open_keyboard,
            exit_app
        ])
        .run(tauri::generate_context!())
        .expect("error while running Console Mode");
}
