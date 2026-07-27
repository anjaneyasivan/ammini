# min-mpv

A tiny desktop video player built with Rust, `eframe`, `egui`, and `libmpv` via [`egui-sharkplayer`](https://crates.io/crates/egui-sharkplayer).

## Features

- Open individual files or whole folders of videos.
- Playlist side panel with click-to-play, prev/next, and repeat.
- Drag-and-drop files directly into the window.
- Keyboard shortcuts for open, playlist toggle, and navigation.
- Recent files menu, persisted across restarts.
- Built-in mpv controls are available in the video overlay (play/pause, seek, fullscreen, etc.).

## Requirements

- Rust 1.92 or newer.
- libmpv installed on your system (the `libmpv2` crate links against it at build time).

### macOS

```bash
brew install mpv
```

`build.rs` automatically adds `/opt/homebrew/lib` or `/usr/local/lib` as a linker/rpath search directory, so no extra environment variables are needed on Apple Silicon or Intel Macs.

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

Install a libmpv build (e.g. from [shinchiro's Windows builds](https://github.com/shinchiro/mpv-winbuild-cmake) or [mpv.io](https://mpv.io/installation/)). Make sure the linker can find `libmpv.dll.a`/`mpv.lib` and that `libmpv.dll` is next to the final `min-mpv.exe` at runtime.

## Build

```bash
cargo build --release
```

## Run

```bash
cargo run
```

## Shortcuts

| Shortcut | Action |
|----------|--------|
| `Ctrl/Cmd + O` | Open file |
| `Ctrl/Cmd + Shift + O` | Open folder |
| `Ctrl/Cmd + P` | Toggle playlist |
| `Ctrl/Cmd + Left` | Previous file |
| `Ctrl/Cmd + Right` | Next file |
| `Drag & drop` | Open dropped video |

mpv's own overlay controls work inside the video area (e.g. space to pause, `F` for fullscreen, arrows to seek).

## Notes

- The `glow` backend is required because `egui-sharkplayer` renders video through OpenGL.
- Recent files are stored in `eframe`'s native storage location for the app.
