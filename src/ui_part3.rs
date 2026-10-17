                        });
                        // timeline
                        let tl_w = (vw - 16.0).max(1.0);
                        let (tl_rect, tl_resp) = ui
                            .horizontal(|ui| {
                                ui.add_space(8.0);
                                ui.allocate_exact_size(
                                    egui::vec2(tl_w, 18.0),
                                    egui::Sense::click_and_drag(),
                                )
                            })
                            .inner;
                        let seek_hovered = tl_resp.hovered();
                        let rail_h = if seek_hovered { 8.0 } else { 6.0 };
                        let tl_visual = egui::Rect::from_center_size(
                            tl_rect.center(),
                            egui::vec2(tl_rect.width(), rail_h),
                        );
                        let painter = ui.painter();
                        painter.rect_filled(tl_visual, 3.0, C_RAISED);
                        // buffered
                        let dur = self
                            .player_mut()
                            .duration()
                            .ok()
                            .flatten()
                            .unwrap_or(1.0)
                            .max(1.0);
                        // buffered
                        let cache_col = C_TEXT.gamma_multiply(0.28);
                        let rail_r = tl_visual.height() * 0.5;
                        for (s, e) in &cache_spans {
                            let x0 = tl_visual.min.x
                                + ((s / dur).clamp(0.0, 1.0) as f32) * tl_visual.width();
                            let x1 = tl_visual.min.x
                                + ((e / dur).clamp(0.0, 1.0) as f32) * tl_visual.width();
                            if x1 - x0 < 0.5 {
                                continue;
                            }
                            let r = egui::Rect::from_min_max(
                                egui::pos2(x0, tl_visual.min.y),
                                egui::pos2(x1, tl_visual.max.y),
                            );
                            let cr = egui::CornerRadius {
                                nw: if x0 <= tl_visual.min.x + 0.5 {
                                    rail_r as u8
                                } else {
                                    0
                                },
                                sw: if x0 <= tl_visual.min.x + 0.5 {
                                    rail_r as u8
                                } else {
                                    0
                                },
                                ne: if x1 >= tl_visual.max.x - 0.5 {
                                    rail_r as u8
                                } else {
                                    0
                                },
                                se: if x1 >= tl_visual.max.x - 0.5 {
                                    rail_r as u8
                                } else {
                                    0
                                },
                            };
                            painter.rect_filled(r, cr, cache_col);
                        }
                        // played
                        let pos = self.player_mut().time_pos().ok().flatten().unwrap_or(0.0);
                        let played_w = (pos / dur) as f32 * tl_visual.width();
                        painter.rect_filled(
                            egui::Rect::from_min_size(
                                tl_visual.min,
                                egui::vec2(played_w, tl_visual.height()),
                            ),
                            3.0,
                            C_TEXT,
                        );
                        if tl_resp.hovered() {
                            ctx.set_cursor_icon(egui::CursorIcon::PointingHand);
                            if let Some(p) = ctx.pointer_hover_pos() {
                                let frac =
                                    ((p.x - tl_rect.min.x) / tl_rect.width()).clamp(0.0, 1.0);
                                let t = frac as f64 * dur;
                                let s = t.floor() as i64;
                                let label = format!("{:02}:{:02}", s / 60, s % 60);
                                let font = egui::FontId::monospace(11.0);
                                ui.painter().text(
                                    egui::pos2(p.x, tl_rect.min.y - 4.0),
                                    egui::Align2::CENTER_BOTTOM,
                                    label,
                                    font,
                                    C_TEXT,
                                );
                            }
                        }
                        if tl_resp.clicked() || tl_resp.dragged() {
                            if let Some(p) = tl_resp.interact_pointer_pos() {
                                let frac =
                                    ((p.x - tl_rect.min.x) / tl_rect.width()).clamp(0.0, 1.0);
                                let _ = self.player_mut().seek_to(frac as f64 * dur);
                            }
                        }
                        ui.add_space(8.0);
                    });
                });
        }

