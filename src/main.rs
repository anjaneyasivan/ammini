use ammini::fonts::icon_label;
use ammini::fsm::{
    PersistentState, PlayerEvent, PlayerFsm, RecentTelegram, record_recent_telegram, track_label,
};
use ammini::telegram::config::TelegramConfig;
use ammini::telegram::panel::TelegramPanel;
use ammini::telegram::state_machine::{TelegramEvent, TelegramFsm};
use ammini::telegram::{BgCommand, UiMessage, start};
use ammini::telemetry::{Metric, SeekTracker, TelemetryEvent};
use eframe::{App, Frame, NativeOptions, egui};
use egui_material_icons::{MaterialIcon, icons::*};
use egui_sharkplayer::{PlayerState, SharkPlayer};
use rfd::FileDialog;
use statig::blocking::StateMachine;
use statig::prelude::*;
use std::rc::Rc;
use std::sync::Arc;
use tokio::sync::mpsc::{UnboundedReceiver, UnboundedSender};
use tracing::{debug, info, trace};

/// eframe storage key. Deliberately kept as the app's former name (`min_mpv_state`)
/// so existing saves (recent files, playlist, volume, resume positions, recent
/// Telegram videos) keep loading after the rename to Ammini.
const APP_KEY: &str = "min_mpv_state";
const VIDEO_EXTS: &[&str] = &["mp4", "mkv", "avi", "mov", "webm", "ogv", "flv"];

/// Material icons for the on-video control bar (play/pause, seek, volume, fullscreen)
/// rendered by egui-sharkplayer. The crate's default provider uses plain text ("<<",
/// ">>", "▶"…) for these; our bundled material-icons font covers them properly.
struct PlayerControlIcons;

/// Icon size for the on-video control bar. The crate lays the bar out at 40px and
/// buttons inherit egui's 13px `TextStyle::Button`, which reads small over video.
const PLAYER_ICON_SIZE: f32 = 20.0;

fn player_icon(icon: MaterialIcon) -> egui::WidgetText {
    icon.rich_text().size(PLAYER_ICON_SIZE).into()
}

impl egui_sharkplayer::ControlsIconProvider for PlayerControlIcons {
    fn play(&self) -> egui::WidgetText {
        player_icon(ICON_PLAY_ARROW)
    }
    fn pause(&self) -> egui::WidgetText {
        player_icon(ICON_PAUSE)
    }
    fn skip_backward(&self) -> egui::WidgetText {
        player_icon(ICON_SKIP_PREVIOUS)
    }
    fn skip_forward(&self) -> egui::WidgetText {
        player_icon(ICON_SKIP_NEXT)
    }
    fn info(&self) -> egui::WidgetText {
        player_icon(ICON_INFO)
    }
    fn muted_volume(&self) -> egui::WidgetText {
        player_icon(ICON_VOLUME_OFF)
    }
    fn low_volume(&self) -> egui::WidgetText {
        player_icon(ICON_VOLUME_MUTE)
    }
    fn medium_volume(&self) -> egui::WidgetText {
        player_icon(ICON_VOLUME_DOWN)
    }
    fn high_volume(&self) -> egui::WidgetText {
        player_icon(ICON_VOLUME_UP)
    }
    fn fullscreen(&self) -> egui::WidgetText {
        player_icon(ICON_FULLSCREEN)
    }
    fn fullscreen_exit(&self) -> egui::WidgetText {
        player_icon(ICON_FULLSCREEN_EXIT)
    }
}

/// A media track (audio or subtitle) discovered via mpv's scalar `track-list/N/*`
/// sub-properties.
#[derive(Clone)]
struct MediaTrack {
    id: i64,
    label: String,
    selected: bool,
}

struct AmminiApp {
    fsm: StateMachine<PlayerFsm>,
    url_input: String,
    show_url_dialog: bool,
    telegram_fsm: StateMachine<TelegramFsm>,
    telegram_panel: TelegramPanel,
    show_telegram: bool,
    bg_tx: Option<UnboundedSender<BgCommand>>,
    ui_rx: UnboundedReceiver<UiMessage>,
    /// egui id of the video surface, captured each frame so global shortcuts don't
    /// double-handle keys the SharkPlayer widget already owns while focused.
    video_focus_id: Option<egui::Id>,
    /// Audio tracks of the currently loaded media (empty until media is loaded).
    audio_tracks: Vec<MediaTrack>,
    /// Subtitle tracks of the currently loaded media (empty until media is loaded).
    subtitle_tracks: Vec<MediaTrack>,
    /// Last seen `track-list/count`, so the lists are only rebuilt when it changes.
    audio_track_count: i64,
    subtitle_track_count: i64,
    /// Set by the SharkPlayer seek callback (arrow keys / J / L / skip buttons);
    /// drained once per frame to measure seek latency. Shared with the widget via
    /// an Arc because the callback outlives the frame that installed it.
    pending_seek: Arc<std::sync::Mutex<Option<SeekTracker>>>,
}

impl AmminiApp {
    fn new(
        cc: &eframe::CreationContext<'_>,
        bg_tx: Option<UnboundedSender<BgCommand>>,
        ui_rx: UnboundedReceiver<UiMessage>,
    ) -> Result<Self, Box<dyn std::error::Error + Send + Sync>> {
        // Install the custom font stack (modern emoji + system script fallbacks) before
        // any UI is drawn.
        ammini::fonts::install(&cc.egui_ctx);
        // Material icon glyphs must be registered AFTER `fonts::install` (which uses
        // `set_fonts` and would replace them); `initialize` uses `add_font`, which merges.
        egui_material_icons::initialize(&cc.egui_ctx);
        // macOS-style spacing/radii/theme after the fonts are in place.
        ammini::style::install(&cc.egui_ctx);

        let player = PlayerState::new(cc).map_err(|e| {
            Box::new(std::io::Error::other(format!(
                "failed to initialize player: {e}"
            ))) as Box<dyn std::error::Error + Send + Sync>
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
            resume_pending: None,
            last_resume_write: None,
        };
        if let Some(volume) = player_fsm.persistent.volume
            && let Err(e) = player_fsm.player.set_volume(volume)
        {
            tracing::warn!("failed to restore volume {volume}: {e}");
        }
        let fsm = player_fsm.state_machine();

        let mut telegram_fsm = TelegramFsm::new().state_machine();
        telegram_fsm.init();

        let mut app = Self {
            fsm,
            url_input: String::new(),
            show_url_dialog: false,
            telegram_fsm,
            telegram_panel: TelegramPanel::new(),
            show_telegram: false,
            bg_tx,
            ui_rx,
            video_focus_id: None,
            audio_tracks: Vec::new(),
            audio_track_count: 0,
            subtitle_tracks: Vec::new(),
            subtitle_track_count: 0,
            pending_seek: Arc::new(std::sync::Mutex::new(None)),
        };
        app.fsm.init();
        info!("app initialized");
        Ok(app)
    }

    fn player_mut(&mut self) -> &mut PlayerState {
        // SAFETY: we only mutate the player for rendering and cleanup; the state
        // machine itself never holds a reference to the player.
        unsafe { &mut self.fsm.inner_mut().player }
    }

    fn proxy_url_mut(&mut self) -> &mut Option<String> {
        // SAFETY: proxy_url is only mutated outside state machine handlers; it is not
        // part of the state machine's invariants.
        unsafe { &mut self.fsm.inner_mut().proxy_url }
    }

    fn status_mut(&mut self) -> &mut String {
        // SAFETY: status is only mutated outside state machine handlers; it is not
        // part of the state machine's invariants.
        unsafe { &mut self.fsm.inner_mut().status }
    }

    fn open_file_dialog(&self) -> Option<PlayerEvent> {
        FileDialog::new()
            .add_filter("Video files", VIDEO_EXTS)
            .pick_file()
            .map(|p| PlayerEvent::OpenFile(p.to_string_lossy().to_string()))
    }

    fn open_folder_dialog(&mut self) -> Vec<PlayerEvent> {
        let mut events = Vec::new();
        if let Some(folder) = FileDialog::new().pick_folder() {
            let mut videos: Vec<String> = std::fs::read_dir(&folder)
                .ok()
                .map(|iter| {
                    iter.filter_map(|e| e.ok())
                        .filter(|e| {
                            e.path()
                                .extension()
                                .and_then(|ext| ext.to_str())
                                .map(|ext| VIDEO_EXTS.contains(&ext.to_lowercase().as_str()))
                                .unwrap_or(false)
                        })
                        .map(|e| e.path().to_string_lossy().to_string())
                        .collect()
                })
                .unwrap_or_default();
            videos.sort();

            if !videos.is_empty() {
                let first = videos.remove(0);
                events.push(PlayerEvent::OpenFile(first));
                for v in videos {
                    events.push(PlayerEvent::AddToPlaylist(vec![v]));
                }
            } else {
                *self.status_mut() = "No video files found in folder".into();
            }
        }
        events
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
        // Let text fields (URL dialog, Telegram auth inputs) own the keyboard.
        if ctx.egui_wants_keyboard_input() {
            return;
        }

        // Transport keys are handled by the SharkPlayer widget while the video surface
        // has focus; these app-level fallbacks cover clicks elsewhere in the window.
        let video_focused = self
            .video_focus_id
            .is_some_and(|id| ctx.memory(|m| m.has_focus(id)));
        if !video_focused {
            if ctx.input(|i| i.key_pressed(egui::Key::Space))
                && let Err(e) = self.player_mut().toggle_pause()
            {
                tracing::warn!("failed to toggle pause: {e}");
            }
            if ctx.input(|i| i.key_pressed(egui::Key::M))
                && let Err(e) = self.player_mut().toggle_mute()
            {
                tracing::warn!("failed to toggle mute: {e}");
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
        }

        if ctx.input(|i| i.key_pressed(egui::Key::O) && i.modifiers.command && !i.modifiers.shift)
            && let Some(e) = self.open_file_dialog()
        {
            events.push(e);
        }
        if ctx.input(|i| i.key_pressed(egui::Key::O) && i.modifiers.command && i.modifiers.shift) {
            events.extend(self.open_folder_dialog());
        }
        if ctx.input(|i| i.key_pressed(egui::Key::U) && i.modifiers.command) {
            self.show_url_dialog = true;
        }
        if ctx.input(|i| i.key_pressed(egui::Key::P) && i.modifiers.command) {
            events.push(PlayerEvent::TogglePlaylist);
        }
        if ctx.input(|i| i.key_pressed(egui::Key::T) && i.modifiers.command) {
            self.show_telegram = !self.show_telegram;
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
                    if ui.button(icon_label(ui, ICON_CHECK, "Load")).clicked()
                        && !self.url_input.trim().is_empty()
                    {
                        let url = self.url_input.trim().to_string();
                        info!("user submitted URL: {url}");
                        events.push(PlayerEvent::OpenUrl(url));
                        close = true;
                    }
                    if ui.button(icon_label(ui, ICON_CLOSE, "Cancel")).clicked() {
                        close = true;
                    }
                });
            });

        if close {
            self.show_url_dialog = false;
            self.url_input.clear();
        }
    }

    /// Detect an in-flight arrow/skip seek whose playback has resumed past the
    /// seek target, and emit the seek-latency metric. Runs once per frame.
    fn poll_seek(&mut self) {
        let pending = self.pending_seek.clone();
        let Some(mut tracker) = pending.lock().unwrap().take() else {
            return;
        };
        if tracker.elapsed() > ammini::telemetry::SEEK_PENDING_TIMEOUT {
            return; // seek never resumed (e.g. paused and never unpaused) — drop
        }
        if let Ok(Some(position)) = self.player_mut().time_pos()
            && let Some(ms) = tracker.poll(position)
        {
            ammini::telemetry::emit(TelemetryEvent::Metric(Metric::SeekLatencyMs { ms }));
        }
    }

    /// Refresh the audio and subtitle track lists from mpv. Only the scalar
    /// `track-list/N/*` sub-properties are read (libmpv2 cannot read the `track-list`
    /// node itself); each list is rebuilt only when its track count changes, so
    /// steady-state costs one property read per frame. Called every frame before the
    /// top bar.
    fn refresh_tracks(&mut self) {
        let loaded = matches!(self.player_mut().duration(), Ok(Some(_)));
        if !loaded {
            if !self.audio_tracks.is_empty() || self.audio_track_count != 0 {
                self.audio_tracks.clear();
                self.audio_track_count = 0;
            }
            if !self.subtitle_tracks.is_empty() || self.subtitle_track_count != 0 {
                self.subtitle_tracks.clear();
                self.subtitle_track_count = 0;
            }
            return;
        }

        let mut audio_tracks = Vec::new();
        let mut subtitle_tracks = Vec::new();
        let count = {
            let mpv = self.player_mut().mpv();
            let count = mpv.get_property("track-list/count").unwrap_or(0);
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

        // Rebuild when either count changes or a list transitions empty ↔ non-empty;
        // otherwise keep the cached lists (labels are stable).
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
        tracing::debug!(
            "media tracks: audio={:?} subs={:?}",
            self.audio_tracks
                .iter()
                .map(|t| (t.id, t.label.clone(), t.selected))
                .collect::<Vec<_>>(),
            self.subtitle_tracks
                .iter()
                .map(|t| (t.id, t.label.clone(), t.selected))
                .collect::<Vec<_>>()
        );
    }

    /// A dropdown listing the given media tracks (audio or subtitle); returns the id of
    /// the track the user picked, or `None`. Disabled while there are no tracks, with
    /// `fallback` shown as the menu label then.
    fn track_switcher_menu(
        ui: &mut egui::Ui,
        icon: MaterialIcon,
        tracks: &[MediaTrack],
        fallback: &str,
    ) -> Option<i64> {
        let current_label = tracks
            .iter()
            .find(|t| t.selected)
            .map(|t| t.label.clone())
            .unwrap_or_else(|| fallback.to_string());
        let tracks = tracks.to_vec();
        let mut chosen = None;
        ui.add_enabled_ui(!tracks.is_empty(), |ui| {
            ui.menu_button(icon_label(ui, icon, &current_label), |ui| {
                for track in &tracks {
                    if ui.selectable_label(track.selected, &track.label).clicked() {
                        chosen = Some(track.id);
                        ui.close();
                    }
                }
            });
        });
        chosen
    }

    fn top_bar(&mut self, ui: &mut egui::Ui, events: &mut Vec<PlayerEvent>) {
        let mut clear_resume = false;
        ui.horizontal(|ui| {
            if ui
                .button(icon_label(ui, ICON_FILE_OPEN, "Open File"))
                .clicked()
                && let Some(e) = self.open_file_dialog()
            {
                events.push(e);
            }
            if ui
                .button(icon_label(ui, ICON_FOLDER_OPEN, "Open Folder"))
                .clicked()
            {
                events.extend(self.open_folder_dialog());
            }
            if ui.button(icon_label(ui, ICON_LINK, "Open URL")).clicked() {
                self.show_url_dialog = true;
            }
            ui.separator();
            if ui
                .button(icon_label(ui, ICON_SKIP_PREVIOUS, "Prev"))
                .clicked()
            {
                events.push(PlayerEvent::Previous);
            }
            if ui.button(icon_label(ui, ICON_SKIP_NEXT, "Next")).clicked() {
                events.push(PlayerEvent::Next);
            }
            ui.separator();
            if ui
                .button(icon_label(ui, ICON_PLAYLIST_PLAY, "Playlist"))
                .clicked()
            {
                events.push(PlayerEvent::TogglePlaylist);
            }
            if ui.button(icon_label(ui, ICON_SEND, "Telegram")).clicked() {
                self.show_telegram = !self.show_telegram;
            }

            // Audio / subtitle track switchers: disabled until mpv reports any tracks.
            let chosen_aid =
                Self::track_switcher_menu(ui, ICON_AUDIOTRACK, &self.audio_tracks, "Audio");
            let chosen_sid =
                Self::track_switcher_menu(ui, ICON_SUBTITLES, &self.subtitle_tracks, "Subtitles");
            if let Some(id) = chosen_aid {
                // Label for the telemetry event, captured before the mutable borrow
                // below flips the selection flags.
                let label = self
                    .audio_tracks
                    .iter()
                    .find(|t| t.id == id)
                    .map(|t| t.label.clone())
                    .unwrap_or_else(|| format!("track {id}"));
                match self.player_mut().mpv().set_property("aid", id) {
                    Ok(()) => {
                        for track in &mut self.audio_tracks {
                            track.selected = track.id == id;
                        }
                        ammini::telemetry::emit(TelemetryEvent::AudioTrackChanged {
                            track_id: id,
                            label,
                        });
                    }
                    Err(e) => tracing::warn!("failed to switch audio track to {id}: {e}"),
                }
            }
            if let Some(id) = chosen_sid {
                match self.player_mut().mpv().set_property("sid", id) {
                    Ok(()) => {
                        for track in &mut self.subtitle_tracks {
                            track.selected = track.id == id;
                        }
                    }
                    Err(e) => tracing::warn!("failed to switch subtitle track to {id}: {e}"),
                }
            }

            ui.menu_button(icon_label(ui, ICON_HISTORY, "Recent"), |ui| {
                if self.fsm.persistent.recent_files.is_empty() {
                    ui.weak("No recent files");
                } else {
                    for path in self.fsm.persistent.recent_files.clone() {
                        if ui.button(truncate_path(&path)).clicked() {
                            ammini::telemetry::emit(TelemetryEvent::RecentItemSelected {
                                kind: ammini::telemetry::RecentKind::File,
                                name: path_basename(&path),
                            });
                            events.push(PlayerEvent::OpenFile(path));
                            ui.close();
                        }
                    }
                    ui.separator();
                    if ui.button("Clear saved positions").clicked() {
                        clear_resume = true;
                    }
                }
                if !self.fsm.persistent.recent_telegram.is_empty() {
                    ui.separator();
                    ui.weak("Recent Telegram files");
                    for entry in self.fsm.persistent.recent_telegram.clone() {
                        let label = format!("{} · {}", entry.chat_name, truncate_path(&entry.name));
                        if ui.button(icon_label(ui, ICON_PLAY_ARROW, &label)).clicked() {
                            ammini::telemetry::emit(TelemetryEvent::RecentItemSelected {
                                kind: ammini::telemetry::RecentKind::Telegram,
                                name: entry.name.clone(),
                            });
                            if let Some(bg) = &self.bg_tx {
                                let _ = bg.send(BgCommand::PlayTelegramVideo {
                                    peer: entry.peer,
                                    msg_id: entry.msg_id,
                                });
                            }
                            ui.close();
                        }
                    }
                }
            });
            if clear_resume {
                // SAFETY: resume positions are plain UI data, not state-machine invariants.
                unsafe { &mut self.fsm.inner_mut().persistent.resume }.clear();
            }

            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                ui.add(egui::Label::new(&self.fsm.status).truncate());
            });
        });
    }

    fn playlist_panel(&mut self, ui: &mut egui::Ui, events: &mut Vec<PlayerEvent>) {
        ui.label("Playlist");
        ui.separator();
        egui::ScrollArea::vertical()
            .id_salt("playlist")
            .show(ui, |ui| {
                for (i, path) in self.fsm.playlist.iter().enumerate() {
                    let selected = self.fsm.current_index == Some(i);
                    if ui.selectable_label(selected, truncate_path(path)).clicked() {
                        events.push(PlayerEvent::SelectTrack(i));
                    }
                }
            });
    }
}

impl App for AmminiApp {
    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut Frame) {
        let ctx = ui.ctx().clone();
        let mut events = Vec::new();

        while let Ok(msg) = self.ui_rx.try_recv() {
            debug!("telegram ui message: {msg:?}");
            match msg {
                UiMessage::ProxyReady { port } => {
                    let url = format!("http://127.0.0.1:{port}");
                    info!("proxy ready at {url}");
                    *self.proxy_url_mut() = Some(url);
                }
                UiMessage::VideoReady { msg_id, url, name } => {
                    info!("telegram: video ready msg_id={msg_id} url={url}");
                    self.telegram_fsm.handle(&TelegramEvent::VideoReady);
                    events.push(PlayerEvent::OpenTelegramUrl(url));
                    // Remember it in the recent-Telegram list (requires the current chat; the
                    // peer holds the access hash needed to refetch on replay).
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
                        // SAFETY: recent-Telegram entries are plain UI data, not
                        // state-machine invariants.
                        let persistent = unsafe { &mut self.fsm.inner_mut().persistent };
                        record_recent_telegram(&mut persistent.recent_telegram, entry);
                    }
                }
                UiMessage::VideoError(e) => {
                    ammini::telemetry::emit(TelemetryEvent::Error {
                        component: "telegram.video",
                        message: e.clone(),
                    });
                    self.telegram_fsm.handle(&TelegramEvent::VideoError(e));
                }
                other => {
                    // Central error/telegram-auth reporting: every background error
                    // funnels through these three messages, so the emit sites stay
                    // in one place instead of scattered through the bg loop.
                    if let UiMessage::AuthError(reason) = &other {
                        ammini::telemetry::emit(TelemetryEvent::TelegramLoginFailed {
                            reason: reason.clone(),
                        });
                    }
                    if let UiMessage::Error(message) = &other {
                        ammini::telemetry::emit(TelemetryEvent::Error {
                            component: "telegram",
                            message: message.clone(),
                        });
                    }
                    if let Some(event) =
                        ammini::telegram::state_machine::ui_message_to_event(&other)
                    {
                        self.telegram_fsm.handle(&event);
                    }
                }
            }
            // One line per drained message, so an empty chat list or thread is instantly
            // diagnosable from the terminal.
            debug!(
                "telegram dialogs={} messages={}",
                self.telegram_fsm.data.dialogs.len(),
                self.telegram_fsm.data.messages.len(),
            );
        }

        self.handle_dropped_files(&ctx, &mut events);
        self.handle_shortcuts(&ctx, &mut events);
        self.handle_url_dialog(ui, &mut events);
        self.refresh_tracks();

        egui::Panel::top("top_bar").show(ui, |ui| {
            self.top_bar(ui, &mut events);
        });

        if self.fsm.show_playlist || self.show_telegram {
            egui::Panel::left("side_panel")
                .default_size(280.0)
                .show(ui, |ui| {
                    if self.show_telegram {
                        self.telegram_panel
                            .ui(ui, &mut self.telegram_fsm, &self.bg_tx);
                    }
                    if self.fsm.show_playlist && self.show_telegram {
                        ui.separator();
                    }
                    if self.fsm.show_playlist {
                        self.playlist_panel(ui, &mut events);
                    }
                });
        }

        events.push(PlayerEvent::Poll);
        for event in events {
            match event {
                PlayerEvent::Poll => trace!("dispatching event: {event:?}"),
                _ => debug!("dispatching event: {event:?}"),
            }
            self.fsm.handle(&event);
        }
        self.poll_seek();

        egui::CentralPanel::default().show(ui, |ui| {
            let pending = self.pending_seek.clone();
            let player_response = ui.add(
                SharkPlayer::new_with_icons(self.player_mut(), PlayerControlIcons).seek_callback(
                    Rc::new(move |_forward, target| {
                        // Arrow keys / J / L / skip buttons — records the seek so the
                        // next frames' `poll_seek` can time when playback resumes.
                        *pending.lock().unwrap() = Some(SeekTracker::new(target, _forward));
                    }),
                ),
            );
            self.video_focus_id = Some(player_response.id);
        });
    }

    fn on_exit(&mut self, _gl: Option<&eframe::glow::Context>) {
        self.player_mut().destroy_gl_resources();
    }

    fn save(&mut self, storage: &mut dyn eframe::Storage) {
        if let Ok(volume) = self.player_mut().volume() {
            // SAFETY: volume is plain UI data, not a state-machine invariant.
            let persistent = unsafe { &mut self.fsm.inner_mut().persistent };
            persistent.volume = Some(volume);
        }
        if let Ok(s) = serde_json::to_string(&self.fsm.persistent) {
            storage.set_string(APP_KEY, s);
        }
    }
}

fn truncate_path(path: &str) -> String {
    let chars: Vec<char> = path.chars().collect();
    if chars.len() > 60 {
        format!(
            "...{}",
            chars[chars.len() - 57..].iter().collect::<String>()
        )
    } else {
        path.to_string()
    }
}

/// Base name of a media path, for telemetry — never full paths.
fn path_basename(path: &str) -> String {
    path.rsplit(['/', '\\']).next().unwrap_or(path).to_owned()
}

fn main() {
    let filter = tracing_subscriber::EnvFilter::try_from_default_env()
        .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("ammini=debug"));
    tracing_subscriber::fmt().with_env_filter(filter).init();
    info!("starting Ammini");

    let telegram_config = match TelegramConfig::from_env() {
        Ok(c) => c,
        Err(e) => {
            eprintln!(
                "Failed to load Telegram configuration: {}\n\n\
                 Copy .env.example to .env and fill in your TELEGRAM_API_ID and \
                 TELEGRAM_API_HASH from https://my.telegram.org/apps",
                e
            );
            std::process::exit(1);
        }
    };

    let (bg_tx, ui_rx) = start(telegram_config);

    // Telemetry — Sentry via the OTLP endpoint (logs/events) plus native metrics
    // through the sentry SDK. Credentials come from `.env` (SENTRY_DSN /
    // SENTRY_OTLP_URL); without them telemetry stays disabled. The guard flushes
    // pending events on exit, after eframe has torn the window down.
    ammini::telemetry::install_panic_hook();
    let _telemetry = ammini::telemetry::start();
    ammini::telemetry::emit(TelemetryEvent::AppStarted {
        version: env!("CARGO_PKG_VERSION").to_string(),
    });

    // Bundled app icon: the blue play squircle, cropped to its edge and resized.
    let icon = eframe::icon_data::from_png_bytes(include_bytes!("../assets/ammini-icon.png"))
        .expect("assets/ammini-icon.png must be a valid PNG");

    let options = NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_inner_size([960.0, 640.0])
            .with_icon(icon),
        renderer: eframe::Renderer::Glow,
        ..Default::default()
    };

    eframe::run_native(
        "Ammini",
        options,
        Box::new(move |cc| {
            let app = AmminiApp::new(cc, Some(bg_tx), ui_rx)?;
            Ok(Box::new(app) as Box<dyn App>)
        }),
    )
    .unwrap();
}
