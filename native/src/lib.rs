//! Windows system layer for Console Mode.
//!
//! Everything here talks to Win32 directly (through `windows-sys`), so the
//! Tauri shell in `src-tauri` stays a thin layer of commands and events.

pub mod apps;
pub mod display;
pub mod games;
pub mod input;
pub mod keys;
pub mod launch;
pub mod media;
pub mod power;
pub mod reg;
pub mod stats;
pub mod util;
pub mod window;
pub mod winrt;
pub mod wifi;
pub mod bluetooth;
pub mod art;
