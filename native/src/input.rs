//! Controller input thread.
//!
//! Polls XInput (Xbox-style pads, including third-party pads in XInput mode)
//! about 120 times a second and decides what each button means, based on
//! what is in front:
//! - Console (our window): buttons go to the UI. No pointer: it is driven like a console.
//! - Game (a full-screen game, or the game we launched): the pad belongs to the
//!   game. Only Guide (or View + Menu) is used, to open the quick menu.
//! - Desktop (everything else, including Netflix, browsers and windowed apps):
//!   the right stick moves the pointer (hold LT for precision), the left stick scrolls
//!   (or moves the pointer too, if chosen), A clicks (hold to drag), B right-clicks,
//!   D-pad up/down scrolls, Y opens the keyboard, View returns to the console.
//!   While the on-screen keyboard is open, the buttons drive the keyboard.

use crate::keys;
use crate::util::wide;
use crate::window;
use std::sync::atomic::{AtomicBool, AtomicIsize, AtomicU32, AtomicU8, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};
use windows_sys::Win32::System::LibraryLoader::{GetProcAddress, LoadLibraryW};
use windows_sys::Win32::UI::Input::XboxController::XINPUT_STATE;

pub const MODE_CONSOLE: u8 = 0;
pub const MODE_GAME: u8 = 1;
pub const MODE_DESKTOP: u8 = 2;

pub struct Shared {
    /// Kept for the UI; the input thread works out the mode itself.
    pub mode: AtomicU8,
    /// Our main window.
    pub our_hwnd: AtomicIsize,
    /// The on-screen keyboard window.
    pub osk_hwnd: AtomicIsize,
    /// True while the on-screen keyboard is showing on the desktop.
    pub osk_open: AtomicBool,
    /// The window that was in front when Guide was pressed (usually the game).
    pub prev_fg: AtomicIsize,
    /// The game window we have seen running, once detected.
    pub game_hwnd: AtomicIsize,
    /// Pointer speed in percent (25–300).
    pub cursor_speed: AtomicU32,
    /// Left stick on the desktop: true = moves the pointer, false = scrolls.
    pub left_stick_mouse: AtomicBool,
    /// True while the app switcher is on screen (buttons drive it).
    pub switcher_open: AtomicBool,
    /// Extra controller buttons (raw button index) and the action each one runs.
    pub extra: Mutex<Vec<(usize, String)>>,
}

impl Default for Shared {
    fn default() -> Self {
        Shared {
            mode: AtomicU8::new(MODE_CONSOLE),
            our_hwnd: AtomicIsize::new(0),
            osk_hwnd: AtomicIsize::new(0),
            osk_open: AtomicBool::new(false),
            prev_fg: AtomicIsize::new(0),
            game_hwnd: AtomicIsize::new(0),
            cursor_speed: AtomicU32::new(100),
            left_stick_mouse: AtomicBool::new(false),
            switcher_open: AtomicBool::new(false),
            extra: Mutex::new(Vec::new()),
        }
    }
}

#[derive(Clone, Debug)]
pub enum PadEvent {
    /// A button press for the console UI (de-bounced, with auto-repeat on directions).
    Button(&'static str),
    /// A button press for the on-screen keyboard.
    Osk(&'static str),
    /// Guide, or View + Menu together.
    Guide,
    /// Controller connected / disconnected.
    Status(bool),
    /// Desktop actions the shell handles: "home" or "keyboard".
    Desktop(&'static str),
    /// The controller clicked something on the desktop (check for a text field).
    Clicked,
    /// The game window we were tracking has closed.
    GameEnded,
    /// App switcher: "open", "commit", "cancel", or a button name to move or close.
    Switcher(&'static str),
    /// An extra controller button mapped to an action.
    Extra(String),
}

/// Apps that may go full screen but should still get the controller pointer.
const POINTER_APPS: [&str; 26] = [
    "chrome.exe", "msedge.exe", "firefox.exe", "brave.exe", "opera.exe", "vivaldi.exe",
    "applicationframehost.exe", "wwahost.exe", "msedgewebview2.exe", "vlc.exe", "spotify.exe",
    "discord.exe", "plex.exe", "plex htpc.exe", "kodi.exe", "explorer.exe", "steam.exe",
    "steamwebhelper.exe", "epicgameslauncher.exe", "galaxyclient.exe", "mpc-hc64.exe",
    "potplayermini64.exe", "console-mode.exe", "netflix.exe", "photos.exe", "stremio.exe",
];

type GetStateFn = unsafe extern "system" fn(u32, *mut XINPUT_STATE) -> u32;

/// Loads XInputGetStateEx (ordinal 100), which also reports the Guide button.
fn load_get_state() -> Option<GetStateFn> {
    for dll in ["xinput1_4.dll", "xinput1_3.dll"] {
        unsafe {
            let lib = LoadLibraryW(wide(dll).as_ptr());
            if lib.is_null() {
                continue;
            }
            if let Some(f) = GetProcAddress(lib, 100usize as *const u8) {
                return Some(std::mem::transmute::<unsafe extern "system" fn() -> isize, GetStateFn>(f));
            }
            if let Some(f) = GetProcAddress(lib, b"XInputGetState\0".as_ptr()) {
                return Some(std::mem::transmute::<unsafe extern "system" fn() -> isize, GetStateFn>(f));
            }
        }
    }
    None
}

const B_A: u32 = 1 << 0;
const B_B: u32 = 1 << 1;
const B_X: u32 = 1 << 2;
const B_Y: u32 = 1 << 3;
const B_LB: u32 = 1 << 4;
const B_RB: u32 = 1 << 5;
const B_LT: u32 = 1 << 6;
const B_RT: u32 = 1 << 7;
const B_VIEW: u32 = 1 << 8;
const B_MENU: u32 = 1 << 9;
const B_L3: u32 = 1 << 10;
const B_R3: u32 = 1 << 11;
const B_UP: u32 = 1 << 12;
const B_DOWN: u32 = 1 << 13;
const B_LEFT: u32 = 1 << 14;
const B_RIGHT: u32 = 1 << 15;
const B_GUIDE: u32 = 1 << 16;

const NAMES: [(u32, &str); 17] = [
    (B_A, "A"), (B_B, "B"), (B_X, "X"), (B_Y, "Y"), (B_LB, "LB"), (B_RB, "RB"),
    (B_LT, "LT"), (B_RT, "RT"), (B_VIEW, "View"), (B_MENU, "Menu"), (B_L3, "L3"),
    (B_R3, "R3"), (B_UP, "Up"), (B_DOWN, "Down"), (B_LEFT, "Left"), (B_RIGHT, "Right"),
    (B_GUIDE, "Guide"),
];
const DIRS: u32 = B_UP | B_DOWN | B_LEFT | B_RIGHT;

fn bits_from(st: &XINPUT_STATE) -> u32 {
    let w = st.Gamepad.wButtons as u32;
    let map: [(u32, u32); 15] = [
        (0x1000, B_A), (0x2000, B_B), (0x4000, B_X), (0x8000, B_Y),
        (0x0100, B_LB), (0x0200, B_RB), (0x0020, B_VIEW), (0x0010, B_MENU),
        (0x0040, B_L3), (0x0080, B_R3), (0x0001, B_UP), (0x0002, B_DOWN),
        (0x0004, B_LEFT), (0x0008, B_RIGHT), (0x0400, B_GUIDE),
    ];
    let mut b = 0;
    for (mask, bit) in map {
        if w & mask != 0 {
            b |= bit;
        }
    }
    if st.Gamepad.bLeftTrigger > 100 {
        b |= B_LT;
    }
    if st.Gamepad.bRightTrigger > 100 {
        b |= B_RT;
    }
    b
}

fn axis(v: i16) -> f32 {
    (v as f32 / 32767.0).clamp(-1.0, 1.0)
}

/// Stick to pointer speed: dead zone, then a curve so small pushes are precise.
fn stick_speed(x: f32, y: f32, scale: f32) -> (f32, f32) {
    let m = (x * x + y * y).sqrt();
    const DEAD: f32 = 0.16;
    if m < DEAD {
        return (0.0, 0.0);
    }
    let n = ((m - DEAD) / (1.0 - DEAD)).min(1.0);
    let speed = (1500.0 * n * n + 120.0 * n) * scale; // pixels per second
    (x / m * speed, -y / m * speed) // XInput Y points up; screen Y points down
}

/// Works out who owns the controller right now.
fn classify(shared: &Shared, fg: isize) -> u8 {
    let ours = shared.our_hwnd.load(Ordering::SeqCst);
    if ours != 0 && fg == ours {
        return MODE_CONSOLE;
    }
    if fg != 0 && fg == shared.osk_hwnd.load(Ordering::SeqCst) {
        return MODE_DESKTOP;
    }
    let game = shared.game_hwnd.load(Ordering::SeqCst);
    if game != 0 && fg == game {
        return MODE_GAME;
    }
    if window::is_shell(fg) || !window::is_fullscreen(fg) {
        return MODE_DESKTOP;
    }
    let proc = window::process_name(fg);
    if proc.is_empty() || POINTER_APPS.contains(&proc.as_str()) {
        return MODE_DESKTOP;
    }
    // A full-screen window from anything else is treated as a game.
    shared.game_hwnd.store(fg, Ordering::SeqCst);
    MODE_GAME
}

pub fn spawn<F>(shared: Arc<Shared>, cb: F)
where
    F: Fn(PadEvent) + Send + 'static,
{
    std::thread::Builder::new()
        .name("controller".into())
        .spawn(move || run(shared, cb))
        .expect("controller thread");
}

fn run<F: Fn(PadEvent)>(shared: Arc<Shared>, cb: F) {
    let Some(get) = load_get_state() else {
        cb(PadEvent::Status(false));
        return;
    };
    let mut slot: Option<u32> = None;
    let mut connected = false;
    let mut last_scan = Instant::now() - Duration::from_secs(10);
    let mut prev: u32 = 0;
    let mut repeat_at: [Option<Instant>; 4] = [None; 4];
    let mut chord_used = false;
    let mut acc = (0.0f32, 0.0f32);
    let mut scroll_acc = (0.0f32, 0.0f32);
    let mut view_at: Option<Instant> = None;
    let mut sw_hold = false;
    let mut raw_prev: Vec<bool> = Vec::new();
    let mut last = Instant::now();
    let mut left_down = false;
    let mut mode = MODE_CONSOLE;
    let mut last_fg: isize = -1;
    let mut last_classify = Instant::now() - Duration::from_secs(1);

    loop {
        std::thread::sleep(Duration::from_millis(8));
        let now = Instant::now();
        let dt = now.duration_since(last).as_secs_f32().min(0.05);
        last = now;

        // Notice when the tracked game window goes away.
        let game = shared.game_hwnd.load(Ordering::SeqCst);
        if game != 0 && !window::exists(game) {
            shared.game_hwnd.store(0, Ordering::SeqCst);
            cb(PadEvent::GameEnded);
        }

        let mut st: XINPUT_STATE = unsafe { std::mem::zeroed() };
        let ok = match slot {
            Some(s) => (unsafe { get(s, &mut st) }) == 0,
            None => false,
        };
        if !ok {
            if connected {
                connected = false;
                prev = 0;
                if left_down {
                    keys::mouse_left(false);
                    left_down = false;
                }
                cb(PadEvent::Status(false));
            }
            slot = None;
            if now.duration_since(last_scan) > Duration::from_secs(1) {
                last_scan = now;
                for i in 0..4u32 {
                    let mut probe: XINPUT_STATE = unsafe { std::mem::zeroed() };
                    if (unsafe { get(i, &mut probe) }) == 0 {
                        slot = Some(i);
                        break;
                    }
                }
            }
            if slot.is_none() {
                std::thread::sleep(Duration::from_millis(250));
            }
            continue;
        }
        if !connected {
            connected = true;
            cb(PadEvent::Status(true));
        }

        let fg = window::foreground();
        if fg != last_fg || now.duration_since(last_classify) > Duration::from_millis(500) {
            mode = classify(&shared, fg);
            last_fg = fg;
            last_classify = now;
            shared.mode.store(mode, Ordering::SeqCst);
        }
        let ours = shared.our_hwnd.load(Ordering::SeqCst);
        let osk = mode == MODE_DESKTOP && shared.osk_open.load(Ordering::SeqCst);
        let desk = mode == MODE_DESKTOP && !osk;
        let left_mouse = desk && shared.left_stick_mouse.load(Ordering::SeqCst);
        let left_scroll = desk && !left_mouse;

        let (lx, ly) = (axis(st.Gamepad.sThumbLX), axis(st.Gamepad.sThumbLY));
        let (rx, ry) = (axis(st.Gamepad.sThumbRX), axis(st.Gamepad.sThumbRY));
        let mut cur = bits_from(&st);

        // The left stick acts like the D-pad unless it is moving the pointer or scrolling.
        if !desk {
            if lx < -0.55 { cur |= B_LEFT; }
            if lx > 0.55 { cur |= B_RIGHT; }
            if ly > 0.55 { cur |= B_UP; }
            if ly < -0.55 { cur |= B_DOWN; }
        }

        // Pointer from the sticks: only on the desktop, never in the console or a game.
        if mode == MODE_DESKTOP {
            let mut scale = shared.cursor_speed.load(Ordering::SeqCst).clamp(25, 300) as f32 / 100.0;
            if st.Gamepad.bLeftTrigger > 60 {
                scale *= 0.3; // hold LT for fine pointing
            }
            let (mut vx, mut vy) = stick_speed(rx, ry, scale);
            if left_mouse {
                let (ax, ay) = stick_speed(lx, ly, scale);
                vx += ax;
                vy += ay;
            }
            acc.0 += vx * dt;
            acc.1 += vy * dt;
            let (mx, my) = (acc.0.trunc() as i32, acc.1.trunc() as i32);
            acc.0 -= mx as f32;
            acc.1 -= my as f32;
            keys::mouse_move(mx, my);
            if left_scroll {
                // Smooth scrolling: speed grows with how far the stick is pushed.
                let curve = |v: f32| if v.abs() < 0.2 { 0.0 } else { v.signum() * ((v.abs() - 0.2) / 0.8).powi(2) };
                scroll_acc.0 += curve(lx) * 2400.0 * dt;
                scroll_acc.1 += curve(ly) * 2400.0 * dt;
                let (sx, sy) = ((scroll_acc.0 / 30.0).trunc() as i32 * 30, (scroll_acc.1 / 30.0).trunc() as i32 * 30);
                scroll_acc.0 -= sx as f32;
                scroll_acc.1 -= sy as f32;
                if sy != 0 { keys::mouse_wheel(sy); }
                if sx != 0 { keys::mouse_hwheel(sx); }
            } else {
                scroll_acc = (0.0, 0.0);
            }
        } else {
            acc = (0.0, 0.0);
            scroll_acc = (0.0, 0.0);
        }

        let pressed = cur & !prev;
        let released = !cur & prev;

        // Guide, or View + Menu held together, from anywhere.
        let chord = cur & (B_VIEW | B_MENU) == (B_VIEW | B_MENU);
        if (chord && !chord_used) || pressed & B_GUIDE != 0 {
            if chord {
                chord_used = true;
            }
            if fg != ours && fg != shared.osk_hwnd.load(Ordering::SeqCst) && !window::is_shell(fg) {
                shared.prev_fg.store(fg, Ordering::SeqCst);
            }
            cb(PadEvent::Guide);
        }

        // Hold View (about half a second) for the app switcher; a quick tap still does what View did before.
        if pressed & B_VIEW != 0 {
            view_at = Some(now);
        }
        if cur & B_VIEW != 0 && cur & B_MENU == 0 && !chord_used && !sw_hold {
            if let Some(t) = view_at {
                if now.duration_since(t) > Duration::from_millis(450) {
                    sw_hold = true;
                    cb(PadEvent::Switcher("open"));
                }
            }
        }
        let mut sw_released = false;
        if released & B_VIEW != 0 {
            view_at = None;
            if sw_hold {
                sw_hold = false;
                sw_released = true;
                if shared.switcher_open.load(Ordering::SeqCst) {
                    cb(PadEvent::Switcher("commit"));
                }
            }
        }
        let switcher = shared.switcher_open.load(Ordering::SeqCst) || sw_hold;

        // Extra buttons that only show up through Windows' raw game-controller interface.
        let has_extra = shared.extra.lock().map(|e| !e.is_empty()).unwrap_or(false);
        if has_extra {
            if let Some(raw) = crate::winrt::raw_buttons() {
                let map = shared.extra.lock().map(|e| e.clone()).unwrap_or_default();
                for (idx, action) in map {
                    let now_on = raw.get(idx).copied().unwrap_or(false);
                    let was_on = raw_prev.get(idx).copied().unwrap_or(false);
                    if now_on && !was_on {
                        cb(PadEvent::Extra(action));
                    }
                }
                raw_prev = raw;
            }
        }

        // Desktop clicking follows A down/up so dragging works.
        if mode == MODE_DESKTOP && !osk && !switcher && pressed & B_A != 0 {
            keys::mouse_left(true);
            left_down = true;
        }
        if released & B_A != 0 && left_down {
            keys::mouse_left(false);
            left_down = false;
            cb(PadEvent::Clicked);
        }

        for (i, &(bit, name)) in NAMES.iter().enumerate() {
            if bit == B_GUIDE {
                continue;
            }
            let fire = if bit == B_VIEW || bit == B_MENU {
                // Fire on release so View + Menu can be a chord (and View can be held).
                released & bit != 0 && !chord_used && !(bit == B_VIEW && sw_released)
            } else if pressed & bit != 0 {
                if bit & DIRS != 0 {
                    repeat_at[i - 12] = Some(now + Duration::from_millis(380));
                }
                true
            } else if cur & bit != 0 && bit & DIRS != 0 {
                match repeat_at[i - 12] {
                    Some(t) if now >= t => {
                        repeat_at[i - 12] = Some(now + Duration::from_millis(110));
                        true
                    }
                    _ => false,
                }
            } else {
                false
            };
            if !fire {
                continue;
            }
            if switcher {
                match name {
                    "A" => cb(PadEvent::Switcher("commit")),
                    "B" => cb(PadEvent::Switcher("cancel")),
                    "Left" | "Right" | "Up" | "Down" | "LB" | "RB" | "X" => cb(PadEvent::Switcher(name)),
                    _ => {}
                }
                continue;
            }
            match mode {
                MODE_CONSOLE => cb(PadEvent::Button(name)),
                MODE_DESKTOP if osk => cb(PadEvent::Osk(name)),
                MODE_DESKTOP => match name {
                    "B" => keys::mouse_right_click(),
                    "Up" => keys::mouse_wheel(120),
                    "Down" => keys::mouse_wheel(-120),
                    "Left" => keys::mouse_hwheel(-120),
                    "Right" => keys::mouse_hwheel(120),
                    "Y" => cb(PadEvent::Desktop("keyboard")),
                    "View" => cb(PadEvent::Desktop("home")),
                    "LB" => keys::tap(keys::VK_BROWSER_BACK),
                    "RB" => keys::tap(keys::VK_BROWSER_FORWARD),
                    _ => {}
                },
                _ => {}
            }
        }

        if cur & (B_VIEW | B_MENU) == 0 {
            chord_used = false;
        }
        prev = cur;
    }
}

/// Raw XInput reading of the first connected pad, for the controller test screen.
#[derive(serde::Serialize, Clone, Debug)]
pub struct XSnapshot {
    pub slot: u32,
    pub buttons: u16,
    pub lt: u8,
    pub rt: u8,
    pub lx: i16,
    pub ly: i16,
    pub rx: i16,
    pub ry: i16,
}

pub fn xinput_snapshot() -> Option<XSnapshot> {
    let get = load_get_state()?;
    for i in 0..4u32 {
        let mut st: XINPUT_STATE = unsafe { std::mem::zeroed() };
        if (unsafe { get(i, &mut st) }) == 0 {
            let g = st.Gamepad;
            return Some(XSnapshot {
                slot: i,
                buttons: g.wButtons as u16,
                lt: g.bLeftTrigger,
                rt: g.bRightTrigger,
                lx: g.sThumbLX,
                ly: g.sThumbLY,
                rx: g.sThumbRX,
                ry: g.sThumbRY,
            });
        }
    }
    None
}
