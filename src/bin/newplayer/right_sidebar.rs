//! Right media inspector overlay (independent component).
use crate::theme::{C_BORDER2, C_DIM, C_MUTED, C_SIDEBAR};
use eframe::egui;

/// Renders the right inspector overlay. Returns `true` when the user clicked outside
/// of it (so the caller can hide the panel).
pub fn show(
    ctx: &egui::Context,
    proxy_url: Option<&str>,
    pos_line: Option<String>,
    info_lines: &[String],
    header_bottom: f32,
    navrail_right: f32,
) -> bool {
    let insp_resp = egui::Area::new("inspector_overlay".into())
        .anchor(egui::Align2::RIGHT_TOP, egui::vec2(-16.0, 0.0))
        .show(ctx, |ui| {
            egui::Frame::new()
                .fill(C_SIDEBAR)
                .stroke(egui::Stroke::new(1.0, C_BORDER2))
                .corner_radius(0.0)
                .inner_margin(egui::Margin {
                    left: 8,
                    right: 12,
                    top: 2,
                    bottom: 10,
                })
                .show(ui, |ui| {
                    ui.set_width(280.0);
                    ui.set_min_height(ctx.viewport_rect().height() - 8.0);
                    ui.set_max_height(ctx.viewport_rect().height() - 8.0);
                    ui.label(
                        egui::RichText::new("MEDIA INSPECTOR")
                            .monospace()
                            .size(11.0)
                            .strong()
                            .color(C_DIM),
                    );
                    if let Some(url) = proxy_url {
                        ui.label(
                            egui::RichText::new(format!("proxy: {}", url))
                                .monospace()
                                .size(11.0)
                                .color(C_MUTED),
                        );
                    }
                    if let Some(pos) = pos_line {
                        ui.label(
                            egui::RichText::new(pos)
                                .monospace()
                                .size(11.0)
                                .color(C_MUTED),
                        );
                    }
                    for line in info_lines {
                        ui.label(
                            egui::RichText::new(line)
                                .monospace()
                                .size(11.0)
                                .color(C_MUTED),
                        );
                    }
                });
        });

    ctx.input(|i| i.pointer.any_click())
        && ctx.input(|i| {
            let pos = i.pointer.hover_pos();
            let in_overlay = pos
                .map(|p| insp_resp.response.rect.contains(p))
                .unwrap_or(false);
            let in_navrail = pos
                .map(|p| p.x < navrail_right && p.y >= 40.0)
                .unwrap_or(false);
            let in_toolbar = pos.map(|p| p.y < header_bottom).unwrap_or(false);
            !in_overlay && !in_navrail && !in_toolbar
        })
}
