use ammini::fsm::{FsmState, PersistentState, PlayerEvent, PlayerFsm, track_label};
use ammini::telegram::config::TelegramConfig;
use ammini::telegram::panel::TelegramPanel;
use ammini::telegram::state_machine::TelegramFsm;
use ammini::telegram::{BgCommand, UiMessage, start};
use ammini::telemetry::{Metric, SeekTracker, TelemetryEvent};
use ammini::update::UpdateAvailable;
use eframe::egui;
// iconify images are loaded via egui_extras image loaders (installed in App::new)
use egui_sharkplayer::PlayerState;
use rfd::FileDialog;
use statig::blocking::StateMachine;
use statig::prelude::IntoStateMachineExt;

use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use tokio::sync::mpsc::{UnboundedReceiver, UnboundedSender};

use crate::iconify::IconCache;
use crate::theme::*;
use crate::types::{CacheCoverage, MediaTrack};

pub const APP_KEY: &str = "min_mpv_state";
const VIDEO_EXTS: &[&str] = &["mp4", "mkv", "avi", "mov", "webm", "ogv", "flv"];

pub struct AmminiApp {
    // player
    pub fsm: StateMachine<PlayerFsm>,
    pub url_input: String,
    pub show_url_dialog: bool,
    pub video_focus_id: Option<egui::Id>,
    pub audio_tracks: Vec<MediaTrack>,
    pub subtitle_tracks: Vec<MediaTrack>,
    pub audio_track_count: i64,
    pub subtitle_track_count: i64,
    pub pending_seek: Arc<std::sync::Mutex<Option<SeekTracker>>>,
    pub display_sleep: ammini::display_sleep::Guard,
    pub cache_coverage: HashMap<i32, CacheCoverage>,
    pub byte_time_map: ammini::telegram::cache::ByteTimeMap,
    pub byte_time_record_last: Option<std::time::Instant>,
    // telegram
    pub show_telegram: bool,
    pub telegram_fsm: StateMachine<TelegramFsm>,
    pub telegram_panel: TelegramPanel,
    pub bg_tx: Option<UnboundedSender<BgCommand>>,
    pub ui_rx: UnboundedReceiver<UiMessage>,
    // update
    pub update_rx: UnboundedReceiver<UpdateAvailable>,
    pub update: Option<UpdateAvailable>,
    pub show_update_dialog: bool,
    // ui
    pub active_tab: &'static str,
    pub nav_rail_open: bool,
    pub chats_open: bool,
    pub inspector_open: bool,
    pub icon_cache: Arc<Mutex<IconCache>>,
    pub hud_hover: Option<(std::time::Instant, egui::Pos2)>,
    pub last_input: std::time::Instant,
    pub audio_menu_open: bool,
    pub subs_menu_open: bool,
    pub speed_menu_open: bool,
    pub settings_menu_open: bool,
    pub playback_speed: f64,
    pub frame_count: u64,
    pub menu_opened_frame: u64,
    pub pending_open_file: Option<String>,
}

impl AmminiApp {
    pub fn new(
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
            settings_menu_open: false,
            playback_speed,
            frame_count: 0,
            menu_opened_frame: 0,
            pending_open_file: None,
        };
        app.fsm.init();
        let _ = app.player_mut().mpv().set_property("speed", playback_speed);
        Ok(app)
    }

    pub fn player_mut(&mut self) -> &mut PlayerState {
        unsafe { &mut self.fsm.inner_mut().player }
    }

    pub fn proxy_url_mut(&mut self) -> &mut Option<String> {
        unsafe { &mut self.fsm.inner_mut().proxy_url }
    }

    pub fn handle_dropped_files(&mut self, ctx: &egui::Context, events: &mut Vec<PlayerEvent>) {
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

    pub fn handle_shortcuts(&mut self, ctx: &egui::Context, events: &mut Vec<PlayerEvent>) {
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

    pub fn handle_url_dialog(&mut self, ui: &mut egui::Ui, events: &mut Vec<PlayerEvent>) {
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

    pub fn refresh_tracks(&mut self) {
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

    pub fn current_cache_spans(&mut self) -> Vec<(f64, f64)> {
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

    pub fn telegram_load_in_progress(&self) -> bool {
        if !matches!(self.fsm.state(), FsmState::Loading { .. }) {
            return false;
        }
        self.fsm
            .playlist
            .get(self.fsm.current_index.unwrap_or(0))
            .is_some_and(|p| ammini::telegram::telegram_url_msg_id(p).is_some())
    }

    pub fn poll_seek(&mut self) {
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

    pub fn update_display_sleep(&mut self) {
        let playing = !self.player_mut().paused().unwrap_or(true)
            && matches!(self.player_mut().duration(), Ok(Some(_)));
        self.display_sleep.set_playing(playing);
    }
}
