use std::collections::HashMap;
use std::path::{Path, PathBuf};

fn main() {
    bake_dotenv();

    #[cfg(target_os = "macos")]
    {
        for dir in ["/opt/homebrew/lib", "/usr/local/lib"] {
            if std::path::Path::new(dir).exists() {
                println!("cargo:rustc-link-search={dir}");
                println!("cargo:rustc-link-arg=-Wl,-rpath,{dir}");
                break;
            }
        }
    }

    #[cfg(target_os = "windows")]
    windows_libmpv();
}

/// Point the MSVC linker at libmpv's import library and stage the runtime DLL next
/// to the executable.
///
/// `libmpv2-sys` emits only `cargo:rustc-link-lib=mpv`, so the linker looks for
/// `mpv.lib` on its search path and the app must supply the directory. Resolution
/// order is `$MPV_LINK_DIR`, then an architecture-specific `libmpv-<arch>` directory at
/// the package root (where a downloaded dev package is unpacked), then a shared
/// `libmpv`. The import library must match the target architecture.
///
/// The runtime DLLs from that directory are copied next to the built executable so
/// plain `cargo run` works without touching `PATH` (Windows' loader searches the exe's
/// directory first).
#[cfg(target_os = "windows")]
fn windows_libmpv() {
    let manifest_dir = std::env::var("CARGO_MANIFEST_DIR").unwrap_or_default();
    let dir = match std::env::var("MPV_LINK_DIR") {
        Ok(dir) => PathBuf::from(dir),
        Err(_) => default_libmpv_dir(Path::new(&manifest_dir)),
    };
    println!("cargo:rerun-if-env-changed=MPV_LINK_DIR");

    if !dir.join("mpv.lib").exists() && !dir.join("libmpv.dll.a").exists() {
        panic!(
            "Windows build: no libmpv import library found in {}. Download an mpv dev \
             package matching the target architecture, unpack it there, or point \
             MPV_LINK_DIR at it. See BUILDING.md.",
            dir.display()
        );
    }

    println!("cargo:rustc-link-search=native={}", dir.display());

    stage_runtime_dlls(&dir);
}

/// Default libmpv directory for the target architecture: prefer `libmpv-<arch>` (e.g.
/// `libmpv-x64`, `libmpv-arm64`) and fall back to a shared `libmpv`.
#[cfg(target_os = "windows")]
fn default_libmpv_dir(manifest_dir: &Path) -> PathBuf {
    let target_arch = std::env::var("CARGO_CFG_TARGET_ARCH");
    let arch = match target_arch.as_deref() {
        Ok("x86_64") => "x64",
        Ok("aarch64") => "arm64",
        Ok("x86") => "x86",
        Ok(other) => other,
        Err(_) => "x64",
    };
    let arch_dir = manifest_dir.join(format!("libmpv-{arch}"));
    if arch_dir.exists() {
        arch_dir
    } else {
        manifest_dir.join("libmpv")
    }
}

/// Copy every DLL from `dir` into the directory holding the built executable (the
/// profile dir, e.g. `target/debug`), so the loader finds libmpv and its sibling
/// dependencies (`lua51.dll`, `vulkan-1.dll`, …). `OUT_DIR` is
/// `<target>/[<triple>/]<profile>/build/<pkg>-<hash>/out`, so the profile dir is its
/// 3rd ancestor.
#[cfg(target_os = "windows")]
fn stage_runtime_dlls(dir: &Path) {
    println!("cargo:rerun-if-changed={}", dir.display());

    let Ok(out_dir) = std::env::var("OUT_DIR") else {
        return;
    };
    let Some(profile_dir) = Path::new(&out_dir).ancestors().nth(3) else {
        return;
    };

    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        let is_dll = path
            .extension()
            .and_then(|e| e.to_str())
            .is_some_and(|e| e.eq_ignore_ascii_case("dll"));
        if !is_dll {
            continue;
        }
        let Some(name) = path.file_name() else {
            continue;
        };
        let dest = profile_dir.join(name);
        // Skip the copy when the destination already has the same size, so repeated
        // builds don't rewrite an 80+ MB file.
        if std::fs::metadata(&dest).map(|m| m.len()).ok()
            == std::fs::metadata(&path).map(|m| m.len()).ok()
        {
            continue;
        }
        match std::fs::copy(&path, &dest) {
            Ok(_) => println!(
                "cargo:warning=staged {} -> {}",
                name.to_string_lossy(),
                dest.display()
            ),
            Err(e) => println!(
                "cargo:warning=failed to stage {}: {e}",
                name.to_string_lossy()
            ),
        }
    }
}

/// Bake the credentials from `.env` into the binary, so a built binary runs
/// without a `.env` file next to it.
///
/// Each key is passed to rustc as a compile-time environment variable, which
/// `config.rs` reads via `option_env!`. A real environment variable wins over
/// the file, both here and at run time. With no `.env` (e.g. CI) nothing is
/// baked and the values must come from the environment as before.
fn bake_dotenv() {
    let manifest_dir = std::env::var("CARGO_MANIFEST_DIR").unwrap_or_default();
    let dotenv_path = Path::new(&manifest_dir).join(".env");

    // `.env` is gitignored, so cargo would not notice edits to it on its own.
    println!("cargo:rerun-if-changed={}", dotenv_path.display());

    let dotenv = parse_env_file(&dotenv_path);

    for key in [
        "TELEGRAM_API_ID",
        "TELEGRAM_API_HASH",
        "SENTRY_DSN",
        "SENTRY_OTLP_URL",
    ] {
        println!("cargo:rerun-if-env-changed={key}");
        if let Some(value) = std::env::var(key).ok().or_else(|| dotenv.get(key).cloned()) {
            println!("cargo:rustc-env={key}={value}");
        }
    }
}

/// Minimal `KEY=VALUE` parser: skips blanks and `#` comments, tolerates an
/// `export ` prefix and one layer of matching quotes.
fn parse_env_file(path: &Path) -> HashMap<String, String> {
    let mut map = HashMap::new();
    let Ok(contents) = std::fs::read_to_string(path) else {
        return map;
    };

    for line in contents.lines() {
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        let line = line.strip_prefix("export ").unwrap_or(line);
        let Some((key, value)) = line.split_once('=') else {
            continue;
        };
        let mut value = value.trim();
        if value.len() >= 2
            && ((value.starts_with('"') && value.ends_with('"'))
                || (value.starts_with('\'') && value.ends_with('\'')))
        {
            value = &value[1..value.len() - 1];
        }
        map.insert(key.trim().to_owned(), value.to_owned());
    }

    map
}
