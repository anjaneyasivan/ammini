use crate::app::AmminiApp;
use crate::theme::*;
use eframe::egui;

impl AmminiApp {
    pub fn paint_scrim(
        &self,
        ctx: &egui::Context,
        video_rect: egui::Rect,
        hud_alpha: f32,
        show_hud: bool,
    ) {
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
    }

    pub fn render_hud(
        &mut self,
        _ui: &mut egui::Ui,
        ctx: &egui::Context,
        _video_rect: egui::Rect,
        cache_spans: &[(f64, f64)],
        hud_alpha: f32,
        show_hud: bool,
        is_paused: bool,
        is_fullscreen: bool,
    ) {
        if !show_hud {
            return;
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
                                    self.render_speed_menu(ctx, speed_btn_rect); // Subtitle track selector
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
                                    self.render_subs_menu(ctx, subs_btn_rect); // Audio track selector
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
                                    self.render_audio_menu(ctx, audio_btn_rect);
                                    ui.add_space(8.0);
                                },
                            );
                            ui.add_space(8.0);
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
                        for (s, e) in cache_spans {
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
    }
}
