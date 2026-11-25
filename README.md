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
| Console | D-pad or left stick to move, **A** select, **B** back, **LB/RB** Games or Apps, **X** options, **Y** Library, **Menu** Settings, **View** Desktop. Right stick moves the pointer; **A** clicks what it points at |
| Desktop and apps | Sticks move the pointer, **A** click (hold to drag), **B** right-click, D-pad scrolls, **LB/RB** back/forward, **Y** keyboard, **View** back to the console. Clicking a text field opens the keyboard by itself |
| Keyboard | D-pad or left stick to pick a key, **A** type it, **B** backspace, **X** shift, **Y** space, **LB/RB** move the text cursor, **Menu** enter, **View** close |
| Full-screen games | The pointer stays off; only Guide (or View + Menu) is used |
| Quick menu | Resume, Switch game, Desktop, Record clip (Game Bar), Discord, Quit game. Music: previous, play/pause, next, volume; **LB/RB** volume; **Y** Spotify panel |

Works with Xbox-style controllers (XInput), including most third-party pads in XInput mode.

## What it does (v0.2)

- Finds games from **Steam**, **Epic Games** and **GOG** (with Steam artwork), plus any game you add yourself.
- Finds entertainment **apps** from the Start menu (Netflix, Prime Video, Spotify, YouTube, JioHotstar, VLC, Plex…).
- Favourite or hide games and apps, open install folders, sort by recently played, name or recently installed.
- Music from any app through Windows: title, artist, cover art, progress, play/pause, next/previous, system volume. Connect Spotify in **Settings › Music** to see the up-next queue.
- Settings: pointer speed, brightness (laptops and DDC/CI monitors), power mode, resolution, refresh rate, live CPU/GPU/memory, start with Windows, exit.
- Moving particle background (can be turned off in Settings).

### Connecting Spotify (optional, for the queue)

1. Open developer.spotify.com/dashboard and create an app.
2. Add the Redirect URI `http://127.0.0.1:43821/callback` and tick **Web API**.
3. Copy the app's Client ID into **Settings › Music**, then press **Connect Spotify** and sign in.

## Not yet

- Controller PIN pad on the Windows sign-in screen (a separate sign-in plug-in, planned).
- PlayStation controllers and pads in Bluetooth mode, Xbox app and EA games, TV on/off.

## Project layout

- `ui/` — the interface (HTML, CSS, JavaScript). It also runs in a normal browser with sample data.
- `native/` — Windows code: controller input, mouse, game and app scanning, launching, media keys, power, display and stats.
- `src-tauri/` — the app shell that connects the two.
- `.github/workflows/build.yml` — builds the Windows installer on every push and publishes it as **Latest build**.
