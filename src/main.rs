use ammini::fsm::{
    FsmState, PersistentState, PlayerEvent, PlayerFsm, RecentTelegram, record_recent_telegram,
    track_label,
};
use ammini::telegram::config::TelegramConfig;
use ammini::telegram::panel::TelegramPanel;
use ammini::telegram::state_machine::{TelegramEvent, TelegramFsm};
use ammini::telegram::{BgCommand, UiMessage, start};
use ammini::telemetry::{Metric, SeekTracker, TelemetryEvent};
use ammini::update::UpdateAvailable;
use eframe::{App, Frame, NativeOptions, egui};
// iconify images are loaded via egui_extras image loaders (installed in App::new)
use egui_sharkplayer::{PlayerState, SharkPlayer};
use rfd::FileDialog;
use statig::blocking::StateMachine;
use statig::prelude::*;
use std::collections::HashMap;
use std::rc::Rc;
use std::sync::{Arc, Mutex};
use tokio::sync::mpsc::{UnboundedReceiver, UnboundedSender};
use tracing::{debug, info, trace};

mod iconify;
mod left_sidebar;
mod right_sidebar;
mod theme;
mod top_bar;

use crate::theme::*;
use iconify::IconCache;

const APP_KEY: &str = "min_mpv_state";
const VIDEO_EXTS: &[&str] = &["mp4", "mkv", "avi", "mov", "webm", "ogv", "flv"];

// ---------------------------------------------------------------------------
// Media track
// ---------------------------------------------------------------------------
#[derive(Clone)]
struct MediaTrack {
    id: i64,
    label: String,
    selected: bool,
}

// ---------------------------------------------------------------------------
// Cache coverage
// ---------------------------------------------------------------------------
#[derive(Clone, Default)]
struct CacheCoverage {
    total_bytes: u64,
    ranges: Vec<(u64, u64)>,
}

// ---------------------------------------------------------------------------
// App
// ---------------------------------------------------------------------------
struct NewPlayerApp {
    // player
    fsm: StateMachine<PlayerFsm>,
    url_input: String,
    show_url_dialog: bool,
    video_focus_id: Option<egui::Id>,
    audio_tracks: Vec<MediaTrack>,
    subtitle_tracks: Vec<MediaTrack>,
    audio_track_count: i64,
    subtitle_track_count: i64,
    pending_seek: Arc<std::sync::Mutex<Option<SeekTracker>>>,
    display_sleep: ammini::display_sleep::Guard,
    cache_coverage: HashMap<i32, CacheCoverage>,
    byte_time_map: ammini::telegram::cache::ByteTimeMap,
    byte_time_record_last: Option<std::time::Instant>,
    // telegram
    show_telegram: bool,
    telegram_fsm: StateMachine<TelegramFsm>,
    telegram_panel: TelegramPanel,
    bg_tx: Option<UnboundedSender<BgCommand>>,
    ui_rx: UnboundedReceiver<UiMessage>,
    // update
    update_rx: UnboundedReceiver<UpdateAvailable>,
    update: Option<UpdateAvailable>,
    show_update_dialog: bool,
    // ui
    active_tab: &'static str,
    nav_rail_open: bool,
    chats_open: bool,
    inspector_open: bool,
    icon_cache: Arc<Mutex<IconCache>>,
    hud_hover: Option<(std::time::Instant, egui::Pos2)>,
    last_input: std::time::Instant,
    audio_menu_open: bool,
    subs_menu_open: bool,
    speed_menu_open: bool,
    playback_speed: f64,
    frame_count: u64,
    menu_opened_frame: u64,
    pending_open_file: Option<String>,
}

impl NewPlayerApp {
    fn new(
        cc: &eframe::CreationContext<'_>,
    ) -> Result<Self, Box<dyn std::error::Error + Send + Sync>> {
        ammini::fonts::install(&cc.egui_ctx);
        egui_material_icons::initialize(&cc.egui_ctx);
        ammini::style::install(&cc.egui_ctx);
        egui_extras::install_image_loaders(&cc.egui_ctx);

        // STYLE.md: dark-only token theme and 4px-grid spacing.
        let mut visuals = egui::Visuals::dark();
        visuals.panel_fill = C_TOOLBAR;
        visuals.window_fill = C_RAISED;
        visuals.override_text_color = Some(C_TEXT);
        cc.egui_ctx.set_visuals(visuals);
        cc.egui_ctx.global_style_mut(|style| {
            style.spacing.item_spacing = egui::vec2(8.0, 8.0);
            style.spacing.button_padding = egui::vec2(10.0, 6.0);
        });

        let player = PlayerState::new(cc).map_err(|e| {
            Box::new(std::io::Error::other(format!("player init: {e}")))
                as Box<dyn std::error::Error + Send + Sync>
        })?;

        let persistent: PersistentState = cc
            .storage
            .and_then(|s| s.get_string(APP_KEY))
            .and_then(|s| serde_json::from_str(&s).ok())
            .unwrap_or_default();

        let player_fsm = PlayerFsm {
            player,
            proxy_url: None,
            playlist: persistent.playlist.clone(),
            current_index: persistent
                .current_index
                .filter(|&i| i < persistent.playlist.len()),
            show_playlist: false,
            persistent,
            status: String::from("Drop a video or press Ctrl/Cmd+O to open"),
            current_media: None,
            resume_pending: None,
            last_resume_write: None,
        };
        if let Some(volume) = player_fsm.persistent.volume
            && let Err(e) = player_fsm.player.set_volume(volume)
        {
            tracing::warn!("failed to restore volume {volume}: {e}");
        }
        let playback_speed = player_fsm.persistent.playback_speed;
        let fsm = player_fsm.state_machine();

        let mut telegram_fsm = TelegramFsm::new().state_machine();
        telegram_fsm.init();

        let (bg_tx, ui_rx) = match TelegramConfig::from_env() {
            Ok(cfg) => {
                let (tx, rx) = start(cfg);
                (Some(tx), rx)
            }
            Err(e) => {
                eprintln!("telegram disabled: {e}");
                let (_tx, rx) = tokio::sync::mpsc::unbounded_channel();
                (None, rx)
            }
        };

        let update_rx = ammini::update::start(cc.egui_ctx.clone());

        let mut app = Self {
            fsm,
            url_input: String::new(),
            show_url_dialog: false,
            video_focus_id: None,
            audio_tracks: Vec::new(),
            subtitle_tracks: Vec::new(),
            audio_track_count: 0,
            subtitle_track_count: 0,
            pending_seek: Arc::new(std::sync::Mutex::new(None)),
            display_sleep: ammini::display_sleep::Guard::new(),
            cache_coverage: HashMap::new(),
            byte_time_map: ammini::telegram::cache::ByteTimeMap::default(),
            byte_time_record_last: None,
            show_telegram: false,
            telegram_fsm,
            telegram_panel: TelegramPanel::new(),
            bg_tx,
            ui_rx,
            update_rx,
            update: None,
            show_update_dialog: false,
            active_tab: "telegram",
            nav_rail_open: true,
            chats_open: true,
            inspector_open: false,
            icon_cache: IconCache::new_shared(),
            hud_hover: None,
            last_input: std::time::Instant::now(),
            audio_menu_open: false,
            subs_menu_open: false,
            speed_menu_open: false,
            playback_speed,
            frame_count: 0,
            menu_opened_frame: 0,
            pending_open_file: None,
        };
        app.fsm.init();
        let _ = app.player_mut().mpv().set_property("speed", playback_speed);
        Ok(app)
    }

    fn player_mut(&mut self) -> &mut PlayerState {
        unsafe { &mut self.fsm.inner_mut().player }
    }

    fn proxy_url_mut(&mut self) -> &mut Option<String> {
        unsafe { &mut self.fsm.inner_mut().proxy_url }
    }

    fn handle_dropped_files(&mut self, ctx: &egui::Context, events: &mut Vec<PlayerEvent>) {
        let dropped: Vec<_> = ctx.input(|i| {
            i.raw
                .dropped_files
                .iter()
                .filter_map(|f| f.path.clone())
                .collect()
        });
        if let Some(first) = dropped.into_iter().next() {
            events.push(PlayerEvent::OpenFile(first.to_string_lossy().to_string()));
        }
    }

    fn handle_shortcuts(&mut self, ctx: &egui::Context, events: &mut Vec<PlayerEvent>) {
        if ctx.input(|i| i.key_pressed(egui::Key::W) && i.modifiers.command) {
            unsafe { self.fsm.inner_mut().finalize_session() };
            ctx.send_viewport_cmd(egui::ViewportCommand::Close);
            return;
        }
        if ctx.input(|i| i.key_pressed(egui::Key::Q) && i.modifiers.command) {
            unsafe { self.fsm.inner_mut().finalize_session() };
            ctx.send_viewport_cmd(egui::ViewportCommand::Close);
            return;
        }
        if ctx.egui_wants_keyboard_input() {
            return;
        }
        if ctx.input(|i| i.key_pressed(egui::Key::Escape)) {
            let is_fullscreen = ctx.input(|i| i.viewport().fullscreen).unwrap_or(false);
            if is_fullscreen {
                ctx.send_viewport_cmd(egui::ViewportCommand::Fullscreen(false));
            }
        }
        let video_focused = self
            .video_focus_id
            .is_some_and(|id| ctx.memory(|m| m.has_focus(id)));
        if !video_focused {
            if ctx.input(|i| i.key_pressed(egui::Key::Space)) {
                let _ = self.player_mut().toggle_pause();
            }
            if ctx.input(|i| i.key_pressed(egui::Key::M)) {
                let _ = self.player_mut().toggle_mute();
            }
            let volume_delta = if ctx
                .input(|i| i.key_pressed(egui::Key::Plus) || i.key_pressed(egui::Key::Equals))
            {
                Some(5.0)
            } else if ctx.input(|i| i.key_pressed(egui::Key::Minus)) {
                Some(-5.0)
            } else {
                None
            };
            if let Some(delta) = volume_delta
                && let Ok(current) = self.player_mut().volume()
            {
                let _ = self
                    .player_mut()
                    .set_volume((current + delta).clamp(0.0, 100.0));
            }
            if ctx.input(|i| i.key_pressed(egui::Key::F)) {
                let is_fullscreen = ctx.input(|i| i.viewport().fullscreen).unwrap_or(false);
                ctx.send_viewport_cmd(egui::ViewportCommand::Fullscreen(!is_fullscreen));
            }
            if ctx.input(|i| i.key_pressed(egui::Key::ArrowRight) && !i.modifiers.command) {
                let _ = self.player_mut().seek_relative(10.0);
            }
            if ctx.input(|i| i.key_pressed(egui::Key::ArrowLeft) && !i.modifiers.command) {
                let _ = self.player_mut().seek_relative(-10.0);
            }
        }
        if ctx.input(|i| i.key_pressed(egui::Key::O) && i.modifiers.command && !i.modifiers.shift) {
            if let Some(path) = FileDialog::new()
                .add_filter("Video files", VIDEO_EXTS)
                .pick_file()
            {
                events.push(PlayerEvent::OpenFile(path.to_string_lossy().to_string()));
            }
        }
        if ctx.input(|i| i.key_pressed(egui::Key::U) && i.modifiers.command) {
            self.show_url_dialog = true;
        }
        if ctx.input(|i| i.key_pressed(egui::Key::T) && i.modifiers.command) {
            self.show_telegram = !self.show_telegram;
        }
        if ctx.input(|i| i.key_pressed(egui::Key::R) && !i.modifiers.command && !i.modifiers.shift)
        {
            self.active_tab = "recent";
            self.chats_open = true;
        }
        if ctx.input(|i| i.key_pressed(egui::Key::ArrowRight) && i.modifiers.command) {
            events.push(PlayerEvent::Next);
        }
        if ctx.input(|i| i.key_pressed(egui::Key::ArrowLeft) && i.modifiers.command) {
            events.push(PlayerEvent::Previous);
        }
    }

    fn handle_url_dialog(&mut self, ui: &mut egui::Ui, events: &mut Vec<PlayerEvent>) {
        if !self.show_url_dialog {
            return;
        }
        let mut close = false;
        egui::Window::new("Open URL")
            .collapsible(false)
            .resizable(false)
            .show(ui.ctx(), |ui| {
                ui.horizontal(|ui| {
                    ui.label("URL:");
                    ui.text_edit_singleline(&mut self.url_input);
                });
                ui.horizontal(|ui| {
                    if ui.button("Load").clicked() && !self.url_input.trim().is_empty() {
                        events.push(PlayerEvent::OpenUrl(self.url_input.trim().to_string()));
                        close = true;
                    }
                    if ui.button("Cancel").clicked() {
                        close = true;
                    }
                });
            });
        if close {
            self.show_url_dialog = false;
            self.url_input.clear();
        }
    }

    fn refresh_tracks(&mut self) {
        let loaded = matches!(self.player_mut().duration(), Ok(Some(_)));
        if !loaded {
            if !self.audio_tracks.is_empty() {
                self.audio_tracks.clear();
                self.audio_track_count = 0;
            }
            if !self.subtitle_tracks.is_empty() {
                self.subtitle_tracks.clear();
                self.subtitle_track_count = 0;
            }
            return;
        }
        let mut audio_tracks = Vec::new();
        let mut subtitle_tracks = Vec::new();
        let count = {
            let mpv = self.player_mut().mpv();
            let count: i64 = mpv.get_property("track-list/count").unwrap_or(0);
            for i in 0..count {
                let Ok(kind) = mpv.get_property::<String>(&format!("track-list/{i}/type")) else {
                    continue;
                };
                let index = match kind.as_str() {
                    "audio" => audio_tracks.len(),
                    "sub" => subtitle_tracks.len(),
                    _ => continue,
                };
                let track = MediaTrack {
                    id: mpv
                        .get_property::<i64>(&format!("track-list/{i}/id"))
                        .unwrap_or(i),
                    label: track_label(
                        mpv.get_property::<String>(&format!("track-list/{i}/title"))
                            .ok()
                            .filter(|s| !s.is_empty()),
                        mpv.get_property::<String>(&format!("track-list/{i}/lang"))
                            .ok()
                            .filter(|s| !s.is_empty()),
                        index,
                    ),
                    selected: mpv
                        .get_property::<bool>(&format!("track-list/{i}/selected"))
                        .unwrap_or(false),
                };
                match kind.as_str() {
                    "audio" => audio_tracks.push(track),
                    "sub" => subtitle_tracks.push(track),
                    _ => {}
                }
            }
            count
        };
        let unchanged = count == self.audio_track_count
            && count == self.subtitle_track_count
            && audio_tracks.is_empty() == self.audio_tracks.is_empty()
            && subtitle_tracks.is_empty() == self.subtitle_tracks.is_empty();
        if unchanged {
            return;
        }
        self.audio_track_count = count;
        self.subtitle_track_count = count;
        self.audio_tracks = audio_tracks;
        self.subtitle_tracks = subtitle_tracks;
    }

    fn current_cache_spans(&mut self) -> Vec<(f64, f64)> {
        let path = self
            .fsm
            .playlist
            .get(self.fsm.current_index.unwrap_or(0))
            .cloned();
        let Some(path) = path else {
            return Vec::new();
        };
        let Some(msg_id) = ammini::telegram::telegram_url_msg_id(&path) else {
            return Vec::new();
        };
        let Some((total_bytes, ranges)) = self
            .cache_coverage
            .get(&msg_id)
            .map(|c| (c.total_bytes, c.ranges.clone()))
        else {
            return Vec::new();
        };
        let duration = self.player_mut().duration().ok().flatten().unwrap_or(0.0);
        ammini::telegram::cache::byte_ranges_to_time_spans_mapped(
            &ranges,
            total_bytes,
            duration,
            &self.byte_time_map,
        )
    }

    fn telegram_load_in_progress(&self) -> bool {
        if !matches!(self.fsm.state(), FsmState::Loading { .. }) {
            return false;
        }
        self.fsm
            .playlist
            .get(self.fsm.current_index.unwrap_or(0))
            .is_some_and(|p| ammini::telegram::telegram_url_msg_id(p).is_some())
    }

    fn poll_seek(&mut self) {
        let pending = self.pending_seek.clone();
        let Some(mut tracker) = pending.lock().unwrap().take() else {
            return;
        };
        if tracker.elapsed() > ammini::telemetry::SEEK_PENDING_TIMEOUT {
            return;
        }
        if let Ok(Some(position)) = self.player_mut().time_pos()
            && let Some(ms) = tracker.poll(position)
        {
            ammini::telemetry::emit(TelemetryEvent::Metric(Metric::SeekLatencyMs { ms }));
        }
    }

    fn update_display_sleep(&mut self) {
        let playing = !self.player_mut().paused().unwrap_or(true)
            && matches!(self.player_mut().duration(), Ok(Some(_)));
        self.display_sleep.set_playing(playing);
    }
}

impl App for NewPlayerApp {
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

        left_sidebar::show(
            ui,
            &ctx,
            &self.icon_cache,
            self.nav_rail_open,
            &mut self.active_tab,
            &mut self.chats_open,
            &mut self.show_telegram,
        );

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
            if r.inner.double_clicked() {
                let is_fs = ctx.input(|i| i.viewport().fullscreen).unwrap_or(false);
                ctx.send_viewport_cmd(egui::ViewportCommand::Fullscreen(!is_fs));
            }
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
        let menu_open = self.audio_menu_open || self.subs_menu_open;
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
            top_bar::show(
                &ctx,
                &mut self.nav_rail_open,
                &mut self.chats_open,
                &mut self.inspector_open,
                hud_alpha,
            );
        }

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

        // URL dialog
        if self.show_url_dialog {
            let mut close = false;
            egui::Window::new("Open URL")
                .collapsible(false)
                .resizable(false)
                .show(&ctx, |ui| {
                    ui.horizontal(|ui| {
                        ui.label("URL:");
                        ui.text_edit_singleline(&mut self.url_input);
                    });
                    ui.horizontal(|ui| {
                        if ui.button("Load").clicked() && !self.url_input.trim().is_empty() {
                            events.push(PlayerEvent::OpenUrl(self.url_input.trim().to_string()));
                            close = true;
                        }
                        if ui.button("Cancel").clicked() {
                            close = true;
                        }
                    });
                });
            if close {
                self.show_url_dialog = false;
                self.url_input.clear();
            }
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
            storage.set_string(APP_KEY, s);
        }
    }
}

// helper for active tab state (no field, reuse active_tab? we removed it; use a simple string in struct? add field)
// We'll add active_tab field via patch below.

fn main() -> eframe::Result<()> {
    let filter = tracing_subscriber::EnvFilter::try_from_default_env()
        .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("ammini=debug"));
    tracing_subscriber::fmt().with_env_filter(filter).init();

    ammini::telemetry::install_panic_hook();
    let _telemetry = ammini::telemetry::start();
    ammini::telemetry::emit(TelemetryEvent::AppStarted {
        version: env!("CARGO_PKG_VERSION").to_string(),
    });

    let icon = eframe::icon_data::from_png_bytes(include_bytes!("../assets/ammini-icon.png"))
        .expect("assets/ammini-icon.png must be a valid PNG");

    let options = NativeOptions {
        renderer: eframe::Renderer::Glow,
        viewport: egui::ViewportBuilder::default()
            .with_inner_size([1280.0, 800.0])
            .with_icon(icon),
        ..Default::default()
    };
    eframe::run_native(
        "Ammini",
        options,
        Box::new(|cc| {
            let app = NewPlayerApp::new(cc)?;
            Ok(Box::new(app))
        }),
    )
}
