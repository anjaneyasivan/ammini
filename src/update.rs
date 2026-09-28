//! Startup "new version available" check.
//!
//! Ammini does **not** download or install updates: it fetches a tiny JSON
//! manifest published by the website deploy workflow (see
//! `.github/workflows/website-deploy.yml`) from the project's GitHub Pages site
//! and, if it names a newer release, tells the UI to show an alert that links to
//! the GitHub Release. A real self-updater would need signed/notarized builds
//! first (the macOS bundle is ad-hoc signed), so this stays notification-only.
//!
//! The check runs once per launch on its own thread, a few seconds after
//! startup, so it never blocks or competes with window/video/Telegram startup.
//! Failures (offline, 404 while no release is published, malformed manifest) are
//! logged at debug/warn and otherwise ignored — the app must behave identically
//! when the manifest is unreachable.

use std::time::Duration;

use eframe::egui;
use serde::Deserialize;
use tokio::sync::mpsc::{UnboundedReceiver, UnboundedSender};

/// Manifest published to GitHub Pages by the website deploy workflow, at
/// `https://anjaneyasivan.github.io/ammini/update.json` (the project site is served
/// under `/ammini`). Override with `AMMINI_UPDATE_URL` for local testing.
const DEFAULT_MANIFEST_URL: &str = "https://anjaneyasivan.github.io/ammini/update.json";

/// Fallback link when the manifest omits `url`.
const RELEASES_URL: &str = "https://github.com/anjaneyasivan/ammini/releases/latest";

/// Wait before the first (and only) check, so a slow network never delays or
/// competes with the app coming up.
const STARTUP_DELAY: Duration = Duration::from_secs(5);

/// Per-request timeout; the manifest is a few hundred bytes.
const REQUEST_TIMEOUT: Duration = Duration::from_secs(15);

/// The subset of the manifest the app reads. Unknown fields are ignored, so the
/// site can carry extra release metadata (download URLs, notes) safely.
#[derive(Debug, Deserialize)]
struct Manifest {
    /// Bare release version, e.g. `0.1.7` (no leading `v`).
    version: String,
    /// Human-facing link to the release; defaults to the releases page.
    #[serde(default)]
    url: Option<String>,
}

/// A newer release the user should be told about.
#[derive(Debug, Clone)]
pub struct UpdateAvailable {
    /// Version currently running (`CARGO_PKG_VERSION`).
    pub current: String,
    /// Newer version named by the manifest.
    pub latest: String,
    /// Where to send the user to get it.
    pub url: String,
}

/// Spawn the one-shot background check and return the channel the UI drains each
/// frame. Sends at most one [`UpdateAvailable`], and wakes the UI (egui only
/// repaints on demand) so the alert appears even while the window is idle.
pub fn start(ctx: egui::Context) -> UnboundedReceiver<UpdateAvailable> {
    let (tx, rx) = tokio::sync::mpsc::unbounded_channel();
    let url = manifest_url();
    if let Err(e) = std::thread::Builder::new()
        .name("ammini-update-check".to_owned())
        .spawn(move || run(url, tx, ctx))
    {
        tracing::warn!("failed to spawn update check: {e}");
    }
    rx
}

/// Manifest URL from `AMMINI_UPDATE_URL`, falling back to the published one.
fn manifest_url() -> String {
    std::env::var("AMMINI_UPDATE_URL")
        .ok()
        .map(|s| s.trim().to_owned())
        .filter(|s| !s.is_empty())
        .unwrap_or_else(|| DEFAULT_MANIFEST_URL.to_owned())
}

fn run(url: String, tx: UnboundedSender<UpdateAvailable>, ctx: egui::Context) {
    let rt = match tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
    {
        Ok(rt) => rt,
        Err(e) => {
            tracing::warn!("update check runtime failed: {e}");
            return;
        }
    };
    rt.block_on(async move {
        tokio::time::sleep(STARTUP_DELAY).await;
        let client = match reqwest::Client::builder().timeout(REQUEST_TIMEOUT).build() {
            Ok(client) => client,
            Err(e) => {
                tracing::warn!("update check client failed: {e}");
                return;
            }
        };
        let body = match client
            .get(&url)
            .header(
                reqwest::header::USER_AGENT,
                concat!("ammini/", env!("CARGO_PKG_VERSION")),
            )
            .header(reqwest::header::ACCEPT, "application/json")
            .send()
            .await
            .and_then(reqwest::Response::error_for_status)
        {
            Ok(response) => match response.text().await {
                Ok(text) => text,
                Err(e) => {
                    tracing::warn!("update check read failed: {e}");
                    return;
                }
            },
            // Offline is the normal case for a video player; stay quiet.
            Err(e) => {
                tracing::debug!("update check skipped: {e}");
                return;
            }
        };
        match evaluate(&body, env!("CARGO_PKG_VERSION")) {
            Ok(Some(update)) => {
                tracing::info!("update available: {} -> {}", update.current, update.latest);
                let _ = tx.send(update);
                ctx.request_repaint();
            }
            Ok(None) => tracing::debug!("update check: up to date"),
            Err(e) => tracing::warn!("update check parse failed: {e}"),
        }
    });
}

/// Parse a manifest and decide whether `current` is out of date. Split from the
/// network path so it can be unit tested.
fn evaluate(body: &str, current: &str) -> Result<Option<UpdateAvailable>, String> {
    let manifest: Manifest = serde_json::from_str(body).map_err(|e| e.to_string())?;
    let latest = semver::Version::parse(normalize(&manifest.version)).map_err(|e| e.to_string())?;
    let current = semver::Version::parse(normalize(current)).map_err(|e| e.to_string())?;
    if latest <= current {
        return Ok(None);
    }
    Ok(Some(UpdateAvailable {
        current: current.to_string(),
        latest: latest.to_string(),
        url: manifest
            .url
            .filter(|u| !u.trim().is_empty())
            .unwrap_or_else(|| RELEASES_URL.to_owned()),
    }))
}

/// Trim surrounding whitespace and an optional leading `v`/`V`. The manifest is
/// published with a bare version, but the git tag (and so a hand-edited manifest)
/// carries the `v` prefix that `semver` rejects.
fn normalize(raw: &str) -> &str {
    raw.trim().trim_start_matches(['v', 'V'])
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn newer_version_is_reported() {
        let update = evaluate(r#"{"version":"0.2.0"}"#, "0.1.6")
            .unwrap()
            .expect("0.2.0 > 0.1.6");
        assert_eq!(update.latest, "0.2.0");
        assert_eq!(update.current, "0.1.6");
        assert_eq!(update.url, RELEASES_URL);
    }

    #[test]
    fn same_or_older_version_is_not_reported() {
        assert!(
            evaluate(r#"{"version":"0.1.6"}"#, "0.1.6")
                .unwrap()
                .is_none()
        );
        assert!(
            evaluate(r#"{"version":"0.1.5"}"#, "0.1.6")
                .unwrap()
                .is_none()
        );
    }

    #[test]
    fn prerelease_of_next_minor_is_newer() {
        // semver: 0.2.0-rc.1 < 0.2.0, but still > 0.1.6.
        let update = evaluate(r#"{"version":"0.2.0-rc.1"}"#, "0.1.6")
            .unwrap()
            .expect("0.2.0-rc.1 > 0.1.6");
        assert_eq!(update.latest, "0.2.0-rc.1");
    }

    #[test]
    fn manifest_url_overrides_the_default() {
        let update = evaluate(
            r#"{"version":"9.9.9","url":"https://example.test/release"}"#,
            "0.1.6",
        )
        .unwrap()
        .expect("9.9.9 > 0.1.6");
        assert_eq!(update.url, "https://example.test/release");
    }

    #[test]
    fn whitespace_and_v_prefix_are_tolerated_in_values() {
        // The published manifest is bare (`0.1.7`), but the git tag carries `v`, so
        // tolerate both if a human edits the manifest.
        let update = evaluate(r#"{"version":" v0.2.0 "}"#, " 0.1.6 ")
            .unwrap()
            .expect("v0.2.0 > 0.1.6");
        assert_eq!(update.latest, "0.2.0");
    }

    #[test]
    fn malformed_manifest_is_an_error_not_a_panic() {
        assert!(evaluate("not json", "0.1.6").is_err());
        assert!(evaluate(r#"{"version":"not-a-version"}"#, "0.1.6").is_err());
        assert!(evaluate(r#"{"tag":"v0.2.0"}"#, "0.1.6").is_err());
    }

    #[test]
    fn blank_url_falls_back_to_the_releases_page() {
        let update = evaluate(r#"{"version":"0.2.0","url":"  "}"#, "0.1.6")
            .unwrap()
            .expect("0.2.0 > 0.1.6");
        assert_eq!(update.url, RELEASES_URL);
    }
}
