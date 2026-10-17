        // Gradient scrim over the bottom 25% of the picture (fades with the HUD).
        if show_hud || hud_alpha > 0.0 {
            let scrim_h = video_rect.height() * 0.25;
            let scrim_rect = egui::Rect::from_min_size(
                egui::pos2(video_rect.min.x, video_rect.max.y - scrim_h),
                egui::vec2(video_rect.width(), scrim_h),
            );
            let mut mesh = egui::Mesh::default();
            let top_color = egui::Color32::TRANSPARENT;
            let bot_color = egui::Color32::from_black_alpha((0xd0 as f32 * hud_alpha) as u8);
            mesh.colored_vertex(scrim_rect.left_top(), top_color);
            mesh.colored_vertex(scrim_rect.right_top(), top_color);
            mesh.colored_vertex(scrim_rect.right_bottom(), bot_color);
            mesh.colored_vertex(scrim_rect.left_bottom(), bot_color);
            mesh.add_triangle(0, 1, 2);
            mesh.add_triangle(0, 2, 3);
            ctx.layer_painter(egui::LayerId::new(
                egui::Order::Foreground,
                egui::Id::new("hud_scrim"),
            ))
            .add(egui::Shape::mesh(mesh));
        }

        if show_hud {
            let hud_h = 70.0;
            let hud_bottom_padding = 24.0;
            // Keep the HUD clear of the (possibly open) left nav rail: indent it
            // by a small margin and shrink its width so it stays inside the video
            // rect instead of overlapping the sidebar.
            let window_rect = ctx.viewport_rect();
            let (hud_left_margin, hud_w) = if is_fullscreen {
                let max_w = 720.0;
                let w = window_rect.width().min(max_w);
                ((window_rect.width() - w) * 0.5, w)
            } else {
                let m = if self.nav_rail_open { 72.0 } else { 8.0 };
                (m, (window_rect.width() - m).max(1.0))
            };
            egui::Area::new("hud".into())
                .order(egui::Order::Tooltip)
                .fixed_pos(egui::pos2(
                    hud_left_margin,
                    window_rect.max.y - hud_h - hud_bottom_padding,
                ))
                .default_size(egui::vec2(hud_w, hud_h))
                .show(&ctx, |ui| {
                    ui.set_opacity(hud_alpha);
                    let vw = hud_w;
                    ui.set_min_size(egui::vec2(vw, hud_h));
                    ui.set_max_size(egui::vec2(vw, hud_h));
                    let local = egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(vw, hud_h));
                    let _ = local;
                    ui.with_layout(egui::Layout::bottom_up(egui::Align::Min), |ui| {
                        // controls row
                        ui.horizontal(|ui| {
                            ui.add_space(8.0);
                            let playing = !is_paused;
                            let play_icon_bytes: &[u8] = if playing {
                                include_bytes!("../assets/icons/newplayer/pause.svg")
                            } else {
                                include_bytes!("../assets/icons/newplayer/play-arrow.svg")
                            };
                            if ui
                                .add(
                                    egui::Button::image(
                                        egui::Image::from_bytes("play.svg", play_icon_bytes)
                                            .fit_to_exact_size(egui::vec2(18.0, 18.0)),
                                    )
                                    .fill(C_RAISED)
                                    .min_size(egui::vec2(36.0, 36.0)),
                                )
                                .clicked()
                            {
                                let _ = self.player_mut().toggle_pause();
                            }
                            if ui
                                .add(
                                    egui::Button::image(
                                        egui::Image::from_bytes(
                                            "replay-10.svg",
                                            include_bytes!(
                                                "../assets/icons/newplayer/replay-10.svg"
                                            ),
                                        )
                                        .fit_to_exact_size(egui::vec2(18.0, 18.0)),
                                    )
                                    .fill(C_RAISED)
                                    .min_size(egui::vec2(36.0, 36.0)),
                                )
                                .clicked()
                            {
                                let _ = self.player_mut().seek_relative(-10.0);
                            }
                            if ui
                                .add(
                                    egui::Button::image(
                                        egui::Image::from_bytes(
                                            "forward-10.svg",
                                            include_bytes!(
                                                "../assets/icons/newplayer/forward-10.svg"
                                            ),
                                        )
                                        .fit_to_exact_size(egui::vec2(18.0, 18.0)),
                                    )
                                    .fill(C_RAISED)
                                    .min_size(egui::vec2(36.0, 36.0)),
                                )
                                .clicked()
                            {
                                let _ = self.player_mut().seek_relative(10.0);
                            }
                            // volume: mute icon + slider that expands on hover
                            let muted = self.player_mut().volume().unwrap_or(50.0) <= 0.0;
                            let mute_icon: &[u8] = if muted {
                                include_bytes!("../assets/icons/newplayer/volume-off.svg")
                            } else {
                                include_bytes!("../assets/icons/newplayer/volume-up.svg")
                            };
                            if ui
                                .add(
                                    egui::Button::image(
                                        egui::Image::from_bytes("volume.svg", mute_icon)
                                            .fit_to_exact_size(egui::vec2(18.0, 18.0)),
                                    )
                                    .fill(C_RAISED)
                                    .min_size(egui::vec2(36.0, 36.0)),
                                )
                                .clicked()
                            {
                                let _ = self.player_mut().toggle_mute();
                            }
                            let (slider_rect, _) = ui
                                .allocate_exact_size(egui::vec2(110.0, 24.0), egui::Sense::hover());
                            let slider_hovered = slider_rect
                                .contains(ctx.pointer_hover_pos().unwrap_or(egui::Pos2::ZERO))
                                || ui.rect_contains_pointer(slider_rect);
                            if slider_hovered {
                                let mut vol = self.player_mut().volume().unwrap_or(50.0) as f32;
                                if ui
                                    .put(
                                        slider_rect,
                                        egui::Slider::new(&mut vol, 0.0..=100.0).show_value(false),
                                    )
                                    .changed()
                                {
                                    let _ = self.player_mut().set_volume(vol as f64);
                                }
                            }
                            let pos = self.player_mut().time_pos().ok().flatten().unwrap_or(0.0);
                            let dur = self.player_mut().duration().ok().flatten().unwrap_or(0.0);
                            let pos_s = pos.floor() as i64;
                            let dur_s = dur.floor() as i64;
                            ui.label(
                                egui::RichText::new(format!(
                                    "{:02}:{:02} / {:02}:{:02}",
                                    pos_s / 60,
                                    pos_s % 60,
                                    dur_s / 60,
                                    dur_s % 60
                                ))
                                .monospace()
                                .size(11.0)
                                .color(C_MUTED),
                            );
                            ui.with_layout(
                                egui::Layout::right_to_left(egui::Align::Center),
                                |ui| {
                                    let is_fullscreen =
                                        ctx.input(|i| i.viewport().fullscreen).unwrap_or(false);
                                    let fs_icon_bytes = if is_fullscreen {
                                        include_bytes!(
                                            "../assets/icons/newplayer/fullscreen-exit.svg"
                                        )
                                    } else {
                                        include_bytes!("../assets/icons/newplayer/fullscreen.svg")
                                    };
                                    if ui
                                        .add(
                                            egui::Button::image(
                                                egui::Image::from_bytes(
                                                    "fullscreen.svg",
                                                    fs_icon_bytes,
                                                )
                                                .fit_to_exact_size(egui::vec2(18.0, 18.0)),
                                            )
                                            .fill(C_RAISED)
                                            .min_size(egui::vec2(36.0, 36.0)),
                                        )
                                        .clicked()
                                    {
                                        ctx.send_viewport_cmd(egui::ViewportCommand::Fullscreen(
                                            !is_fullscreen,
                                        ));
                                    }
                                    // Speed menu
                                    let speed_label = format!("{:.2}x", self.playback_speed)
                                        .trim_end_matches('0')
                                        .trim_end_matches('.')
                                        .to_string()
                                        + "x";
                                    let speed_resp = ui.add(
                                        egui::Button::new(&speed_label)
                                            .fill(C_RAISED)
                                            .min_size(egui::vec2(0.0, 36.0)),
                                    );
                                    let speed_btn_rect = speed_resp.rect;
                                    if speed_resp.clicked() {
                                        self.speed_menu_open = !self.speed_menu_open;
                                        self.audio_menu_open = false;
                                        self.subs_menu_open = false;
                                        if self.speed_menu_open {
                                            self.menu_opened_frame = self.frame_count;
                                        }
                                    }
                                    if self.speed_menu_open {
                                        egui::Area::new("speed_menu".into())
                                            .order(egui::Order::Tooltip)
                                            .pivot(egui::Align2::LEFT_BOTTOM)
                                            .fixed_pos(egui::pos2(
                                                speed_btn_rect.left(),
                                                speed_btn_rect.top() - 4.0,
                                            ))
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
                                    // Subtitle track selector
                                    let sub_label = self
                                        .subtitle_tracks
                                        .iter()
                                        .find(|t| t.selected)
                                        .map(|t| t.label.clone())
                                        .unwrap_or_else(|| "Subs".to_string());
                                    let subs_resp = ui.add(
                                        egui::Button::image_and_text(
                                            egui::Image::from_bytes(
                                                "subtitles.svg",
                                                include_bytes!(
                                                    "../assets/icons/newplayer/subtitles.svg"
                                                ),
                                            )
                                            .fit_to_exact_size(egui::vec2(18.0, 18.0)),
                                            &sub_label,
                                        )
                                        .fill(C_RAISED)
                                        .min_size(egui::vec2(0.0, 36.0)),
                                    );
                                    let subs_btn_rect = subs_resp.rect;
                                    if subs_resp.clicked() {
                                        self.subs_menu_open = !self.subs_menu_open;
                                        self.audio_menu_open = false;
                                        if self.subs_menu_open {
                                            self.menu_opened_frame = self.frame_count;
                                        }
                                    }
                                    if self.subs_menu_open {
                                        egui::Area::new("subs_menu".into())
                                            .order(egui::Order::Tooltip)
                                            .pivot(egui::Align2::LEFT_BOTTOM)
                                            .fixed_pos(egui::pos2(
                                                subs_btn_rect.left(),
                                                subs_btn_rect.top() - 4.0,
                                            ))
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
                                                                for t in
                                                                    self.subtitle_tracks.clone()
                                                                {
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
                                                                        if let Ok(()) = self
                                                                            .player_mut()
                                                                            .mpv()
                                                                            .set_property("sid", t.id)
                                                                        {
                                                                            for tr in &mut self
                                                                                .subtitle_tracks
                                                                            {
                                                                                tr.selected =
                                                                                    tr.id == t.id;
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
                                    // Audio track selector
                                    let audio_label = self
                                        .audio_tracks
                                        .iter()
                                        .find(|t| t.selected)
                                        .map(|t| t.label.clone())
                                        .unwrap_or_else(|| "Audio".to_string());
                                    let audio_resp = ui.add(
                                        egui::Button::image_and_text(
                                            egui::Image::from_bytes(
                                                "audiotrack.svg",
                                                include_bytes!(
                                                    "../assets/icons/newplayer/audiotrack.svg"
                                                ),
                                            )
                                            .fit_to_exact_size(egui::vec2(18.0, 18.0)),
                                            &audio_label,
                                        )
                                        .fill(C_RAISED)
                                        .min_size(egui::vec2(0.0, 36.0)),
                                    );
                                    let audio_btn_rect = audio_resp.rect;
                                    if audio_resp.clicked() {
                                        self.audio_menu_open = !self.audio_menu_open;
                                        self.subs_menu_open = false;
                                        if self.audio_menu_open {
                                            self.menu_opened_frame = self.frame_count;
                                        }
                                    }
                                    if self.audio_menu_open {
                                        egui::Area::new("audio_menu".into())
                                            .order(egui::Order::Tooltip)
                                            .pivot(egui::Align2::LEFT_BOTTOM)
                                            .fixed_pos(egui::pos2(
                                                audio_btn_rect.left(),
                                                audio_btn_rect.top() - 4.0,
                                            ))
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
                                                                if let Ok(()) = self
                                                                    .player_mut()
                                                                    .mpv()
                                                                    .set_property("aid", t.id)
                                                                {
                                                                    for tr in &mut self.audio_tracks
                                                                    {
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
                                    ui.add_space(8.0);
                                },
                            );
                            ui.add_space(8.0);
