//! Left navigation rail (independent component).
use crate::iconify::{IconCache, iconify_icon};
use crate::theme::{C_RAIL, C_RAISED2};
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
                    let frame = egui::Frame::new()
                        .fill(if active {
                            C_RAISED2
                        } else {
                            egui::Color32::TRANSPARENT
                        })
                        .stroke(if active {
                            egui::Stroke::new(1.0, egui::Color32::from_rgb(0x33, 0x33, 0x42))
                        } else {
                            egui::Stroke::NONE
                        })
                        .inner_margin(egui::Margin::same(8));
                    frame.show(ui, |ui| {
                        if iconify_icon(ui, icon_cache, ctx, pfx, name, "#ffffff", 20, None) {
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
                    });
                    ui.add_space(4.0);
                }
            });
        });
}
