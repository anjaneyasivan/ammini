# Windows support — research & plan

> **Status: research only, not implemented.** Written 2026-09-25 after a codebase audit +
> web research, so the implementation can start from facts instead of re-doing the
> investigation. Work items are listed in "Suggested order of work" at the bottom.

## TL;DR

Ammini is already ~95% Windows-clean: the only platform conditional in the whole tree is
the Homebrew link-path block in `build.rs`, and every dependency that matters (eframe/glow,
the vendored egui-sharkplayer, grammers, keepawake, reqwest) has a native Windows backend.
Adding Windows support means:

1. One small `build.rs` addition so the linker can find libmpv's import library.
2. Providing `libmpv-2.dll` at runtime (download a prebuilt dev package; no compilation).
3. A Windows system-font fallback table (otherwise Indic/Arabic/Thai/CJK text is tofu).
4. An `.ico` + exe resource embed (window/taskbar icon, version info).
5. Two CI workflows: a Windows test job, and a release job producing a portable zip.

There are no rewrite-level risks: the video path (mpv OpenGL render API + eframe glow)
is platform-neutral code and works over WGL on Windows; the one thing to validate early
is that runtime pairing (see "Risks").

## What already works cross-platform (audited 2026-09-25)

| Concern | Status |
|---|---|
| `build.rs` | Only block at build.rs:7-16 is `#[cfg(target_os = "macos")]` (Homebrew link-search + rpath). The dotenv baking (build.rs:26-46) is platform-neutral. |
| libmpv linking | `libmpv2-sys 4.0.1` emits **only** `cargo:rustc-link-lib=mpv` — no link-search path, no pkg-config. Whoever builds the app must supply the search path. Its pregenerated bindings are plain C ABI declarations, so they compile on MSVC without headers or bindgen. |
| Rendering | `vendor/egui-sharkplayer/src/backend.rs` uses libmpv2's render API (`RenderParamApiType::OpenGl`) with eframe's glow `get_proc_address` — no unix-specific code anywhere in the vendored crate (only the unused `wgpu` feature hardcodes `libEGL.so.1`; we don't enable it). WGL is glutin/eframe's supported Windows path. |
| Telegram | grammers' `SqliteSession` compiles its own SQLite (`libsql` → bundled C via cc) — no system sqlite setup needed on MSVC. Proxy binds `127.0.0.1:0`; `dirs` paths map to `%LOCALAPPDATA%`. |
| Display sleep | `keepawake 0.6` has a native Windows backend (`SetThreadExecutionState` via the `windows` crate). The `reason` field is ignored on Windows (harmless). |
| Shortcuts | `main.rs:271-284` uses `modifiers.command`, which egui maps to **Ctrl** on Windows — Ctrl+W/Ctrl+Q work without changes. |
| File dialogs / drag-drop | `rfd 0.17` uses IFileDialog natively; dropped-files input is winit-level. |
| Telemetry | OTLP over `reqwest` (rustls), `sentry` crate — pure Rust, fine on Windows. |
| Filesystem paths | Only two `dirs::` calls: session (`src/telegram/session.rs:8-12` → `%LOCALAPPDATA%\min-mpv\telegram.session`) and the block cache (`src/telegram/mod.rs:143-146` → `%LOCALAPPDATA%\cache\min-mpv\telegram_cache` — the literal `cache` subdir comes from `dirs::cache_dir()` on Windows; works, just unusually nested). |

## Required code changes

### 1. `build.rs` — Windows link search (the only *required* code change)

`libmpv2-sys` asks the linker for a bare `mpv`, so on MSVC the linker looks for `mpv.lib`.
The dev package ships a MinGW import library named `libmpv.dll.a` — content is a standard
COFF import library, so MSVC accepts it, but the **filename** must match what the linker
looks for. Plan:

```rust
#[cfg(target_os = "windows")]
{
    // libmpv2-sys emits only `cargo:rustc-link-lib=mpv`; point the linker at the
    // directory holding the import lib (libmpv.dll.a copied as mpv.lib, or a
    // mpv.lib generated from libmpv-2.def).
    let dir = std::env::var("MPV_LINK_DIR").unwrap_or_else(|_| "libmpv".into());
    println!("cargo:rustc-link-search=native={dir}");
    println!("cargo:rerun-if-env-changed=MPV_LINK_DIR");
}
```

Two acceptable sources of `mpv.lib`:

- `copy libmpv.dll.a mpv.lib` (works — GNU import libs are COFF archives MSVC links),
- or generate a native one: `lib.exe /def:libmpv-2.def /machine:X64 /out:mpv.lib`
  (lib.exe ships with VS Build Tools, which CI has anyway).

Fallback if MSVC fights the GNU import lib: build with the `x86_64-pc-windows-gnu`
toolchain, where `libmpv.dll.a` links naturally. MSVC should stay the primary target.

### 2. `src/fonts.rs` — Windows system-font fallbacks

`SYSTEM_FALLBACKS` (fonts.rs:40-99) currently only lists macOS `/System/Library/Fonts`
entries (plus two Linux paths). Each entry is `(name, path, ttc_font_index)` and loading
is existence-checked per entry (fonts.rs:128-144), so adding a Windows table is safe and
gracefully degrades. Candidates in `C:\Windows\Fonts`:

- `Nirmala.ttf` — one file covering most Indic scripts (Devanagari, Bengali, Gurmukhi,
  Gujarati, Tamil, Telugu, Kannada, Malayalam, Odia),
- `msyh.ttc` / `msjh.ttc` — Simplified/Traditional Chinese,
- `malgun.ttc` — Korean,
- `tahoma.ttf` or `segoeui.ttf` — Arabic/Thai/Hebrew basics.

Exactly which files to register (and their .ttc indexes) gets pinned down during
implementation, using the same `tests/emoji_support.rs` guard that asserts fonts parse.

### 3. Cache manifest rename — verify, don't assume it's broken

`src/telegram/cache.rs:216` rewrites the `.meta` manifest as write-temp + `fs::rename`.
The audit flagged Windows rename-over-destination, but modern Rust `std::fs::rename` uses
POSIX-semantics replacement on Windows (FileRenameInfoEx, MoveFileEx fallback) — it does
replace existing files. The real residual risk is a **sharing violation** if another
handle holds the destination open without `FILE_SHARE_DELETE`; our own opens go through
`std::fs::File`, which opens with full sharing, so it should hold. Action: add a tiny
retry/fallback (remove-then-rename) behind a debug log, and let the Windows CI test job
exercise `proxy_video` (which rewrites manifests) to prove it.

### 4. `.ico` + exe resources

The repo has no `.ico` (the window icon in `main.rs:962-969` is a PNG via
`eframe::icon_data`, which already works as a Windows *window* icon; this item is about
the **exe** icon in Explorer/taskbar and version info):

```bash
magick assets/ammini-icon.png -define icon:auto-resize=256,128,64,48,32,16 assets/ammini.ico
```

and in `build.rs` behind `#[cfg(windows)]`, using the `winresource` build-dependency:
set the icon + `ProductName`/`FileVersion` from `CARGO_PKG_*`.

### 5. Minor cleanups while there

- `src/main.rs:110-112` comment claims display-sleep is a "no-op on other platforms" —
  stale (keepawake has Windows/Linux backends); fix when touching the code.
- Optionally use `dirs::data_local_dir()` (instead of `cache_dir()`) for the block cache
  on Windows to avoid the `%LOCALAPPDATA%\cache\min-mpv\...` nesting.

## libmpv on Windows

No compilation needed — take a prebuilt dev package. Two sources were verified live:

| Source | URL | Notes |
|---|---|---|
| **dyphire/mpv-winbuild** (GitHub Releases) | `https://github.com/dyphire/mpv-winbuild/releases/download/<tag>/mpv-dev-x86_64-<date>-git-<sha>.7z` | Daily builds, **assets list sha256 checksums** — best for pinned CI downloads. Verified example: `mpv_own-2026-08-31` → `mpv-dev-x86_64-20260831-git-02a595ddc1.7z` (31.5 MB, sha256 `90cafccef4894f071bdee208cae70beab352bb05de8b1d3427402846534e0122`). |
| **shinchiro builds** (SourceForge, canonical) | `https://sourceforge.net/projects/mpv-player-windows/files/libmpv/` | Same packaging (mpv-dev-x86_64 / -x86_64-v3 / -i686 / -aarch64), updated ~biweekly. Cloudflare can make it flaky from CI; fine for local dev. |

`mpv-dev-x86_64-*.7z` contains exactly what we need:

- `libmpv-2.dll` — ship next to `ammini.exe` (self-contained; ffmpeg etc. linked in),
- `libmpv.dll.a` — MinGW import lib (→ rename to `mpv.lib` for MSVC, or use the .def),
- `libmpv-2.def` — export definitions (source for `lib.exe`),
- `include/mpv/*.h` — only needed for the optional `use-bindgen` feature; not used by us.

Variants worth knowing: `-x86_64-v3` (AVX2 baseline), `-i686` (32-bit — skip), `-aarch64`
(Windows-on-ARM, later), `mpv-dev-lgpl-*` (LGPL feature set — choose this variant if
licensing ever matters for distribution; the default build is GPL).

`hwdec=auto-safe` resolves to d3d11va on Windows with no extra DLLs; `d3dcompiler_47.dll`
is a system DLL on Windows 10+. Target floor: **Windows 10 x64**.

## Local Windows development

1. Rust MSVC toolchain + VS Build Tools (C++ workload).
2. Download `mpv-dev-x86_64-*.7z` (link above), extract to e.g. `C:\mpv-dev`.
3. `copy C:\mpv-dev\libmpv.dll.a C:\mpv-dev\mpv.lib`.
4. `$env:MPV_LINK_DIR = "C:\mpv-dev"; cargo build --release`.
5. Copy `libmpv-2.dll` next to `target\release\ammini.exe` (or run from `C:\mpv-dev`),
   then `cargo run`.

These steps belong in BUILDING.md's Windows section once implemented.

## CI

7-Zip ships preinstalled on GitHub's Windows runners; `Get-FileHash` covers checksums.

**Test job** (add to `ci.yml`):

```yaml
test-windows:
  runs-on: windows-latest
  steps:
    - uses: actions/checkout@v4
    - uses: dtolnay/rust-toolchain@stable
    - uses: Swatinem/rust-cache@v2
    - name: Provision libmpv          # download pinned mpv-dev 7z, verify sha256, 7z x
      run: ...                        # (shared script/step with the release job)
    - run: cargo test --lib
    - run: cargo test --test emoji_support
    - run: cargo test --test proxy_video
```

This permanently covers the Windows-specific items (font table, manifest rename, linking).

**Release job** (`.github/workflows/build-windows.yml`, twin of `build-macos.yml`):
`workflow_dispatch` + `push: tags: v*`; pinned `MPV_DEV_URL` + `MPV_DEV_SHA256` env,
`actions/cache` on the extracted dir; `cargo build --release` with the same four secrets
(`TELEGRAM_API_ID/HASH`, `SENTRY_DSN/OTLP_URL`) baked by build.rs; stage
`dist/Ammini-x64/Ammini.exe + libmpv-2.dll`, `Compress-Archive` to
`Ammini-<version>-x64.zip`; upload as artifact (+ bundler-log-style output artifact for
cross-checking, like the macOS job) and attach to the GitHub Release on tag refs via
`gh release upload`.

Packaging strategy: **portable zip first**. An Inno Setup installer (start-menu entry,
uninstaller) is a nice later addition and doesn't block anything.

## Risks & open questions

1. **WGL + mpv render API interop** — the only genuinely untested runtime path. The code
   is platform-neutral, but do an early smoke test (headless build → run on a real
   Windows box or a Windows VM) before investing in packaging. If the OpenGL interop
   misbehaves, the fallback is mpv's `render.gl` with a context created the way other
   Rust players do it — no redesign, just glue.
2. **SourceForge from CI** can be flaky (Cloudflare); the dyphire GitHub releases with
   checksums avoid it. Pin + `actions/cache` so it rarely downloads at all.
3. **Pinned mpv-dev rots** — refresh the pin when mpv releases matter (client API 2.x is
   stable; `libmpv2 6` targets API 2.2+).
4. **LGPL vs GPL build** — only matters if Ammini is ever distributed with licensing
   constraints; the `-lgpl` dev package is the drop-in answer.

## Suggested order of work

1. `build.rs` Windows link-search + fix stale display-sleep comment (tiny).
2. `fonts.rs` Windows fallback table.
3. cache.rs manifest-rename fallback + `ci.yml` Windows test job → proves 1–3 for free.
4. `.ico` + `winresource` embed.
5. `build-windows.yml` release job (portable zip → artifact → release attach).
6. Docs: BUILDING.md Windows section expansion, README "Windows 10+ (x64)" line.
7. Later/optional: Inno Setup installer, aarch64 build, installer code-signing.

## Sources

- [dyphire/mpv-winbuild releases](https://github.com/dyphire/mpv-winbuild/releases) (checksummed mpv-dev assets)
- [shinchiro mpv builds — SourceForge libmpv](https://sourceforge.net/projects/mpv-player-windows/files/libmpv/) / [build repo](https://github.com/shinchiro/mpv-winbuild-cmake)
- [libmpv2 crate](https://docs.rs/libmpv2) (linking delegated to the app; pregenerated bindings)
- [keepawake-rs](https://github.com/segevfiner/keepawake-rs) (Windows backend)
- [std::fs::rename docs](https://doc.rust-lang.org/std/fs/fn.rename.html) (Windows POSIX-semantics replacement)
- [dirs 5](https://docs.rs/dirs/5/dirs/fn.cache_dir.html) (`%LOCALAPPDATA%\cache` on Windows)
- [GitHub Actions Windows runner images](https://github.com/actions/runner-images) (7-Zip preinstalled)
