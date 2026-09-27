# Building Ammini for Windows (x64)

Step-by-step instructions to produce a portable Windows **x86_64** build of Ammini, plus a
ready-to-use GitHub Actions workflow. This is the exact sequence that was used to build
and run the app on Windows.

> **Always build the x64 target.** Ammini renders video through mpv's **OpenGL render
> API**. The standard ARM64 Windows mpv builds pass `-Dgl=disabled` (because ANGLE does
> not support Windows on ARM), so `mpv_render_context_create` returns
> `MPV_ERROR_NOT_IMPLEMENTED (-19)` and the player cannot start. Windows 11 emulates x64,
> so an ARM64 machine runs the x64 build fine.

## What the build needs

| Piece | Why | Where it comes from |
|---|---|---|
| Rust 1.92+ with the `x86_64-pc-windows-msvc` target | The app is edition 2024; MSVC is the supported Windows toolchain | rustup |
| MSVC C++ build tools (x64) | Links the exe and compiles bundled C deps (SQLite) | Visual Studio Build Tools, "Desktop development with C++" |
| `libmpv` dev package (x64) | `libmpv2-sys` links against `mpv.lib`; the DLL is loaded at runtime | prebuilt mpv dev package (below) |

`build.rs` does the platform glue automatically: it finds the import library, adds it to
the linker search path, and copies the runtime DLLs next to the executable. It looks, in
order, at `$MPV_LINK_DIR`, then `libmpv-x64` (or `libmpv-arm64`/`libmpv-x86`), then
`libmpv`, at the repository root.

## Step 1 — Rust and the MSVC toolchain

```powershell
rustup target add x86_64-pc-windows-msvc
```

Install **Visual Studio Build Tools 2022** with the "Desktop development with C++"
workload (the standard `windows-latest` GitHub runner already has it).

## Step 2 — libmpv dev package

Download a prebuilt mpv **dev** package, verify its checksum, unpack it at the repo root
as `libmpv-x64`, and create the `mpv.lib` the MSVC linker expects:

```powershell
$mpvUrl = "https://github.com/dyphire/mpv-winbuild/releases/download/mpv_own-2026-08-31/mpv-dev-x86_64-20260831-git-02a595ddc1.7z"
$mpvSha = "90cafccef4894f071bdee208cae70beab352bb05de8b1d3427402846534e0122"

Invoke-WebRequest -Uri $mpvUrl -OutFile "$env:TEMP\mpv-dev-x64.7z"
$hash = (Get-FileHash "$env:TEMP\mpv-dev-x64.7z" -Algorithm SHA256).Hash.ToLower()
if ($hash -ne $mpvSha) { throw "libmpv checksum mismatch: $hash" }

7z x "$env:TEMP\mpv-dev-x64.7z" -olibmpv-x64 -y

# libmpv2-sys asks the linker for a bare `mpv`, i.e. `mpv.lib`. The package ships a
# MinGW import library named libmpv.dll.a; MSVC links it under the expected name.
Copy-Item libmpv-x64\libmpv.dll.a libmpv-x64\mpv.lib
```

Other sources with the same layout: [zhongfly/mpv-winbuild](https://github.com/zhongfly/mpv-winbuild/releases),
[shinchiro's builds](https://sourceforge.net/projects/mpv-player-windows/files/libmpv/)
(SourceForge can be flaky from CI). The dev package contains `libmpv-2.dll`,
`libmpv.dll.a`, `lua51.dll`, `vulkan-1.dll` and `include/`. `libmpv-x64/` is gitignored.

> The pin rots: refresh the URL/sha256 when a newer mpv is wanted (client API 2.x is
> stable). The `-lgpl` variants are drop-in if licensing ever matters; the default is GPL.

## Step 3 — Credentials (optional at build time)

`build.rs` bakes `TELEGRAM_API_ID` / `TELEGRAM_API_HASH` / `SENTRY_DSN` /
`SENTRY_OTLP_URL` into the binary from the environment or a `.env` file, so the produced
exe needs no `.env` beside it. Provide them in the environment for CI. Without them the
app still builds but exits at startup asking for credentials.

```powershell
# Local: put them in .env (gitignored) or export them, e.g.
$env:TELEGRAM_API_ID = "1234567"
$env:TELEGRAM_API_HASH = "0123456789abcdef0123456789abcdef"
```

## Step 4 — Build

```powershell
cargo build --release --target x86_64-pc-windows-msvc
```

Output: `target\x86_64-pc-windows-msvc\release\ammini.exe`, with `libmpv-2.dll`,
`lua51.dll` and `vulkan-1.dll` staged next to it by `build.rs`.

## Step 5 — Run / smoke test

The app needs an OpenGL 2.0+ context. Machines with a GPU driver are fine. On a VM or a
headless box (including GitHub runners) Windows' built-in `opengl32.dll` is only OpenGL
1.1, so drop a software implementation next to the exe:

```powershell
# Mesa llvmpipe (software OpenGL) for the current architecture.
Invoke-WebRequest "https://github.com/mmozeiko/build-mesa/releases/download/26.2.3/mesa-llvmpipe-x64-26.2.3.7z" -OutFile "$env:TEMP\mesa.7z"
7z x "$env:TEMP\mesa.7z" -o"$env:TEMP\mesa" -y
Copy-Item "$env:TEMP\mesa\opengl32.dll" "target\x86_64-pc-windows-msvc\release\"
```

mpv logs `Suspected software renderer or indirect context` and renders normally. The CI
workflow publishes this DLL as a separate `*-software-gl` artifact, so on a GPU-less
machine unzip it over an extracted portable package:

```powershell
Expand-Archive Ammini-0.1.3-x64.zip -DestinationPath Ammini
Expand-Archive Ammini-0.1.3-x64-software-gl.zip -DestinationPath Ammini   # overlay
.\Ammini\ammini.exe
```

## Step 6 — Package

Ship a portable folder/zip containing the exe and the DLLs next to it:

```powershell
$out = "target\x86_64-pc-windows-msvc\release"
$dist = "dist\Ammini-x64"
New-Item -ItemType Directory -Force -Path $dist | Out-Null
Copy-Item "$out\ammini.exe" $dist
foreach ($dll in "libmpv-2.dll","lua51.dll","vulkan-1.dll") {
    Copy-Item "$out\$dll" $dist -ErrorAction SilentlyContinue
}
# Add opengl32.dll here too if the target machine has no GPU driver.
$version = (Select-String -Path Cargo.toml -Pattern '^version\s*=\s*"([^"]+)"').Matches[0].Groups[1].Value
Compress-Archive -Path "$dist\*" -DestinationPath "dist\Ammini-$version-x64.zip" -Force
```

## GitHub Actions workflow

Save as `.github/workflows/build-windows.yml`. It builds on every tag and on manual
dispatch, uploads the portable folder as an artifact, and attaches a versioned zip to the
GitHub Release on tags. It also publishes a second, optional
`Ammini-<version>-x64-software-gl` artifact with Mesa llvmpipe's `opengl32.dll` for
machines without an OpenGL 2.0+ driver (see Step 5). That DLL is kept **out** of the main
package on purpose — an `opengl32.dll` next to the exe takes precedence over the real
driver, so it must stay opt-in.

```yaml
name: build-windows

on:
  workflow_dispatch:
  push:
    tags:
      - "v*"

permissions:
  contents: write

env:
  MPV_DEV_URL: https://github.com/dyphire/mpv-winbuild/releases/download/mpv_own-2026-08-31/mpv-dev-x86_64-20260831-git-02a595ddc1.7z
  MPV_DEV_SHA256: 90cafccef4894f071bdee208cae70beab352bb05de8b1d3427402846534e0122

jobs:
  build:
    runs-on: windows-latest
    steps:
      - uses: actions/checkout@v4

      - uses: dtolnay/rust-toolchain@stable
        with:
          targets: x86_64-pc-windows-msvc

      - uses: Swatinem/rust-cache@v2
        with:
          key: windows-x64

      - name: Provision libmpv
        shell: pwsh
        run: |
          $archive = "$env:RUNNER_TEMP\mpv-dev.7z"
          Invoke-WebRequest -Uri $env:MPV_DEV_URL -OutFile $archive
          $hash = (Get-FileHash $archive -Algorithm SHA256).Hash.ToLower()
          if ($hash -ne $env:MPV_DEV_SHA256) { throw "libmpv checksum mismatch: $hash" }
          7z x $archive -olibmpv-x64 -y
          Copy-Item libmpv-x64\libmpv.dll.a libmpv-x64\mpv.lib

      - name: Build
        shell: pwsh
        env:
          TELEGRAM_API_ID: ${{ secrets.TELEGRAM_API_ID }}
          TELEGRAM_API_HASH: ${{ secrets.TELEGRAM_API_HASH }}
          SENTRY_DSN: ${{ secrets.SENTRY_DSN }}
          SENTRY_OTLP_URL: ${{ secrets.SENTRY_OTLP_URL }}
        run: cargo build --release --target x86_64-pc-windows-msvc

      - name: Package
        shell: pwsh
        run: |
          $out = "target\x86_64-pc-windows-msvc\release"
          $dist = "dist\Ammini-x64"
          New-Item -ItemType Directory -Force -Path $dist | Out-Null
          Copy-Item "$out\ammini.exe" $dist
          foreach ($dll in "libmpv-2.dll","lua51.dll","vulkan-1.dll") {
            Copy-Item "$out\$dll" $dist -ErrorAction SilentlyContinue
          }
          $version = (Select-String -Path Cargo.toml -Pattern '^version\s*=\s*"([^"]+)"').Matches[0].Groups[1].Value
          Compress-Archive -Path "$dist\*" -DestinationPath "dist\Ammini-$version-x64.zip" -Force

      - uses: actions/upload-artifact@v4
        with:
          name: Ammini-x64
          path: dist/*.zip

      - name: Attach to release
        if: startsWith(github.ref, 'refs/tags/')
        shell: pwsh
        env:
          GH_TOKEN: ${{ github.token }}
        run: gh release upload $env:GITHUB_REF_NAME (Get-ChildItem dist\*.zip).FullName --clobber
```

Set the four credentials as repository secrets (`gh secret set TELEGRAM_API_ID`, …).
Tags/releases created by `GITHUB_TOKEN` do not trigger other workflows, so for a
`release-plz`-made tag dispatch this one manually:

```bash
gh workflow run build-windows.yml --ref v<version>
```

## Gotchas

- **TLS backend.** On Windows the app uses `native-tls` (Schannel); elsewhere it uses
  rustls + ring. `aws-lc-rs` (reqwest's default rustls provider) cannot build on
  `aarch64-pc-windows-msvc`, and `ring` needs `clang` there — hence the split. See
  `Cargo.toml`.
- **Baked credentials.** A built exe contains whatever credentials were present at build
  time (recoverable with `strings`). Treat release artifacts accordingly.
- **`cargo test` on Windows.** The offline suites pass; the `proxy_video` test
  `prefetch_downloads_future_blocks_in_parallel` is timing-sensitive and can fail under
  x64 emulation (a real x64 host is fine).
- **Icon / version info.** The exe icon and `ProductName`/`FileVersion` are embedded by
  `winresource` (a build-dependency) from `assets/ammini.ico`. Regenerate that file with
  `powershell -ExecutionPolicy Bypass -File scripts/make-ico.ps1` after changing the
  artwork. This needs a Windows resource compiler (`rc.exe`, shipped with the Windows
  SDK / VS Build Tools).
