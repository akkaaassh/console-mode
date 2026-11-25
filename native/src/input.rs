//! Controller input thread.
//!
//! Polls XInput (Xbox-style pads, including the Blitz in XInput mode) about
//! 120 times a second and decides what each button means:
//! - Console mode (our window in front): buttons go to the UI as events,
//!   the right stick moves the real mouse pointer.
//! - Desktop mode (anything else in front, no game running): both sticks move
//!   the mouse, A clicks, B right-clicks, D-pad up/down scrolls,
//!   Y opens the on-screen keyboard, View returns to the console.
//! - Game mode: the pad is left alone, except Guide (or View + Menu
//!   together), which always brings the console quick menu up.

use crate::keys;
use crate::util::wide;
use crate::window;
use std::sync::atomic::{AtomicIsize, AtomicU8, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};
use windows_sys::Win32::System::LibraryLoader::{GetProcAddress, LoadLibraryW};
use windows_sys::Win32::UI::Input::XboxController::XINPUT_STATE;

pub const MODE_CONSOLE: u8 = 0;
pub const MODE_GAME: u8 = 1;
pub const MODE_DESKTOP: u8 = 2;

#[derive(Default)]
pub struct Shared {
    /// What the UI says is going on: MODE_CONSOLE, MODE_GAME or MODE_DESKTOP.
    pub mode: AtomicU8,
    /// Our main window handle.
    pub our_hwnd: AtomicIsize,
    /// The window that was in front when Guide was pressed (usually the game).
    pub prev_fg: AtomicIsize,
}

#[derive(Clone, Debug)]
pub enum PadEvent {
    /// A button press for the console UI (already de-bounced and auto-repeated).
    Button(&'static str),
    /// Guide, or View + Menu together.
    Guide,
    /// Controller connected / disconnected.
    Status(bool),
    /// Desktop-mode actions the shell handles: "home" or "keyboard".
    Desktop(&'static str),
}

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

// Logical buttons, one bit each.
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
fn stick_speed(x: f32, y: f32) -> (f32, f32) {
    let m = (x * x + y * y).sqrt();
    const DEAD: f32 = 0.16;
    if m < DEAD {
        return (0.0, 0.0);
    }
    let n = ((m - DEAD) / (1.0 - DEAD)).min(1.0);
    let speed = 1500.0 * n * n + 120.0 * n; // pixels per second
    (x / m * speed, -y / m * speed) // XInput Y is up; screen Y is down
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
    let mut last = Instant::now();
    let mut left_down = false;

    loop {
        std::thread::sleep(Duration::from_millis(8));
        let now = Instant::now();
        let dt = now.duration_since(last).as_secs_f32().min(0.05);
        last = now;

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
                    if unsafe { get(i, &mut probe) } == 0 {
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

        let ours = shared.our_hwnd.load(Ordering::SeqCst);
        let fg = window::foreground();
        let mode = if ours != 0 && fg == ours {
            MODE_CONSOLE
        } else {
            match shared.mode.load(Ordering::SeqCst) {
                MODE_GAME => MODE_GAME,
                _ => MODE_DESKTOP,
            }
        };

        let (lx, ly) = (axis(st.Gamepad.sThumbLX), axis(st.Gamepad.sThumbLY));
        let (rx, ry) = (axis(st.Gamepad.sThumbRX), axis(st.Gamepad.sThumbRY));
        let mut cur = bits_from(&st);

        // Left stick acts like the D-pad everywhere except the desktop.
        if mode != MODE_DESKTOP {
            if lx < -0.55 { cur |= B_LEFT; }
            if lx > 0.55 { cur |= B_RIGHT; }
            if ly > 0.55 { cur |= B_UP; }
            if ly < -0.55 { cur |= B_DOWN; }
        }

        // Mouse pointer from the sticks (never during a game).
        if mode != MODE_GAME {
            let (mut vx, mut vy) = stick_speed(rx, ry);
            if mode == MODE_DESKTOP {
                let (ax, ay) = stick_speed(lx, ly);
                vx += ax;
                vy += ay;
            }
            acc.0 += vx * dt;
            acc.1 += vy * dt;
            let (mx, my) = (acc.0.trunc() as i32, acc.1.trunc() as i32);
            acc.0 -= mx as f32;
            acc.1 -= my as f32;
            keys::mouse_move(mx, my);
        } else {
            acc = (0.0, 0.0);
        }

        let pressed = cur & !prev;
        let released = !cur & prev;

        // Guide, or View + Menu held together.
        let chord = cur & (B_VIEW | B_MENU) == (B_VIEW | B_MENU);
        if (chord && !chord_used) || pressed & B_GUIDE != 0 {
            if chord {
                chord_used = true;
            }
            if fg != ours {
                shared.prev_fg.store(fg, Ordering::SeqCst);
            }
            cb(PadEvent::Guide);
        }

        // Desktop clicking follows A down/up so dragging works.
        if mode == MODE_DESKTOP {
            if pressed & B_A != 0 {
                keys::mouse_left(true);
                left_down = true;
            }
        }
        if released & B_A != 0 && left_down {
            keys::mouse_left(false);
            left_down = false;
        }

        for (i, &(bit, name)) in NAMES.iter().enumerate() {
            if bit == B_GUIDE {
                continue;
            }
            let fire = if bit == B_VIEW || bit == B_MENU {
                // Fire on release so View + Menu can be a chord.
                released & bit != 0 && !chord_used
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
            match mode {
                MODE_CONSOLE => cb(PadEvent::Button(name)),
                MODE_DESKTOP => match name {
                    "B" => keys::mouse_right_click(),
                    "Up" => keys::mouse_wheel(120),
                    "Down" => keys::mouse_wheel(-120),
                    "Y" => cb(PadEvent::Desktop("keyboard")),
                    "View" => cb(PadEvent::Desktop("home")),
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
