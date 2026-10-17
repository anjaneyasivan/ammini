mod app;
mod hud;
mod iconify;
mod left_sidebar;
mod overlays;
mod right_sidebar;
mod theme;
mod top_bar;
mod types;
mod ui;

use crate::theme::*;
use app::NewPlayerApp;

const APP_KEY: &str = "min_mpv_state";
const VIDEO_EXTS: &[&str] = &["mp4", "mkv", "avi", "mov", "webm", "ogv", "flv"];

// Re-exported for convenience.
pub use types::*;

fn main() -> eframe::Result<()> {
    let filter = tracing_subscriber::EnvFilter::try_from_default_env()
        .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("ammini=debug"));
    tracing_subscriber::fmt().with_env_filter(filter).init();

    ammini::telemetry::install_panic_hook();
    let _telemetry = ammini::telemetry::start();
    ammini::telemetry::emit(ammini::telemetry::TelemetryEvent::AppStarted {
        version: env!("CARGO_PKG_VERSION").to_string(),
    });

    let icon = eframe::icon_data::from_png_bytes(include_bytes!("../assets/ammini-icon.png"))
        .expect("assets/ammini-icon.png must be a valid PNG");

    let options = eframe::NativeOptions {
        renderer: eframe::Renderer::Glow,
        viewport: eframe::egui::ViewportBuilder::default()
            .with_inner_size([1280.0, 800.0])
            .with_icon(icon),
        ..Default::default()
    };
    eframe::run_native(
        "Ammini",
        options,
        Box::new(|cc| {
            let app = NewPlayerApp::new(cc)?;
            Ok(Box::new(app))
        }),
    )
}
