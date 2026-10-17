//! Left navigation rail (independent component).
use crate::iconify::{IconCache, iconify_icon};
use crate::theme::{C_ACCENT, C_RAIL, C_RAISED2};
use eframe::egui;
use std::sync::{Arc, Mutex};

/// Renders the left nav rail into `ui` when `open` is true. Updates the active tab
/// and panel visibility flags in place.
pub fn show(
    ui: &mut egui::Ui,
    ctx: &egui::Context,
    icon_cache: &Arc<Mutex<IconCache>>,
    open: bool,
    active_tab: &mut &'static str,
    chats_open: &mut bool,
    show_telegram: &mut bool,
) {
    if !open {
        return;
    }
    egui::Panel::left("navrail")
        .resizable(false)
        .default_size(56.0)
        .frame(egui::Frame::new().fill(C_RAIL))
        .show(ui, |ui| {
            ui.set_width(56.0);
            ui.vertical_centered(|ui| {
                let items = [
                    ("telegram", "cib", "telegram-plane"),
                    ("subtitles", "material-symbols-light", "subtitles"),
                    ("recent", "material-symbols-light", "history"),
                ];
                for (id, pfx, name) in items {
                    let active = *active_tab == id;
                    let (rect, response) = ui.allocate_exact_size(egui::vec2(44.0, 44.0), egui::Sense::click());
                    let hovered = response.hovered() || ui.rect_contains_pointer(rect);
                    let fill = if active {
                        C_RAISED2
                    } else if hovered {
                        C_ACCENT.linear_multiply(0.15)
                    } else {
                        egui::Color32::TRANSPARENT
                    };
                    let stroke = if active || hovered {
                        egui::Stroke::new(1.0, C_ACCENT)
                    } else {
                        egui::Stroke::new(1.0, egui::Color32::TRANSPARENT)
                    };
                    ui.painter().rect_filled(rect, 8.0, fill);
                    ui.painter().rect_stroke(rect, 8.0, stroke, egui::StrokeKind::Inside);
                    let mut clicked = false;
                    let mut child = ui.new_child(
                        egui::UiBuilder::new()
                            .max_rect(rect)
                            .layout(egui::Layout::centered_and_justified(egui::Direction::TopDown)),
                    );
                    if iconify_icon(&mut child, icon_cache, ctx, pfx, name, "#ffffff", 20, None) {
                        clicked = true;
                    }
                    if clicked || response.clicked() {
                        if *active_tab == id && *chats_open {
                            *chats_open = false;
                        } else {
                            *active_tab = id;
                            *chats_open = true;
                        }
                        if id == "telegram" {
                            *show_telegram = true;
                        }
                    }
                    ui.add_space(4.0);
                }
            });
        });
}
