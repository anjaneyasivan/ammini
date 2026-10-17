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

