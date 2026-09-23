# Building Ammini

Everything needed to build, test, and package Ammini from source. For what the app does,
see the [README](README.md).

- [1. Prerequisites](#1-prerequisites)
- [2. Get the source](#2-get-the-source)
- [3. Telegram credentials](#3-telegram-credentials)
- [4. Development build](#4-development-build)
- [5. Release build](#5-release-build)
- [6. macOS app bundle and DMG](#6-macos-app-bundle-and-dmg)
- [7. Tests](#7-tests)
- [8. Troubleshooting](#8-troubleshooting)
- [9. Project layout](#9-project-layout)

## 1. Prerequisites

### Rust

Rust **1.92 or newer** (the crate uses edition 2024). With [rustup](https://rustup.rs):

```bash
rustup update stable
rustc --version   # must be >= 1.92
```

### libmpv

Ammini links against `libmpv` at build time and loads it at runtime.

**macOS**

```bash
brew install mpv
```

`build.rs` adds `/opt/homebrew/lib` or `/usr/local/lib` to the linker and rpath search
paths automatically, so no environment variables are needed on Apple Silicon or Intel.

**Linux**

```bash
# Debian / Ubuntu
sudo apt install libmpv-dev pkg-config

# Fedora
sudo dnf install mpv-devel pkg-config

# Arch
sudo pacman -S mpv
```

If libmpv lives in a non-standard prefix:

```bash
LIBRARY_PATH=/path/to/libmpv/lib cargo build
```

**Windows**

Install a libmpv build (e.g. [shinchiro's builds](https://github.com/shinchiro/mpv-winbuild-cmake)
or [mpv.io](https://mpv.io/installation/)), make sure the linker can find
`libmpv.dll.a` / `mpv.lib`, and put `libmpv.dll` next to the produced `ammini.exe`.

### Platform toolchain

- **macOS**: Xcode Command Line Tools (`xcode-select --install`). The bundling script
  additionally uses `otool`, `install_name_tool`, `codesign`, `hdiutil`, `iconutil` —
  all bundled with macOS.
- **Linux**: a C toolchain plus `pkg-config` (see above). Video renders through OpenGL,
  so a working GL driver is required at runtime.
- **Windows**: the MSVC toolchain (or the GNU one with a matching libmpv build).

## 2. Get the source

```bash
git clone <your-remote> ammini
cd ammini
```

The `vendor/egui-sharkplayer` directory is a patched copy of the upstream crate, wired up
through `[patch.crates-io]` in `Cargo.toml`; it is built automatically as a dependency,
nothing extra to do.

## 3. Telegram credentials

The Telegram sidebar needs an API ID and hash from <https://my.telegram.org/apps>:

```bash
cp .env.example .env
# then edit .env:
#   TELEGRAM_API_ID=1234567
#   TELEGRAM_API_HASH=0123456789abcdef0123456789abcdef
```

`.env` is gitignored. At runtime the values are resolved in this order:

1. the live environment (`TELEGRAM_API_ID` / `TELEGRAM_API_HASH`),
2. `.env`, loaded by `dotenvy` from the working directory or any parent,
3. the value **baked into the binary at compile time**.

`build.rs` parses `.env` and re-exports the keys with `cargo:rustc-env`, so a plain
`cargo build` captures them and the resulting binary runs without any `.env` beside it.
Consequences worth knowing:

- Changing `.env` requires a rebuild (handled automatically — `build.rs` emits
  `cargo:rerun-if-changed=.env`, which matters because the file is gitignored and cargo
  would not otherwise watch it).
- Any binary you build or distribute **contains your credentials**; they can be recovered
  from it with `strings`. That is normal for Telegram clients, but treat release
  artifacts accordingly.
- CI has no `.env`, so nothing is baked there and the real environment must provide the
  values.

## 4. Development build

```bash
cargo build          # -> target/debug/ammini
cargo run            # build and launch
```

Logging is on by default (`ammini=debug`). Raise or narrow it with `RUST_LOG`:

```bash
RUST_LOG=ammini=trace cargo run
RUST_LOG=ammini::telegram=debug cargo run
```

The `glow` (OpenGL) renderer is required because `egui-sharkplayer` draws video through
OpenGL — do not switch it to `wgpu`.

## 5. Release build

```bash
cargo build --release    # -> target/release/ammini
```

The binary is portable across directories on the same machine, but it still dynamically
links libmpv. To produce something that runs without Homebrew's mpv installed, use the
macOS bundle below.

## 6. macOS app bundle and DMG

```bash
scripts/bundle-macos.sh
```

Produces:

```
dist/Ammini.app              # self-contained bundle
dist/Ammini-<version>.dmg    # drag-to-Applications installer
```

The script:

1. runs `cargo build --release`;
2. assembles `Ammini.app/Contents/{MacOS,Resources,Frameworks}` and writes `Info.plist`;
3. copies libmpv and its entire dylib closure (~48 libraries) into `Contents/Frameworks`;
4. rewrites every install name to `@rpath/<name>`, drops the build-time `/opt/homebrew/lib`
   rpath and adds `@executable_path/../Frameworks`;
5. verifies the bundle is self-contained (fails if any `@rpath` entry is unresolved or a
   Homebrew path survives);
6. ad-hoc signs the libraries and the bundle;
7. builds the DMG.

Install it with `open dist/Ammini-<version>.dmg` and drag **Ammini** into
**Applications**, or run it in place with `open dist/Ammini.app`.

Notes:

- **Signing.** Everything is ad-hoc signed, which is enough locally. Recipients of the
  DMG may need right-click → Open the first time. Shipping publicly without a Gatekeeper
  warning requires a Developer ID certificate plus notarization, which the script
  deliberately does not do.
- **Licensing.** Homebrew's mpv/ffmpeg are GPL-licensed; redistributing the bundled
  libraries carries licence obligations. The script is aimed at personal builds.
- **Regenerating the icon.** The artwork is `assets/ammini-logo.png`; the derived files
  are `assets/ammini-icon.png` (window icon, embedded via `include_bytes!`) and
  `assets/Ammini.icns` (bundle icon). To regenerate after changing the logo:

  <details>
  <summary>ImageMagick + iconutil commands</summary>

  ```bash
  # Crop the transparent margin off the logo, then make the window icon.
  magick assets/ammini-logo.png -crop 896x896+80+80 +repage /tmp/ammini-square.png
  magick /tmp/ammini-square.png -resize 512x512 assets/ammini-icon.png

  # Build the .icns from the same crop.
  mkdir -p /tmp/Ammini.iconset
  s=/tmp/ammini-square.png
  magick "$s" -resize 16x16     /tmp/Ammini.iconset/icon_16x16.png
  magick "$s" -resize 32x32     /tmp/Ammini.iconset/icon_16x16@2x.png
  magick "$s" -resize 32x32     /tmp/Ammini.iconset/icon_32x32.png
  magick "$s" -resize 64x64     /tmp/Ammini.iconset/icon_32x32@2x.png
  magick "$s" -resize 128x128   /tmp/Ammini.iconset/icon_128x128.png
  magick "$s" -resize 256x256   /tmp/Ammini.iconset/icon_128x128@2x.png
  magick "$s" -resize 256x256   /tmp/Ammini.iconset/icon_256x256.png
  magick "$s" -resize 512x512   /tmp/Ammini.iconset/icon_256x256@2x.png
  magick "$s" -resize 512x512   /tmp/Ammini.iconset/icon_512x512.png
  magick "$s" -resize 1024x1024 /tmp/Ammini.iconset/icon_512x512@2x.png
  iconutil -c icns /tmp/Ammini.iconset -o assets/Ammini.icns

  rm -rf /tmp/Ammini.iconset /tmp/ammini-square.png
  ```

  The crop geometry (`896x896+80+80`) is the bounding box of the blue squircle in the
  source logo. Find it for a different image with:

  ```bash
  magick logo.png -alpha extract -threshold 50% -format "%@\n" info:
  ```

  </details>

## 7. Tests

The offline suite is what CI runs (macOS) and is safe to run anywhere:

```bash
cargo fmt --check
cargo clippy --all-targets -- -D warnings

cargo test --lib                    # cache, proxy range parsing, FSM, …
cargo test --test emoji_support     # bundled font/icon coverage
cargo test --test proxy_video       # offline proxy integration (fake video source)
```

The vendored crate has its own tests (fixed-width seekbar time labels) and needs
`LIBRARY_PATH` on macOS so it can link libmpv:

```bash
LIBRARY_PATH=/opt/homebrew/lib cargo test --manifest-path vendor/egui-sharkplayer/Cargo.toml
```

`cargo test --test hevc_proxy` is a **live** integration test: it talks to Telegram and
needs valid credentials, an authorized session, and an HEVC video in some chat. It prints
"Skipping test…" and passes when any of those is missing, but it is slow (it scans all
dialogs) and is deliberately excluded from CI. Run it explicitly:

```bash
cargo test --test hevc_proxy
```

## 8. Troubleshooting

**`Failed to load Telegram configuration: TELEGRAM_API_ID not set …`**
Create `.env` from `.env.example` (step 3) and rebuild, or export the variables.

**`ld: library not found for -lmpv` (macOS)**
Homebrew's `mpv` is not installed, or it is in a non-standard prefix. `brew install mpv`,
or set `LIBRARY_PATH=/path/to/lib cargo build`.

**`error while loading shared libraries: libmpv.so…` (Linux)**
The runtime loader cannot find libmpv — install the runtime package (`libmpv2` /
`mpv-libs`) or set `LD_LIBRARY_PATH`.

**`Library not loaded: @rpath/libmpv.2.dylib` when opening the `.app`**
The bundle was built before the dylib step or was tampered with. Re-run
`scripts/bundle-macos.sh`; the script's verification step fails loudly if the bundle is
not self-contained.

**macOS says the app "is damaged" or is from an unidentified developer**
That is Gatekeeper on an ad-hoc signed, quarantined app. Right-click → Open, or:

```bash
xattr -dr com.apple.quarantine /Applications/Ammini.app
```

**`warning: the following packages contain code that will be rejected by a future
version of Rust: proc-macro-error2`**
Harmless and deliberately ignored: it comes from `statig`'s macro dependency, whose repo
is archived. Do not try to "fix" it by bumping or patching dependencies.

## 9. Project layout

```
src/
  main.rs                 eframe app, window setup, top bar, track switchers
  fsm.rs                  PlayerFsm (player state machine) + persistence
  proxy.rs                helper that builds proxied URLs for the player
  style.rs                macOS-style theme and palette helpers
  fonts.rs                font stack + Material icon label helpers
  telegram/
    panel.rs              Telegram sidebar UI (auth, chat list, messages)
    state_machine.rs      TelegramFsm
    client.rs             grammers client + VideoSource abstraction
    proxy.rs              axum proxy (remote URLs + Telegram video streaming)
    cache.rs              disk-backed 512 KiB block cache
    session.rs, config.rs Telegram session storage and credentials
vendor/egui-sharkplayer/  patched upstream crate (mpv property accessor, time labels)
assets/                   fonts, logo, generated icons
scripts/bundle-macos.sh   .app + .dmg builder
tests/                    offline and live integration tests
```
