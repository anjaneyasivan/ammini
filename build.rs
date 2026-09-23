use std::collections::HashMap;
use std::path::Path;

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
