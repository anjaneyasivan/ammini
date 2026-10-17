use crate::app::AmminiApp;
use crate::right_sidebar;
use crate::theme::*;
use ammini::telegram::BgCommand;
use eframe::egui;

impl AmminiApp {
    pub fn render_chats_overlay(
        &mut self,
        ui: &mut egui::Ui,
        ctx: &egui::Context,
        video_rect: egui::Rect,
    ) {
        if !self.chats_open {
            return;
        }
        // Floating chats overlay (Telegram)
        if self.chats_open {
            let x = if self.nav_rail_open { 58.0 } else { 8.0 };
            let chats_resp = egui::Area::new("chats_overlay".into())
                .fixed_pos(egui::pos2(x, 0.0))
                .show(&ctx, |ui| {
                    egui::Frame::new()
                        .fill(C_SIDEBAR)
                        .stroke(egui::Stroke::new(1.0, C_BORDER2))
                        .corner_radius(0.0)
                        .inner_margin(egui::Margin {
                            left: 12,
                            right: 12,
                            top: 8,
                            bottom: 12,
                        })
                        .show(ui, |ui| {
                            ui.set_width(300.0);
                            ui.set_min_height(ctx.viewport_rect().height() - 8.0);
                            ui.set_max_height(ctx.viewport_rect().height() - 8.0);
                            ui.add_space(4.0);
                            match self.active_tab {
                                "telegram" => {
                                    self.telegram_panel
                                        .ui(ui, &mut self.telegram_fsm, &self.bg_tx);
                                }
                                "recent" => {
                                    ui.label(
                                        egui::RichText::new("RECENTLY PLAYED")
                                            .monospace()
                                            .size(11.0)
                                            .strong()
                                            .color(C_DIM),
                                    );
                                    ui.add_space(4.0);
                                    let recent_tg = self.fsm.persistent.recent_telegram.clone();
                                    if recent_tg.is_empty() {
                                        ui.label(
                                            egui::RichText::new("No recent Telegram files")
                                                .color(C_MUTED),
                                        );
                                    } else {
                                        egui::ScrollArea::vertical().show(ui, |ui| {
                                            for entry in &recent_tg {
                                                let (rect, resp) = ui.allocate_exact_size(
                                                    egui::vec2(ui.available_width(), 52.0),
                                                    egui::Sense::click(),
                                                );
                                                ui.painter().rect_filled(
                                                    rect,
                                                    4.0,
                                                    if resp.hovered() {
                                                        C_RAISED
                                                    } else {
                                                        egui::Color32::TRANSPARENT
                                                    },
                                                );
                                                ui.painter().text(
                                                    rect.left_top() + egui::vec2(8.0, 6.0),
                                                    egui::Align2::LEFT_TOP,
                                                    &entry.chat_name,
                                                    egui::FontId::proportional(12.0),
                                                    C_TEXT,
                                                );
                                                let name = if entry.name.chars().count() > 46 {
                                                    format!(
                                                        "{}…",
                                                        entry
                                                            .name
                                                            .chars()
                                                            .take(46)
                                                            .collect::<String>()
                                                    )
                                                } else {
                                                    entry.name.clone()
                                                };
                                                ui.painter().text(
                                                    rect.left_top() + egui::vec2(8.0, 24.0),
                                                    egui::Align2::LEFT_TOP,
                                                    &name,
                                                    egui::FontId::monospace(10.0),
                                                    C_MUTED,
                                                );
                                                if resp.clicked() {
                                                    if let Some(bg) = &self.bg_tx {
                                                        let _ =
                                                            bg.send(BgCommand::PlayTelegramVideo {
                                                                peer: entry.peer,
                                                                msg_id: entry.msg_id,
                                                            });
                                                    }
                                                }
                                            }
                                        });
                                    }
                                }
                                "library" => {
                                    ui.label(egui::RichText::new("Media library").color(C_MUTED));
                                }
                                "audio" => {
                                    ui.label(egui::RichText::new("Audio tracks").color(C_MUTED));
                                    for t in &self.audio_tracks {
                                        ui.label(egui::RichText::new(&t.label).color(C_TEXT));
                                    }
                                }
                                "subtitles" => {
                                    ui.label(egui::RichText::new("Subtitle tracks").color(C_MUTED));
                                    ui.add_space(6.0);
                                    for t in self.subtitle_tracks.clone() {
                                        let label = if t.selected {
                                            format!("✓ {}", t.label)
                                        } else {
                                            t.label.clone()
                                        };
                                        let (rect, resp) = ui.allocate_exact_size(
                                            egui::vec2(ui.available_width(), 32.0),
                                            egui::Sense::click(),
                                        );
                                        let hovered = resp.hovered();
                                        let fill = if t.selected {
                                            C_RAISED2
                                        } else if hovered {
                                            C_ACCENT.linear_multiply(0.15)
                                        } else {
                                            egui::Color32::TRANSPARENT
                                        };
                                        ui.painter().rect_filled(rect, 6.0, fill);
                                        ui.painter().text(
                                            rect.left_center() + egui::vec2(10.0, 0.0),
                                            egui::Align2::LEFT_CENTER,
                                            &label,
                                            egui::FontId::proportional(13.0),
                                            if t.selected { C_TEXT } else { C_MUTED },
                                        );
                                        if resp.hovered() {
                                            ctx.set_cursor_icon(egui::CursorIcon::PointingHand);
                                        }
                                        if resp.clicked() {
                                            if let Ok(()) =
                                                self.player_mut().mpv().set_property("sid", t.id)
                                            {
                                                for tr in &mut self.subtitle_tracks {
                                                    tr.selected = tr.id == t.id;
                                                }
                                            }
                                        }
                                        ui.add_space(4.0);
                                    }
                                }
                                "settings" => {
                                    ui.label(egui::RichText::new("Settings").color(C_MUTED));
                                }
                                _ => {
                                    ui.label(egui::RichText::new("Select an item").color(C_MUTED));
                                }
                            }
                        });
                });
            // Cover the left border so the panel reads as an extension of the nav rail.
            let frame_rect = chats_resp.response.rect;
            ui.painter().rect_filled(
                egui::Rect::from_min_max(
                    frame_rect.min,
                    egui::pos2(frame_rect.min.x + 2.0, frame_rect.max.y),
                ),
                0.0,
                C_SIDEBAR,
            );
            // dismiss on click outside
            let clicked_outside = ctx.input(|i| i.pointer.any_click())
                && ctx.input(|i| {
                    let pos = i.pointer.hover_pos();
                    let in_overlay = pos
                        .map(|p| chats_resp.response.rect.contains(p))
                        .unwrap_or(false);
                    let in_navrail = pos.map(|p| p.x < 60.0 && p.y >= 40.0).unwrap_or(false);
                    let in_toolbar = pos
                        .map(|p| p.y >= video_rect.min.y && p.y < video_rect.min.y + 40.0)
                        .unwrap_or(false);
                    !in_overlay && !in_navrail && !in_toolbar
                });
            if clicked_outside {
                self.chats_open = false;
            }
        }
    }

    pub fn render_inspector_overlay(
        &mut self,
        _ui: &mut egui::Ui,
        ctx: &egui::Context,
        video_rect: egui::Rect,
    ) {
        if !self.inspector_open {
            return;
        }
        // Floating inspector overlay
        if self.inspector_open {
            let proxy_url = self.proxy_url_mut().clone();
            let pos_line = self
                .player_mut()
                .time_pos()
                .ok()
                .flatten()
                .map(|pos| format!("pos: {:.1}s", pos));
            let info_lines = {
                let mpv = self.player_mut().mpv();
                let mut lines: Vec<String> = Vec::new();
                if let Ok(w) = mpv.get_property::<i64>("width")
                    && let Ok(h) = mpv.get_property::<i64>("height")
                {
                    lines.push(format!("resolution: {}x{}", w, h));
                }
                if let Ok(c) = mpv.get_property::<String>("video-codec")
                    && !c.is_empty()
                {
                    lines.push(format!("video codec: {}", c));
                }
                if let Ok(c) = mpv.get_property::<String>("audio-codec")
                    && !c.is_empty()
                {
                    lines.push(format!("audio codec: {}", c));
                }
                if let Ok(fs) = mpv.get_property::<i64>("file-size")
                    && fs > 0
                {
                    lines.push(format!("file size: {:.1} MB", fs as f64 / 1_048_576.0));
                }
                if let Ok(vb) = mpv.get_property::<f64>("video-bitrate")
                    && vb > 0.0
                {
                    lines.push(format!("video bitrate: {:.0} kbps", vb / 1000.0));
                }
                if let Ok(ab) = mpv.get_property::<f64>("audio-bitrate")
                    && ab > 0.0
                {
                    lines.push(format!("audio bitrate: {:.0} kbps", ab / 1000.0));
                }
                if let Ok(fps) = mpv.get_property::<f64>("container-fps")
                    && fps > 0.0
                {
                    lines.push(format!("fps: {:.2}", fps));
                }
                lines
            };
            let dismiss = right_sidebar::show(
                &ctx,
                proxy_url.as_deref(),
                pos_line,
                &info_lines,
                video_rect.min.y + 40.0,
                60.0,
            );
            if dismiss {
                self.inspector_open = false;
            }
        }
    }

    pub fn render_speed_menu(&mut self, ctx: &egui::Context, btn_rect: egui::Rect) {
        if self.speed_menu_open {
            egui::Area::new("speed_menu".into())
                .order(egui::Order::Tooltip)
                .pivot(egui::Align2::LEFT_BOTTOM)
                .fixed_pos(egui::pos2(btn_rect.left(), btn_rect.top() - 4.0))
                .show(&ctx, |ui| {
                    egui::Frame::new()
                        .fill(C_RAISED)
                        .stroke(egui::Stroke::new(1.0, C_BORDER2))
                        .corner_radius(6.0)
                        .inner_margin(egui::Margin::same(6))
                        .show(ui, |ui| {
                            ui.set_min_width(120.0);
                            ui.set_max_width(160.0);
                            for speed in [0.5f64, 0.75, 1.0, 1.25, 1.5, 2.0] {
                                let selected = (speed - self.playback_speed).abs() < f64::EPSILON;
                                let label = if selected {
                                    format!("✓ {speed}x")
                                } else {
                                    format!("{speed}x")
                                };
                                let (rect, resp) = ui.allocate_exact_size(
                                    egui::vec2(ui.available_width(), 28.0),
                                    egui::Sense::click(),
                                );
                                let fill = if selected {
                                    C_RAISED2
                                } else if resp.hovered() {
                                    C_ACCENT.linear_multiply(0.15)
                                } else {
                                    egui::Color32::TRANSPARENT
                                };
                                ui.painter().rect_filled(rect, 4.0, fill);
                                ui.painter().text(
                                    rect.left_center() + egui::vec2(8.0, 0.0),
                                    egui::Align2::LEFT_CENTER,
                                    &label,
                                    egui::FontId::proportional(12.0),
                                    if selected { C_TEXT } else { C_MUTED },
                                );
                                if resp.hovered() {
                                    ctx.set_cursor_icon(egui::CursorIcon::PointingHand);
                                }
                                if resp.clicked() {
                                    self.playback_speed = speed;
                                    let _ = self.player_mut().mpv().set_property("speed", speed);
                                    self.speed_menu_open = false;
                                }
                                ui.add_space(2.0);
                            }
                        });
                });
        }
    }

    pub fn render_subs_menu(&mut self, ctx: &egui::Context, btn_rect: egui::Rect) {
        if self.subs_menu_open {
            egui::Area::new("subs_menu".into())
                .order(egui::Order::Tooltip)
                .pivot(egui::Align2::LEFT_BOTTOM)
                .fixed_pos(egui::pos2(btn_rect.left(), btn_rect.top() - 4.0))
                .show(&ctx, |ui| {
                    egui::Frame::new()
                        .fill(C_RAISED)
                        .stroke(egui::Stroke::new(1.0, C_BORDER2))
                        .corner_radius(6.0)
                        .inner_margin(egui::Margin::same(6))
                        .show(ui, |ui| {
                            ui.set_min_width(120.0);
                            ui.set_max_width(160.0);
                            ui.set_max_height(240.0);
                            egui::ScrollArea::vertical()
                                .max_height(200.0)
                                .show(ui, |ui| {
                                    for t in self.subtitle_tracks.clone() {
                                        let label = if t.selected {
                                            format!("✓ {}", t.label)
                                        } else {
                                            t.label.clone()
                                        };
                                        let (rect, resp) = ui.allocate_exact_size(
                                            egui::vec2(ui.available_width(), 28.0),
                                            egui::Sense::click(),
                                        );
                                        let fill = if t.selected {
                                            C_RAISED2
                                        } else if resp.hovered() {
                                            C_ACCENT.linear_multiply(0.15)
                                        } else {
                                            egui::Color32::TRANSPARENT
                                        };
                                        ui.painter().rect_filled(rect, 4.0, fill);
                                        ui.painter().text(
                                            rect.left_center() + egui::vec2(8.0, 0.0),
                                            egui::Align2::LEFT_CENTER,
                                            &label,
                                            egui::FontId::proportional(12.0),
                                            if t.selected { C_TEXT } else { C_MUTED },
                                        );
                                        if resp.hovered() {
                                            ctx.set_cursor_icon(egui::CursorIcon::PointingHand);
                                        }
                                        if resp.clicked() {
                                            if let Ok(()) =
                                                self.player_mut().mpv().set_property("sid", t.id)
                                            {
                                                for tr in &mut self.subtitle_tracks {
                                                    tr.selected = tr.id == t.id;
                                                }
                                            }
                                            self.subs_menu_open = false;
                                        }
                                        ui.add_space(2.0);
                                    }
                                });
                        });
                });
        }
    }

    pub fn render_audio_menu(&mut self, ctx: &egui::Context, btn_rect: egui::Rect) {
        if self.audio_menu_open {
            egui::Area::new("audio_menu".into())
                .order(egui::Order::Tooltip)
                .pivot(egui::Align2::LEFT_BOTTOM)
                .fixed_pos(egui::pos2(btn_rect.left(), btn_rect.top() - 4.0))
                .show(&ctx, |ui| {
                    egui::Frame::new()
                        .fill(C_RAISED)
                        .stroke(egui::Stroke::new(1.0, C_BORDER2))
                        .corner_radius(6.0)
                        .inner_margin(egui::Margin::same(6))
                        .show(ui, |ui| {
                            ui.set_min_width(120.0);
                            ui.set_max_width(160.0);
                            for t in self.audio_tracks.clone() {
                                let label = if t.selected {
                                    format!("✓ {}", t.label)
                                } else {
                                    t.label.clone()
                                };
                                let (rect, resp) = ui.allocate_exact_size(
                                    egui::vec2(ui.available_width(), 28.0),
                                    egui::Sense::click(),
                                );
                                let fill = if t.selected {
                                    C_RAISED2
                                } else if resp.hovered() {
                                    C_ACCENT.linear_multiply(0.15)
                                } else {
                                    egui::Color32::TRANSPARENT
                                };
                                ui.painter().rect_filled(rect, 4.0, fill);
                                ui.painter().text(
                                    rect.left_center() + egui::vec2(8.0, 0.0),
                                    egui::Align2::LEFT_CENTER,
                                    &label,
                                    egui::FontId::proportional(12.0),
                                    if t.selected { C_TEXT } else { C_MUTED },
                                );
                                if resp.hovered() {
                                    ctx.set_cursor_icon(egui::CursorIcon::PointingHand);
                                }
                                if resp.clicked() {
                                    if let Ok(()) =
                                        self.player_mut().mpv().set_property("aid", t.id)
                                    {
                                        for tr in &mut self.audio_tracks {
                                            tr.selected = tr.id == t.id;
                                        }
                                    }
                                    self.audio_menu_open = false;
                                }
                                ui.add_space(2.0);
                            }
                        });
                });
        }
    }
}
