# Ammini

A small desktop video player built with Rust, `eframe`/`egui`, and `libmpv` via
[`egui-sharkplayer`](https://crates.io/crates/egui-sharkplayer) — with a built-in
**Telegram client** sidebar for browsing chats and streaming videos straight from your
Telegram account.

> **Building from source?** See **[BUILDING.md](BUILDING.md)** for platform
> prerequisites, credential handling, release builds, the macOS `.app`/`.dmg` bundler,
> and troubleshooting.

## Features

- Open individual files or whole folders of videos; drag-and-drop works too.
- Playlist side panel with click-to-play and prev/next navigation.
- **Audio-track switcher**: the top-bar `Audio` dropdown lists the current file's audio
  streams (title/language), with the active one checked; picking another switches mpv
  to it.
- **Persistent state across restarts**: recent files, the current playlist and track,
  volume, per-file resume positions, and a "Recent Telegram files" section in the Recent
  menu — recently played Telegram videos are replayed by refetching their message, so
  they work even after a restart or chat switch. Quitting (`Cmd+W` / `Cmd+Q`) saves the
  current position; picking a recent entry resumes from it when one exists (local files
  by path, Telegram videos by message id — the proxy URL's port changes every launch),
  and playback keeps the display awake (macOS).
- Telegram sidebar (`Cmd+T`): phone/2FA sign-in, chat list, message list with video
  playback, load-more pagination, and real sign-out (revokes the session and wipes local
  data).
- Telegram videos stream through a local HTTP proxy backed by a **disk block cache** —
  watched videos are cached (512 KiB blocks), reused across restarts, prefetched ahead
  of the playhead, and garbage-collected (30-day age / 2 GiB budget). The part of the
  seekbar already in the cache is shaded lighter than the track (approximate for VBR
  videos, since the cache maps bytes, not time), and a dimmed **loading spinner**
  overlays the video while a Telegram file's initial load is in progress.
- Material Design icons on all buttons, Material icons for the on-video control bar.
- **Native-feeling macOS UI**: Inter font, system light/dark appearance, chat list
  cards with avatar circles, and iMessage-style message bubbles (right-aligned accent
  for your own messages, grouped by sender with timestamps at group starts).
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
`libmpv.dll.a`/`mpv.lib` and that `libmpv.dll` is next to the final `ammini.exe` at
runtime.

## Setup

Copy the example environment file and fill in your Telegram API credentials (get them
at https://my.telegram.org/apps):

```bash
cp .env.example .env
```

The app exits at startup if `TELEGRAM_API_ID` / `TELEGRAM_API_HASH` are missing — unless
they were baked in at build time from `.env` (see below), in which case the binary runs
without any `.env` next to it.

## Build & run

```bash
cargo run
```

Credentials are resolved as: live environment → `.env` (via `dotenvy`) → the value baked
into the binary at compile time. `build.rs` reads `.env` and re-exports the keys with
`cargo:rustc-env`, so a `cargo build` picks them up automatically and the resulting
binary can be copied anywhere.

## macOS app bundle

```bash
scripts/bundle-macos.sh
```

Builds a release binary, assembles `dist/Ammini.app`, copies libmpv and its entire dylib
closure (≈48 libraries) into `Contents/Frameworks` with their install names rewritten to
`@rpath`, ad-hoc signs everything, and produces `dist/Ammini-<version>.dmg` with the
usual drag-to-`Applications` layout. The bundle is self-contained — it runs on Macs that
do not have Homebrew's mpv installed — and only needs the built-in macOS tools
(`otool`, `install_name_tool`, `codesign`, `hdiutil`).

Worth knowing:

- Everything is ad-hoc signed, which is fine locally and shareable via
  right-click → Open the first time. Shipping to the public without a Gatekeeper warning
  needs a Developer ID signature and notarization, which the script does not do.
- The release build bakes your Telegram credentials into the binary, so treat the DMG as
  containing them.
- Homebrew's mpv/ffmpeg are GPL-licensed; redistributing the bundled libraries carries
  licence obligations, so the script is aimed at personal builds.
- The bundle only runs on the macOS it was built on or newer — Homebrew builds its dylibs
  against the build machine — so build the DMG on the oldest macOS you need to support
  (`MIN_MACOS=15.5 scripts/bundle-macos.sh` declares the target; see BUILDING.md →
  "Supported macOS versions").

## Shortcuts

| Shortcut | Action |
|----------|--------|
| `Ctrl/Cmd + O` | Open file |
| `Ctrl/Cmd + Shift + O` | Open folder |
| `Ctrl/Cmd + U` | Open URL dialog |
| `Ctrl/Cmd + P` | Toggle playlist panel |
| `Ctrl/Cmd + T` | Toggle Telegram panel |
| `Ctrl/Cmd + Left/Right` | Previous/next file |
| `Ctrl/Cmd + W` | Close the window (quits, saving the resume position + recent entry) |
| `Ctrl/Cmd + Q` | Quit (same save-on-exit behavior) |
| `Space` | Play/pause (when the video surface is not focused) |
| `F` | Toggle fullscreen |
| `Esc` | Leave fullscreen |
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
  work like a local file. Played videos appear under **Recent → Recent Telegram files**
  and can be replayed from there at any time (the message is refetched on demand, so
  this survives restarts and chat switches).
- Telegram videos are cached under `~/Library/Caches/min-mpv/telegram_cache/`
  (`{chat_id}_{msg_id}.bin` + a manifest). The cache is reused across restarts and
  swept automatically (files older than 30 days, or the oldest files beyond a 2 GiB
  total budget).

## Telemetry

Ammini can report important events to Sentry through two channels: an OTLP log
pipeline (the project's `.../integration/otlp` endpoint) for events and errors, and
the Sentry SDK's native metrics (Sentry does not ingest OTLP metrics) for counters
and distributions. Both are driven by a single typed event emitter
(`src/telemetry.rs`) on a dedicated background thread, so telemetry never blocks the
UI.

What is tracked: app start, Telegram sign-in / sign-out / login failures, the
signed-in Telegram account's display name as the Sentry user context (username),
played files (basename only — never full paths; Telegram videos also carry their
size plus metadata: duration, resolution and mime type), Recent-menu picks (local
files and Telegram videos), audio-track switches, playback errors, panics,
background Telegram/proxy errors, and metrics (download speed — sampled twice a
second per stream — plus block-cache hit/miss, video counters, and seek latency:
the time from an arrow / J / L / skip-button seek until playback resumes past the
seek target).

Telemetry is **off by default** and only activates when both of these are present in
`.env` (resolved like the Telegram credentials: environment → `.env` → baked into the
binary at build time, see `build.rs`):

```
SENTRY_DSN=https://<key>@o<org>.ingest.sentry.io/<project_id>
SENTRY_OTLP_URL=https://o<org>.ingest.sentry.io/api/<project_id>/integration/otlp
```

While telemetry runs, events are batched (5 s or 64 records, retried on failure) and
flushed when the app exits. The only Telegram account data reported is the display
name (username on the Sentry user context) and phone number, on a single
`telegram.user_identified` log that fires at sign-in (including when a saved session
resumes at launch); the user context is cleared again on sign-out.

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
- The Telegram session, video cache and storage key still use the app's former name
  (`min-mpv` / `min_mpv_state`) on disk, so existing sessions, caches and settings keep
  working after the rename to Ammini.