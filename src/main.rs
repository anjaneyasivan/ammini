mod fsm;
mod proxy;
mod telegram;

use eframe::{App, Frame, NativeOptions, egui};
use egui_sharkplayer::{PlayerState, SharkPlayer};
use fsm::{PersistentState, PlayerEvent, PlayerFsm};
use rfd::FileDialog;
use statig::blocking::StateMachine;
use statig::prelude::*;
use telegram::config::TelegramConfig;
use telegram::panel::TelegramPanel;
use telegram::state_machine::{TelegramEvent, TelegramFsm};
use telegram::{BgCommand, UiMessage, start};
use tokio::sync::mpsc::{UnboundedReceiver, UnboundedSender};
use tracing::{debug, info, trace, warn};

const APP_KEY: &str = "min_mpv_state";
const VIDEO_EXTS: &[&str] = &["mp4", "mkv", "avi", "mov", "webm", "ogv", "flv"];

struct MinMpvApp {
    fsm: StateMachine<PlayerFsm>,
    url_input: String,
    show_url_dialog: bool,
    telegram_fsm: StateMachine<TelegramFsm>,
    telegram_panel: TelegramPanel,
    show_telegram: bool,
    bg_tx: Option<UnboundedSender<BgCommand>>,
    ui_rx: UnboundedReceiver<UiMessage>,
}

impl MinMpvApp {
    fn new(
        cc: &eframe::CreationContext<'_>,
        bg_tx: Option<UnboundedSender<BgCommand>>,
        ui_rx: UnboundedReceiver<UiMessage>,
    ) -> Result<Self, Box<dyn std::error::Error + Send + Sync>> {
        let player = PlayerState::new(cc).map_err(|e| {
            Box::new(std::io::Error::new(
                std::io::ErrorKind::Other,
                format!("failed to initialize player: {e}"),
            )) as Box<dyn std::error::Error + Send + Sync>
        })?;

        let persistent: PersistentState = cc
            .storage
            .and_then(|s| s.get_string(APP_KEY))
            .and_then(|s| serde_json::from_str(&s).ok())
            .unwrap_or_default();

        let fsm = PlayerFsm {
            player,
            proxy_url: None,
            playlist: Vec::new(),
            current_index: None,
            show_playlist: false,
            persistent,
            status: String::from("Drop a video or press Ctrl/Cmd+O to open"),
        }
        .state_machine();

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
        if ctx.input(|i| i.key_pressed(egui::Key::O) && i.modifiers.command && !i.modifiers.shift)
        {
            if let Some(e) = self.open_file_dialog() {
                events.push(e);
            }
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
                    if ui.button("Load").clicked() && !self.url_input.trim().is_empty() {
                        let url = self.url_input.trim().to_string();
                        info!("user submitted URL: {url}");
                        events.push(PlayerEvent::OpenUrl(url));
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

    fn top_bar(&mut self, ui: &mut egui::Ui, events: &mut Vec<PlayerEvent>) {
        ui.horizontal(|ui| {
            if ui.button("Open File").clicked() {
                if let Some(e) = self.open_file_dialog() {
                    events.push(e);
                }
            }
            if ui.button("Open Folder").clicked() {
                events.extend(self.open_folder_dialog());
            }
            if ui.button("Open URL").clicked() {
                self.show_url_dialog = true;
            }
            ui.separator();
            if ui.button("Prev").clicked() {
                events.push(PlayerEvent::Previous);
            }
            if ui.button("Next").clicked() {
                events.push(PlayerEvent::Next);
            }
            ui.separator();
            if ui.button("Playlist").clicked() {
                events.push(PlayerEvent::TogglePlaylist);
            }
            if ui.button("Telegram").clicked() {
                self.show_telegram = !self.show_telegram;
            }

            ui.menu_button("Recent", |ui| {
                if self.fsm.persistent.recent_files.is_empty() {
                    ui.weak("No recent files");
                } else {
                    for path in self.fsm.persistent.recent_files.clone() {
                        if ui.button(truncate_path(&path)).clicked() {
                            events.push(PlayerEvent::OpenFile(path));
                            ui.close();
                        }
                    }
                }
            });

            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                ui.add(egui::Label::new(&self.fsm.status).truncate());
            });
        });
    }

    fn playlist_panel(&mut self, ui: &mut egui::Ui, events: &mut Vec<PlayerEvent>) {
        ui.label("Playlist");
        ui.separator();
        egui::ScrollArea::vertical().show(ui, |ui| {
            for (i, path) in self.fsm.playlist.iter().enumerate() {
                let selected = self.fsm.current_index == Some(i);
                if ui.selectable_label(selected, truncate_path(path)).clicked() {
                    events.push(PlayerEvent::SelectTrack(i));
                }
            }
        });
    }
}

impl App for MinMpvApp {
    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut Frame) {
        let ctx = ui.ctx().clone();
        let mut events = Vec::new();

        while let Ok(msg) = self.ui_rx.try_recv() {
            trace!("telegram ui message: {msg:?}");
            match msg {
                UiMessage::ProxyReady { port } => {
                    let url = format!("http://127.0.0.1:{port}");
                    info!("proxy ready at {url}");
                    *self.proxy_url_mut() = Some(url);
                }
                UiMessage::VideoReady { msg_id: _, url } => {
                    self.telegram_fsm.handle(&TelegramEvent::VideoReady);
                    events.push(PlayerEvent::OpenTelegramUrl(url));
                }
                UiMessage::VideoError(e) => {
                    self.telegram_fsm.handle(&TelegramEvent::VideoError(e));
                }
                other => {
                    if let Some(event) = telegram::state_machine::ui_message_to_event(&other) {
                        self.telegram_fsm.handle(&event);
                    }
                }
            }
        }

        self.handle_dropped_files(&ctx, &mut events);
        self.handle_shortcuts(&ctx, &mut events);
        self.handle_url_dialog(ui, &mut events);

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

        egui::CentralPanel::default().show(ui, |ui| {
            ui.add(SharkPlayer::new(self.player_mut()));
        });
    }

    fn on_exit(&mut self, _gl: Option<&eframe::glow::Context>) {
        self.player_mut().destroy_gl_resources();
    }

    fn save(&mut self, storage: &mut dyn eframe::Storage) {
        if let Ok(s) = serde_json::to_string(&self.fsm.persistent) {
            storage.set_string(APP_KEY, s);
        }
    }
}

fn truncate_path(path: &str) -> String {
    let chars: Vec<char> = path.chars().collect();
    if chars.len() > 60 {
        format!("...{}", chars[chars.len() - 57..].iter().collect::<String>())
    } else {
        path.to_string()
    }
}

fn main() {
    let filter = tracing_subscriber::EnvFilter::try_from_default_env()
        .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("min_mpv=debug"));
    tracing_subscriber::fmt().with_env_filter(filter).init();
    info!("starting min-mpv");

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

    let options = NativeOptions {
        viewport: egui::ViewportBuilder::default().with_inner_size([960.0, 640.0]),
        renderer: eframe::Renderer::Glow,
        ..Default::default()
    };

    eframe::run_native(
        "min-mpv",
        options,
        Box::new(move |cc| {
            let app = MinMpvApp::new(cc, Some(bg_tx), ui_rx)?;
            Ok(Box::new(app) as Box<dyn App>)
        }),
    )
    .unwrap();
}
