use crate::app::AmminiApp;
use crate::theme::*;
use ammini::telegram::BgCommand;
use eframe::egui;

impl AmminiApp {
    pub fn render_settings_menu(&mut self, ctx: &egui::Context) {
        if !self.settings_menu_open {
            return;
        }
        let window_rect = ctx.viewport_rect();
        let menu_resp = egui::Area::new("settings_menu".into())
            .order(egui::Order::Tooltip)
            .fixed_pos(egui::pos2(8.0, window_rect.max.y - 160.0))
            .show(ctx, |ui| {
                egui::Frame::new()
                    .fill(C_RAISED)
                    .stroke(egui::Stroke::new(1.0, C_BORDER2))
                    .corner_radius(10.0)
                    .inner_margin(egui::Margin::same(8))
                    .show(ui, |ui| {
                        ui.set_min_width(220.0);
                        ui.set_max_width(220.0);

                        // Sign out
                        let (rect, resp) = ui.allocate_exact_size(
                            egui::vec2(ui.available_width(), 40.0),
                            egui::Sense::click(),
                        );
                        let fill = if resp.hovered() {
                            C_ACCENT.linear_multiply(0.15)
                        } else {
                            egui::Color32::TRANSPARENT
                        };
                        ui.painter().rect_filled(rect, 6.0, fill);
                        let icon_rect = egui::Rect::from_center_size(
                            rect.left_center() + egui::vec2(20.0, 0.0),
                            egui::vec2(18.0, 18.0),
                        );
                        ui.put(
                            icon_rect,
                            egui::Image::from_bytes(
                                "kl-log-out-fill.svg",
                                ammini::icons::ICON_KL_LOG_OUT_FILL.1,
                            )
                            .fit_to_exact_size(egui::vec2(18.0, 18.0)),
                        );
                        ui.painter().text(
                            rect.left_center() + egui::vec2(44.0, 0.0),
                            egui::Align2::LEFT_CENTER,
                            "Sign out",
                            egui::FontId::proportional(14.0),
                            C_TEXT,
                        );
                        if resp.hovered() {
                            ctx.set_cursor_icon(egui::CursorIcon::PointingHand);
                        }
                        if resp.clicked() {
                            if let Some(bg) = &self.bg_tx {
                                let _ = bg.send(BgCommand::SignOut);
                            }
                            self.settings_menu_open = false;
                        }
                        ui.add_space(4.0);

                        // About
                        let (rect, resp) = ui.allocate_exact_size(
                            egui::vec2(ui.available_width(), 40.0),
                            egui::Sense::click(),
                        );
                        let fill = if resp.hovered() {
                            C_ACCENT.linear_multiply(0.15)
                        } else {
                            egui::Color32::TRANSPARENT
                        };
                        ui.painter().rect_filled(rect, 6.0, fill);
                        let icon_rect = egui::Rect::from_center_size(
                            rect.left_center() + egui::vec2(20.0, 0.0),
                            egui::vec2(18.0, 18.0),
                        );
                        ui.put(
                            icon_rect,
                            egui::Image::from_bytes(
                                "kl-info-fill.svg",
                                ammini::icons::ICON_KL_INFO_FILL.1,
                            )
                            .fit_to_exact_size(egui::vec2(18.0, 18.0)),
                        );
                        ui.painter().text(
                            rect.left_center() + egui::vec2(44.0, 0.0),
                            egui::Align2::LEFT_CENTER,
                            "About",
                            egui::FontId::proportional(14.0),
                            C_TEXT,
                        );
                        if resp.hovered() {
                            ctx.set_cursor_icon(egui::CursorIcon::PointingHand);
                        }
                        // About does nothing for now.
                        if resp.clicked() {
                            self.settings_menu_open = false;
                        }
                    });
            });
        let clicked_outside = ctx.input(|i| i.pointer.any_click())
            && ctx.input(|i| {
                let pos = i.pointer.hover_pos().unwrap_or_default();
                let in_menu = menu_resp.response.rect.contains(pos);
                let in_icon = pos.x < 60.0 && pos.y > window_rect.max.y - 60.0;
                !in_menu && !in_icon
            });
        if clicked_outside {
            self.settings_menu_open = false;
        }
    }
}
