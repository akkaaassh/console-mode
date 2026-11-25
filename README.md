# Console Mode

A console-style launcher for Windows that you drive with a controller.

## Install

1. Open **Releases → Latest build** in this repo.
2. Download the `.exe` and run it. It installs for your user only, so no admin prompt.
3. Console Mode opens full screen. Turn on **Settings → Startup → Open Console Mode when Windows starts** to have it open after you sign in.

Windows SmartScreen may warn that the app is unrecognised, because the installer isn't code-signed yet. Choose **More info → Run anyway**.

## Controls

| Where | Controller |
| --- | --- |
| Anywhere | **Guide**, or **View + Menu** together: quick menu |
| Console | D-pad or left stick to move, **A** to select, **B** back, **LB/RB** Games or Apps, **Y** Library, **Menu** Settings, **View** Desktop |
| Console | Right stick moves the mouse pointer; **A** clicks what it points at |
| Desktop | Either stick moves the mouse, **A** click (hold to drag), **B** right-click, D-pad up/down scroll, **Y** on-screen keyboard, **View** back to the console |
| Quick menu | Resume, Switch game, Desktop, Record clip (Game Bar), Discord, Quit game. Music: previous, play/pause, next; **LB/RB** volume; **Y** Spotify panel |

Works with Xbox-style controllers (XInput), including most third-party pads in XInput mode.

## What it does today (v0.1)

- Finds installed games from **Steam**, **Epic Games** and **GOG**, with Steam artwork.
- Finds entertainment **apps** from the Start menu (Netflix, Prime Video, Spotify, YouTube, JioHotstar, VLC, Plex…).
- Sorts games and apps by recently played, name, or recently installed.
- Music controls work with any player. Now playing is read from the Spotify app.
- Settings: brightness, power mode, resolution, refresh rate, live CPU, GPU and memory, start with Windows, exit.

## Not yet

- Controller PIN pad on the Windows sign-in screen (a separate sign-in plug-in, planned).
- On-screen keyboard that opens by itself on text fields (for now, **Y** on the desktop opens Windows' keyboard).
- Spotify queue (needs Spotify sign-in), PlayStation and Bluetooth-mode controllers, Xbox app and EA games, TV on/off.

## Project layout

- `ui/` — the interface (HTML, CSS, JavaScript). It also runs in a normal browser with sample data.
- `native/` — Windows code: controller input, mouse, game and app scanning, launching, media keys, power, display and stats.
- `src-tauri/` — the app shell that connects the two.
- `.github/workflows/build.yml` — builds the Windows installer on every push and publishes it as **Latest build**.
