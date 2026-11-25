//! Synthetic keyboard and mouse input through `SendInput`.

use std::mem::size_of;
use windows_sys::Win32::UI::Input::KeyboardAndMouse::*;

pub const VK_WIN: u16 = 0x5B; // left Windows key
pub const VK_ALT: u16 = 0x12;
pub const VK_G: u16 = 0x47;
pub const VK_PLAY_PAUSE: u16 = 0xB3;
pub const VK_NEXT_TRACK: u16 = 0xB0;
pub const VK_PREV_TRACK: u16 = 0xB1;
pub const VK_VOL_UP: u16 = 0xAF;
pub const VK_VOL_DOWN: u16 = 0xAE;
pub const VK_VOL_MUTE: u16 = 0xAD;

fn send(inputs: &[INPUT]) {
    if inputs.is_empty() {
        return;
    }
    unsafe {
        SendInput(inputs.len() as u32, inputs.as_ptr(), size_of::<INPUT>() as i32);
    }
}

fn key_input(vk: u16, up: bool) -> INPUT {
    let mut flags: u32 = 0;
    // Media and Windows keys are "extended" keys.
    if matches!(vk, 0x5B | 0xAD..=0xB3) {
        flags |= KEYEVENTF_EXTENDEDKEY;
    }
    if up {
        flags |= KEYEVENTF_KEYUP;
    }
    INPUT {
        r#type: INPUT_KEYBOARD,
        Anonymous: INPUT_0 {
            ki: KEYBDINPUT { wVk: vk, wScan: 0, dwFlags: flags, time: 0, dwExtraInfo: 0 },
        },
    }
}

fn mouse_input(flags: u32, dx: i32, dy: i32, data: i32) -> INPUT {
    INPUT {
        r#type: INPUT_MOUSE,
        Anonymous: INPUT_0 {
            mi: MOUSEINPUT { dx, dy, mouseData: data as u32, dwFlags: flags, time: 0, dwExtraInfo: 0 },
        },
    }
}

/// Press and release one key.
pub fn tap(vk: u16) {
    send(&[key_input(vk, false), key_input(vk, true)]);
}

/// Press keys in order, release them in reverse (e.g. Win+Alt+G).
pub fn combo(vks: &[u16]) {
    let mut v: Vec<INPUT> = vks.iter().map(|&k| key_input(k, false)).collect();
    v.extend(vks.iter().rev().map(|&k| key_input(k, true)));
    send(&v);
}

/// Pressing and releasing Alt lets a background process take the foreground.
pub fn alt_nudge() {
    send(&[key_input(VK_ALT, false), key_input(VK_ALT, true)]);
}

pub fn mouse_move(dx: i32, dy: i32) {
    if dx != 0 || dy != 0 {
        send(&[mouse_input(MOUSEEVENTF_MOVE, dx, dy, 0)]);
    }
}

pub fn mouse_left(down: bool) {
    send(&[mouse_input(if down { MOUSEEVENTF_LEFTDOWN } else { MOUSEEVENTF_LEFTUP }, 0, 0, 0)]);
}

pub fn mouse_right_click() {
    send(&[
        mouse_input(MOUSEEVENTF_RIGHTDOWN, 0, 0, 0),
        mouse_input(MOUSEEVENTF_RIGHTUP, 0, 0, 0),
    ]);
}

/// Positive scrolls up, negative scrolls down (120 = one notch).
pub fn mouse_wheel(delta: i32) {
    send(&[mouse_input(MOUSEEVENTF_WHEEL, 0, 0, delta)]);
}

/// Windows Game Bar "record what happened" shortcut.
pub fn record_clip() {
    combo(&[VK_WIN, VK_ALT, VK_G]);
}
