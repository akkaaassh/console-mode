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
| Anywhere | **Guide**, or **View + Menu** together: quick menu. **Hold View** (about half a second): app switcher with live previews; move with the stick, D-pad or LB/RB, let go of View or press **A** to switch, **X** closes that app, **B** cancels |
| Console | D-pad or left stick to move, **A** select, **B** back, **LB/RB** Games or Apps, **X** options, **Y** Library, **Menu** Settings, **View** Desktop. Right stick moves the pointer; **A** clicks what it points at |
| Desktop and apps | Sticks move the pointer, **A** click (hold to drag), **B** right-click, D-pad scrolls, **LB/RB** back/forward, **Y** keyboard, **View** back to the console. Clicking a text field opens the keyboard by itself |
| Keyboard | D-pad or left stick to pick a key, **A** type it, **B** backspace, **X** shift, **Y** space, **LB/RB** move the text cursor, **Menu** enter, **View** close |
| Full-screen games | The pointer stays off; only Guide (or View + Menu) is used |
| Quick menu | Resume, Switch game, Desktop, Record clip (Game Bar), Discord, Quit game. Music: previous, play/pause, next, volume; **LB/RB** volume; **Y** music panel (Up next, Library, Search; **LB/RB** switch tabs, **A** plays, **B** goes back) |

Works with Xbox-style controllers (XInput), including most third-party pads in XInput mode.

## What it does (v0.4)

- Finds games from **Steam**, **Epic Games** and **GOG** (with Steam artwork), plus any game you add yourself.
- Finds entertainment **apps** from the Start menu (Netflix, Prime Video, Spotify, YouTube, JioHotstar, VLC, Plex…).
- Favourite or hide games and apps, open install folders, sort by recently played, name or recently installed.
- Music from any app through Windows: title, artist, cover art, progress, play/pause, next/previous, system volume. Connect Spotify in **Settings › Music** to see the up-next queue.
- Settings: pointer speed, brightness (laptops and DDC/CI monitors), power mode, resolution, refresh rate, live CPU/GPU/memory, start with Windows, exit.
- Moving particle background (can be turned off in Settings).
- **Change artwork** from a game's or app's ··· menu: pick a cover, background or logo from SteamGridDB, choose a different match, search another name, or use an image from your PC.
- **Web apps**: Chrome and Edge installed web apps show up in Apps, and **+ Add web app** turns any website into a full-screen app tile.
- **Updates**: Settings › Updates checks GitHub for a newer build and installs it.
- **Controller test**: Settings › Controller shows every button Windows sees and lets you give extra buttons (if your pad reports them) an action.
- **Close to tray**: from Power or Settings › Startup. The controller mouse keeps working; open Console Mode again from the icon by the clock.

### Connecting Spotify (optional, for the music panel)

1. Open developer.spotify.com/dashboard and create an app.
2. Add the Redirect URI `http://127.0.0.1:43821/callback` and tick **Web API**.
3. Copy the app's Client ID into **Settings › Music**, then press **Connect Spotify** and sign in.

If you connected before v0.4, press **Reconnect Spotify** once so Console Mode can read your library and start playback. Starting playback needs Spotify Premium.

## Not yet

- Controller PIN pad on the Windows sign-in screen (a separate sign-in plug-in, planned).
- PlayStation controllers and pads in Bluetooth mode, Xbox app and EA games, TV on/off.

## Project layout

- `ui/` — the interface (HTML, CSS, JavaScript). It also runs in a normal browser with sample data.
- `native/` — Windows code: controller input, mouse, game and app scanning, launching, media keys, power, display and stats.
- `src-tauri/` — the app shell that connects the two.
- `.github/workflows/build.yml` — builds the Windows installer on every push and publishes it as **Latest build**.
