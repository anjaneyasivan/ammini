//! Floating top action bar (navigation toggles) shown with the HUD.
use crate::theme::{C_ACCENT, C_DIM, C_RAISED, C_RAISED2, C_TEXT};
use eframe::egui;

/// Renders the top-right action buttons. Toggles the passed-in flags in place.
pub fn show(
    ctx: &egui::Context,
    nav_rail_open: &mut bool,
    chats_open: &mut bool,
    inspector_open: &mut bool,
    alpha: f32,
) {
    egui::Area::new("top_header".into())
        .order(egui::Order::Tooltip)
        .fixed_pos(egui::pos2(0.0, 8.0))
        .show(ctx, |ui| {
            ui.set_opacity(alpha);
            ui.set_width(ctx.viewport_rect().width());
            ui.horizontal(|ui| {
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    ui.add_space(12.0);
                    for (id, icon_bytes, on) in [
                        (
                            "nav",
                            &include_bytes!("../assets/icons/newplayer/kl-layers-fill.svg")[..],
                            *nav_rail_open,
                        ),
                        (
                            "chat",
                            &include_bytes!("../assets/icons/newplayer/kl-send-fill.svg")[..],
                            *chats_open,
                        ),
                        (
                            "insp",
                            &include_bytes!("../assets/icons/newplayer/kl-info-fill.svg")[..],
                            *inspector_open,
                        ),
                    ] {
                        let fill = if on { C_RAISED2 } else { C_RAISED };
                        let txt_col = if on { C_TEXT } else { C_DIM };
                        let btn = ui
                            .scope(|ui| {
                                let v = &mut ui.style_mut().visuals;
                                v.widgets.inactive.weak_bg_fill = fill;
                                v.widgets.inactive.bg_fill = fill;
                                v.widgets.inactive.bg_stroke = egui::Stroke::NONE;
                                v.widgets.hovered.weak_bg_fill = C_ACCENT.linear_multiply(0.15);
                                v.widgets.hovered.bg_fill = C_ACCENT.linear_multiply(0.15);
                                v.widgets.hovered.bg_stroke = egui::Stroke::NONE;
                                v.widgets.hovered.fg_stroke.color = txt_col;
                                ui.add_sized(
                                    egui::vec2(36.0, 36.0),
                                    egui::Button::image(
                                        egui::Image::from_bytes("icon.svg", icon_bytes)
                                            .fit_to_exact_size(egui::vec2(16.0, 16.0)),
                                    )
                                    .stroke(egui::Stroke::NONE),
                                )
                                .on_hover_text(match id {
                                    "nav" => "Navigation rail",
                                    "chat" => "Chats",
                                    "insp" => "Inspector",
                                    _ => "",
                                })
                            })
                            .inner;
                        if btn.clicked() {
                            match id {
                                "nav" => *nav_rail_open = !*nav_rail_open,
                                "chat" => *chats_open = !*chats_open,
                                "insp" => *inspector_open = !*inspector_open,
                                _ => {}
                            }
                        }
                    }
                });
            });
        });
}
