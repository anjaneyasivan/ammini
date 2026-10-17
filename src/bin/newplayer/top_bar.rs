//! Floating top action bar (navigation toggles) shown with the HUD.
use crate::theme::{C_BORDER2, C_DIM, C_RAISED, C_RAISED2, C_TEXT};
use eframe::egui;
use egui_material_icons::icons::{ICON_INFO, ICON_LAYERS, ICON_MESSAGE};

/// Renders the top-right action buttons. Toggles the passed-in flags in place.
pub fn show(
    ctx: &egui::Context,
    nav_rail_open: &mut bool,
    chats_open: &mut bool,
    inspector_open: &mut bool,
) {
    egui::Area::new("top_header".into())
        .fixed_pos(egui::pos2(0.0, 0.0))
        .show(ctx, |ui| {
            ui.set_width(ctx.viewport_rect().width());
            ui.horizontal(|ui| {
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    ui.add_space(12.0);
                    for (id, icon, on) in [
                        ("nav", ICON_LAYERS, *nav_rail_open),
                        ("chat", ICON_MESSAGE, *chats_open),
                        ("insp", ICON_INFO, *inspector_open),
                    ] {
                        let fill = if on { C_RAISED2 } else { C_RAISED };
                        let stroke = if on {
                            egui::Stroke::new(1.0, egui::Color32::from_rgb(0x33, 0x33, 0x42))
                        } else {
                            egui::Stroke::new(1.0, C_BORDER2)
                        };
                        let txt_col = if on { C_TEXT } else { C_DIM };
                        let btn = ui
                            .scope(|ui| {
                                let v = &mut ui.style_mut().visuals;
                                v.widgets.hovered.weak_bg_fill = fill;
                                v.widgets.hovered.bg_stroke = stroke;
                                v.widgets.hovered.fg_stroke.color = txt_col;
                                ui.add_sized(
                                    egui::vec2(32.0, 32.0),
                                    egui::Button::new(icon.rich_text().size(14.0).color(txt_col))
                                        .fill(fill)
                                        .stroke(stroke),
                                )
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
