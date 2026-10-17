use ammini::fsm::{PlayerEvent, RecentTelegram, record_recent_telegram};
use ammini::telegram::UiMessage;
use ammini::telegram::state_machine::TelegramEvent;
use ammini::telemetry::SeekTracker;
use eframe::{App, Frame, egui};
use egui_sharkplayer::SharkPlayer;

use std::rc::Rc;
use tracing::{debug, info, trace};

use crate::app::AmminiApp;
use crate::types::*;

impl App for AmminiApp {
    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut Frame) {
        self.frame_count += 1;
        let ctx = ui.ctx().clone();
        let mut events = Vec::new();

        while let Ok(msg) = self.ui_rx.try_recv() {
            match msg {
                UiMessage::ProxyReady { port } => {
                    *self.proxy_url_mut() = Some(format!("http://127.0.0.1:{port}"));
                }
                UiMessage::VideoReady { msg_id, url, name } => {
                    info!("telegram: video ready msg_id={msg_id} url={url}");
                    self.telegram_fsm.handle(&TelegramEvent::VideoReady);
                    events.push(PlayerEvent::OpenTelegramUrl(url));
                    // Hide the chats overlay so the video is visible.
                    self.chats_open = false;
                    if let Some(peer) = self.telegram_fsm.data.selected_chat {
                        let chat_name = self
                            .telegram_fsm
                            .data
                            .selected_chat_name
                            .clone()
                            .unwrap_or_else(|| "Chat".to_string());
                        let entry = RecentTelegram {
                            peer,
                            chat_name,
                            msg_id,
                            name,
                        };
                        let persistent = unsafe { &mut self.fsm.inner_mut().persistent };
                        record_recent_telegram(&mut persistent.recent_telegram, entry);
                    }
                }
                UiMessage::VideoError(e) => {
                    self.telegram_fsm.handle(&TelegramEvent::VideoError(e));
                }
                UiMessage::VideoRemoved { msg_id } => {
                    let persistent = unsafe { &mut self.fsm.inner_mut().persistent };
                    persistent.recent_telegram.retain(|e| e.msg_id != msg_id);
                }
                UiMessage::CacheCoverage {
                    msg_id,
                    total_bytes,
                    ranges,
                } => {
                    self.cache_coverage.insert(
                        msg_id,
                        CacheCoverage {
                            total_bytes,
                            ranges,
                        },
                    );
                }
                other => {
                    if let UiMessage::AuthError(r) = &other {
                        let _ = r;
                    }
                    if let UiMessage::Error(m) = &other {
                        let _ = m;
                    }
                    if let Some(ev) = ammini::telegram::state_machine::ui_message_to_event(&other) {
                        self.telegram_fsm.handle(&ev);
                    }
                }
            }
        }
        while let Ok(update) = self.update_rx.try_recv() {
            self.update = Some(update);
            self.show_update_dialog = true;
        }

        self.handle_dropped_files(&ctx, &mut events);
        self.handle_shortcuts(&ctx, &mut events);
        self.handle_url_dialog(ui, &mut events);
        if let Some(p) = self.pending_open_file.take() {
            events.push(PlayerEvent::OpenFile(p));
        }
        self.refresh_tracks();

        // Learn the byte->time map for cache shading (throttled ~1 Hz).
        let now = std::time::Instant::now();
        let should_record = self
            .byte_time_record_last
            .map(|t| t.elapsed().as_secs_f64() >= 1.0)
            .unwrap_or(true);
        if should_record {
            let t = self.player_mut().time_pos().ok().flatten();
            let b = self
                .player_mut()
                .mpv()
                .get_property::<i64>("stream-pos")
                .ok();
            if let (Some(t), Some(b)) = (t, b) {
                self.byte_time_map.record(b as u64, t);
            }
            self.byte_time_record_last = Some(now);
        }

        events.push(PlayerEvent::Poll);

        // Nav rail:

        crate::left_sidebar::show(
            ui,
            &ctx,
            &self.icon_cache,
            self.nav_rail_open,
            &mut self.active_tab,
            &mut self.chats_open,
            &mut self.show_telegram,
            &mut self.settings_menu_open,
        );

        self.render_settings_menu(&ctx);

        // Central viewport with SharkPlayer (HUD disabled; custom HUD below)
        let (player_response, cache_spans) = {
            let cache_spans = self.current_cache_spans();
            let cache_color = ammini::style::cache_bar_fill(ui.visuals().widgets.inactive.bg_fill);
            let pending = self.pending_seek.clone();
            let r = ui.centered_and_justified(|ui| {
                ui.add(
                    SharkPlayer::new(self.player_mut())
                        .show_hud(false)
                        .seek_callback(Rc::new(move |_forward, target| {
                            *pending.lock().unwrap() = Some(SeekTracker::new(target, _forward));
                        }))
                        .cache_overlay(cache_spans.clone(), cache_color),
                )
            });
            self.video_focus_id = Some(SharkPlayer::player_focus_id(ui));
            (r, cache_spans)
        };

        // Loading overlay
        if self.telegram_load_in_progress() {
            let video_rect = player_response.inner.rect;
            let painter = ui.painter().with_clip_rect(video_rect);
            painter.rect_filled(video_rect, 0.0, egui::Color32::from_black_alpha(70));
            let spinner_size = 40.0;
            let spinner_rect =
                egui::Rect::from_center_size(video_rect.center(), egui::Vec2::splat(spinner_size));
            egui::Spinner::new()
                .size(spinner_size)
                .paint_at(ui, spinner_rect);
            let font_id = egui::TextStyle::Body.resolve(ui.style());
            painter.text(
                egui::pos2(video_rect.center().x, spinner_rect.max.y + 12.0),
                egui::Align2::CENTER_TOP,
                "Loading…",
                font_id,
                ui.visuals().weak_text_color(),
            );
        }

        // In fullscreen the SharkPlayer allocates a zero-size widget and renders into
        // an overlay, so fall back to the whole content rect for HUD/scrim anchoring.
        let is_fullscreen = ctx.input(|i| i.viewport().fullscreen).unwrap_or(false);
        let video_rect = if is_fullscreen {
            ctx.content_rect()
        } else {
            player_response.inner.rect
        };
        // Double-click on the video toggles fullscreen. Read from raw pointer input so it
        // works windowed and fullscreen regardless of which layer the player draws on.
        if ctx.input(|i| {
            i.pointer
                .button_double_clicked(egui::PointerButton::Primary)
        }) {
            if let Some(pos) = ctx.pointer_interact_pos() {
                const UI_LAYERS: [&str; 10] = [
                    "hud",
                    "top_header",
                    "volume_popup",
                    "speed_menu",
                    "subs_menu",
                    "audio_menu",
                    "settings_menu",
                    "chats_overlay",
                    "inspector_overlay",
                    "hud_scrim",
                ];
                let over_ui = ctx
                    .layer_id_at(pos)
                    .is_some_and(|l| UI_LAYERS.iter().any(|n| l.id == egui::Id::new(*n)));
                let in_video = video_rect.contains(pos) && pos.y >= 40.0;
                if in_video && !over_ui && !self.show_url_dialog {
                    ctx.send_viewport_cmd(egui::ViewportCommand::Fullscreen(!is_fullscreen));
                }
            }
        }
        let hovered = ctx
            .pointer_hover_pos()
            .map(|p| video_rect.contains(p) || p.y < 40.0)
            .unwrap_or(false);
        let is_paused = self.player_mut().paused().unwrap_or(true);
        if hovered {
            if let Some(pos) = ctx.pointer_hover_pos() {
                if self.hud_hover.map(|(_, prev)| prev != pos).unwrap_or(true) {
                    self.hud_hover = Some((std::time::Instant::now(), pos));
                }
            }
        } else {
            self.hud_hover = None;
        }
        let moved = ctx.input(|i| i.pointer.delta() != egui::Vec2::ZERO || i.pointer.any_down());
        if moved {
            self.last_input = std::time::Instant::now();
        }
        let menu_open = self.audio_menu_open || self.subs_menu_open || self.volume_popup_hovered;
        let hud_on = (!is_paused
            && self.last_input.elapsed() < std::time::Duration::from_millis(4000))
            || is_paused
            || menu_open;
        let show_hud = hud_on && !self.chats_open && !self.inspector_open;
        let hud_alpha = ctx.animate_bool_with_time(egui::Id::new("hud"), show_hud, 0.18);
        if !hud_on {
            ctx.set_cursor_icon(egui::CursorIcon::None);
        }
        if !hud_on {
            ctx.request_repaint_after(std::time::Duration::from_millis(250));
        }
        // Floating top header (toggles), visible with HUD
        if show_hud {
            crate::top_bar::show(
                &ctx,
                &mut self.nav_rail_open,
                &mut self.chats_open,
                &mut self.inspector_open,
                hud_alpha,
            );
        }

        self.paint_scrim(&ctx, video_rect, hud_alpha, show_hud);

        if show_hud {
            self.render_hud(
                ui,
                &ctx,
                video_rect,
                &cache_spans,
                hud_alpha,
                show_hud,
                is_paused,
                is_fullscreen,
            );
        }
        if self.chats_open {
            self.render_chats_overlay(ui, &ctx, video_rect);
        }
        if self.inspector_open {
            self.render_inspector_overlay(ui, &ctx, video_rect);
        }
        if self.show_url_dialog {
            self.handle_url_dialog(ui, &mut events);
        }

        for event in events {
            if matches!(
                event,
                PlayerEvent::OpenFile(_)
                    | PlayerEvent::OpenUrl(_)
                    | PlayerEvent::OpenTelegramUrl(_)
            ) {
                self.byte_time_map = ammini::telegram::cache::ByteTimeMap::default();
                self.byte_time_record_last = None;
                let speed = self.playback_speed;
                let _ = self.player_mut().mpv().set_property("speed", speed);
            }
            match event {
                PlayerEvent::Poll => trace!("poll"),
                _ => debug!("{event:?}"),
            }
            self.fsm.handle(&event);
        }
        // VideoReady handling from drained messages needs OpenTelegramUrl event; handle via telegram fsm data?
        // We'll handle OpenTelegramUrl by checking if a video was just readied — skip for now; telegram panel handles play.

        if (self.audio_menu_open || self.subs_menu_open || self.speed_menu_open)
            && self.frame_count > self.menu_opened_frame
        {
            if ctx.input(|i| i.pointer.any_click()) {
                self.audio_menu_open = false;
                self.subs_menu_open = false;
                self.speed_menu_open = false;
            }
        }
        self.poll_seek();
        self.update_display_sleep();
    }

    fn on_exit(&mut self, _gl: Option<&eframe::glow::Context>) {
        self.player_mut().destroy_gl_resources();
    }

    fn save(&mut self, storage: &mut dyn eframe::Storage) {
        if let Ok(volume) = self.player_mut().volume() {
            let persistent = unsafe { &mut self.fsm.inner_mut().persistent };
            persistent.volume = Some(volume);
            persistent.playback_speed = self.playback_speed;
        }
        unsafe { self.fsm.inner_mut().finalize_session() };
        if let Ok(s) = serde_json::to_string(&self.fsm.persistent) {
            storage.set_string(crate::app::APP_KEY, s);
        }
    }
}
