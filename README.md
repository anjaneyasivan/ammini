# min-mpv

A small desktop video player built with Rust, `eframe`/`egui`, and `libmpv` via
[`egui-sharkplayer`](https://crates.io/crates/egui-sharkplayer) — with a built-in
**Telegram client** sidebar for browsing chats and streaming videos straight from your
Telegram account.

## Features

- Open individual files or whole folders of videos; drag-and-drop works too.
- Playlist side panel with click-to-play and prev/next navigation.
- **Persistent state across restarts**: recent files, the current playlist and track,
  volume, and per-file resume positions (local files only).
- Telegram sidebar (`Cmd+T`): phone/2FA sign-in, chat list, message list with video
  playback, load-more pagination, and real sign-out (revokes the session and wipes local
  data).
- Telegram videos stream through a local HTTP proxy backed by a **disk block cache** —
  watched videos are cached (512 KiB blocks), reused across restarts, prefetched ahead
  of the playhead, and garbage-collected (30-day age / 2 GiB budget).
- Material Design icons on all buttons, Material icons for the on-video control bar.
- mpv's own overlay controls inside the video area (play/pause, seek, volume, fullscreen).

## Requirements

- Rust 1.92 or newer.
- libmpv installed on your system (the app links against it at build time).

### macOS

```bash
brew install mpv
```

`build.rs` automatically adds `/opt/homebrew/lib` or `/usr/local/lib` as a
linker/rpath search directory, so no extra environment variables are needed on Apple
Silicon or Intel Macs.

### Linux

```bash
# Debian/Ubuntu
sudo apt install libmpv-dev

# Fedora
sudo dnf install mpv-devel

# Arch
sudo pacman -S mpv
```

If `libmpv` is in a non-standard path, set `LIBRARY_PATH` before building:

```bash
LIBRARY_PATH=/path/to/libmpv/lib cargo build
```

### Windows

Install a libmpv build (e.g. from [shinchiro's Windows builds](https://github.com/shinchiro/mpv-winbuild-cmake)
or [mpv.io](https://mpv.io/installation/)). Make sure the linker can find
`libmpv.dll.a`/`mpv.lib` and that `libmpv.dll` is next to the final `min-mpv.exe` at
runtime.

## Setup

Copy the example environment file and fill in your Telegram API credentials (get them
at https://my.telegram.org/apps):

```bash
cp .env.example .env
```

The app exits at startup if `TELEGRAM_API_ID` / `TELEGRAM_API_HASH` are missing.

## Build & run

```bash
cargo run
```

## Shortcuts

| Shortcut | Action |
|----------|--------|
| `Ctrl/Cmd + O` | Open file |
| `Ctrl/Cmd + Shift + O` | Open folder |
| `Ctrl/Cmd + U` | Open URL dialog |
| `Ctrl/Cmd + P` | Toggle playlist panel |
| `Ctrl/Cmd + T` | Toggle Telegram panel |
| `Ctrl/Cmd + Left/Right` | Previous/next file |
| `Space` | Play/pause (when the video surface is not focused) |
| `F` | Toggle fullscreen |
| `M` | Mute/unmute |
| `+` / `-` | Volume ±5 |

When the video surface has keyboard focus, the mpv overlay controls apply instead:
`Space`/`K` play-pause, `I` media info, `M` mute, `F` fullscreen, `Left/Right` seek ±10 s,
`Up/Down` volume ±5.

## Telegram usage

- `Cmd+T` opens the sidebar. Sign in with your phone number, the login code, and (if
  enabled) your 2FA password.
- The session persists in
  `~/Library/Application Support/min-mpv/telegram.session` (platform data dir
  elsewhere). **Sign out** revokes the session server-side and deletes the local
  session file and all cached videos — the next launch starts logged out.
- Chats and messages paginate with the "Load more" buttons. Messages with videos show a
  play button; playing one streams it through the local proxy, so seeking and scrubbing
  work like a local file.
- Telegram videos are cached under `~/Library/Caches/min-mpv/telegram_cache/`
  (`{chat_id}_{msg_id}.bin` + a manifest). The cache is reused across restarts and
  swept automatically (files older than 30 days, or the oldest files beyond a 2 GiB
  total budget).

## Testing

```bash
cargo test --lib              # unit tests (cache, proxy range parsing, FSM, …)
cargo test --test emoji_support   # bundled font stack coverage
cargo test --test proxy_video     # offline proxy integration tests (fake source)
```

`cargo test --test hevc_proxy` is a live integration test that needs a valid Telegram
session and an HEVC video in some chat; it skips cleanly when either is missing and is
deliberately not part of plain `cargo test`.

## Notes

- The `glow` backend is required because `egui-sharkplayer` renders video through
  OpenGL — don't change the renderer.
- App state (recent files, playlist, volume, resume positions) is stored in eframe's
  native storage for the app under the `min_mpv_state` key.