use ammini::fonts::icon_label;
use ammini::telegram::config::TelegramConfig;
use ammini::telegram::panel::TelegramPanel;
use ammini::telegram::state_machine::{TelegramEvent, TelegramFsm};
use ammini::telegram::{BgCommand, UiMessage, start};
use eframe::{App, Frame, NativeOptions, egui};
use egui_material_icons::{MaterialIcon, icons::*};
use statig::blocking::StateMachine;
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use tokio::sync::mpsc::{UnboundedReceiver, UnboundedSender};
use statig::prelude::*;

// ---------------------------------------------------------------------------
// Color palette (from redesign)
// ---------------------------------------------------------------------------
const C_WINDOW: egui::Color32 = egui::Color32::from_rgb(0x0e, 0x0e, 0x11);
const C_TOOLBAR: egui::Color32 = egui::Color32::from_rgb(0x16, 0x16, 0x1a);
const C_RAIL: egui::Color32 = egui::Color32::from_rgb(0x11, 0x11, 0x14);
const C_SIDEBAR: egui::Color32 = egui::Color32::from_rgb(0x12, 0x12, 0x15);
const C_VIEWPORT: egui::Color32 = egui::Color32::from_rgb(0x08, 0x08, 0x0a);
const C_CANVAS: egui::Color32 = egui::Color32::from_rgb(0x05, 0x05, 0x07);
const C_RAISED: egui::Color32 = egui::Color32::from_rgb(0x1f, 0x1f, 0x26);
const C_RAISED2: egui::Color32 = egui::Color32::from_rgb(0x22, 0x22, 0x2b);
const C_BORDER2: egui::Color32 = egui::Color32::from_rgb(0x26, 0x26, 0x2e);
const C_TEXT: egui::Color32 = egui::Color32::from_rgb(0xf4, 0xf4, 0xf5);
const C_TEXT2: egui::Color32 = egui::Color32::from_rgb(0xe4, 0xe4, 0xe7);
const C_MUTED: egui::Color32 = egui::Color32::from_rgb(0xa1, 0xa1, 0xaa);
const C_DIM: egui::Color32 = egui::Color32::from_rgb(0x71, 0x71, 0x7a);

// ---------------------------------------------------------------------------
// Media data (placeholder)
// ---------------------------------------------------------------------------
#[derive(Clone)]
struct Media { id: &'static str, channel: &'static str, filename: &'static str, title: &'static str, subtitle: &'static str, mtype: &'static str, quality: &'static str, size: &'static str, progress: u8, unread: u8 }
const MEDIA: &[Media] = &[
    Media { id: "tg-447277", channel: "11 TVSeries @ProSearch Bot", filename: "VisionQuest.S01E02.1080p.10bit.WEBRip.x265.mkv", title: "VisionQuest", subtitle: "S01 E02", mtype: "Series", quality: "1080p", size: "1.42 GB", progress: 68, unread: 3 },
    Media { id: "tg-902114", channel: "Amritha Private Share", filename: "Headhunters.2011.DUAL-AUDIO.NOR-ENG.1080p.mkv", title: "Headhunters", subtitle: "Feature Film (2011)", mtype: "Movies", quality: "1080p", size: "2.85 GB", progress: 100, unread: 0 },
];

// ---------------------------------------------------------------------------
// Nav rail items
// ---------------------------------------------------------------------------
struct NavItem { id: &'static str, icon: MaterialIcon, label: &'static str }
const NAV_TOP: &[NavItem] = &[
    NavItem { id: "library", icon: ICON_MOVIE, label: "Media Library" },
    NavItem { id: "telegram", icon: ICON_MESSAGE, label: "Telegram Streams" },
    NavItem { id: "audio", icon: ICON_MUSIC_NOTE, label: "Audio Output" },
    NavItem { id: "subtitles", icon: ICON_SUBTITLES, label: "Subtitles" },
    NavItem { id: "recent", icon: ICON_HISTORY, label: "Watch History" },
];

// ---------------------------------------------------------------------------
// Iconify icon cache
// ---------------------------------------------------------------------------
#[derive(Clone)]
#[allow(dead_code)]
enum IconState { Loading, Ready(Vec<u8>), Failed(String) }

struct IconCache { dir: PathBuf, entries: HashMap<String, IconState> }

impl IconCache {
    fn new() -> Self {
        let dir = dirs::cache_dir().unwrap_or_else(std::env::temp_dir).join("newplayer_icon_cache");
        let _ = std::fs::create_dir_all(&dir);
        Self { dir, entries: HashMap::new() }
    }
    fn new_shared() -> Arc<Mutex<Self>> { Arc::new(Mutex::new(Self::new())) }
    fn key(prefix: &str, name: &str, color: &str, size: u32) -> String {
        format!("{prefix}_{name}_{color}_{size}").replace(['/', '\\', ':', '*', '?', '"', '<', '>', '|', '#', '%'], "_")
    }
    fn cache_path(&self, key: &str) -> PathBuf { self.dir.join(format!("{key}.svg")) }
    fn state(&mut self, ctx: &egui::Context, prefix: &str, name: &str, color: &str, size: u32) -> IconState {
        let key = Self::key(prefix, name, color, size);
        if let Some(st) = self.entries.get(&key).cloned() {
            match st {
                IconState::Ready(b) => return IconState::Ready(b),
                IconState::Failed(e) => return IconState::Failed(e),
                IconState::Loading => {
                    let path = self.cache_path(&key);
                    if let Ok(bytes) = std::fs::read(&path) { if !bytes.is_empty() {
                        self.entries.insert(key, IconState::Ready(bytes.clone()));
                        return IconState::Ready(bytes);
                    }}
                    return IconState::Loading;
                }
            }
        }
        let path = self.cache_path(&key);
        if let Ok(bytes) = std::fs::read(&path) { if !bytes.is_empty() {
            self.entries.insert(key.clone(), IconState::Ready(bytes.clone()));
            return IconState::Ready(bytes);
        }}
        self.entries.insert(key.clone(), IconState::Loading);
        let ctx = ctx.clone();
        let cache_dir = self.dir.clone();
        let url = format!("https://api.iconify.design/{prefix}/{name}.svg?color={color}&height={size}", color = color.replace('#', "%23"));
        let key2 = key.clone();
        std::thread::spawn(move || {
            let out = std::process::Command::new("curl").args(["-sS", "-L", &url]).output();
            if let Ok(o) = out { if o.status.success() && !o.stdout.is_empty() {
                let _ = std::fs::write(cache_dir.join(format!("{key2}.svg")), &o.stdout);
            }}
            ctx.request_repaint();
        });
        IconState::Loading
    }
}

fn iconify_icon(ui: &mut egui::Ui, cache: &Arc<Mutex<IconCache>>, ctx: &egui::Context,
               prefix: &str, name: &str, color: &str, size: u32) -> bool {
    let state = { let mut c = cache.lock().unwrap(); c.state(ctx, prefix, name, color, size) };
    let desired = egui::vec2(size as f32, size as f32);
    let mut clicked = false;
    match state {
        IconState::Ready(bytes) => {
            let image = egui::Image::from_bytes(format!("{prefix}_{name}.svg"), bytes)
                .max_size(desired).fit_to_original_size(1.0);
            let btn = ui.add(egui::Button::image(image).frame(false).min_size(desired + egui::vec2(8.0,8.0)));
            clicked = btn.clicked();
        }
        IconState::Failed(_) => {
            let btn = ui.add(egui::Button::new("!").min_size(desired + egui::vec2(8.0,8.0)));
            clicked = btn.clicked();
        }
        IconState::Loading => {
            let (rect, _) = ui.allocate_exact_size(desired + egui::vec2(8.0,8.0), egui::Sense::click());
            ui.painter().text(rect.center(), egui::Align2::CENTER_CENTER, "…", egui::FontId::proportional(16.0), C_DIM);
        }
    }
    clicked
}

// ---------------------------------------------------------------------------
// App
// ---------------------------------------------------------------------------
struct NewPlayerApp {
    // telegram
    show_telegram: bool,
    telegram_fsm: StateMachine<TelegramFsm>,
    telegram_panel: TelegramPanel,
    bg_tx: Option<UnboundedSender<BgCommand>>,
    ui_rx: UnboundedReceiver<UiMessage>,
    // ui state
    active_tab: &'static str,
    nav_rail_open: bool,
    chats_open: bool,
    inspector_open: bool,
    search_query: String,
    active_filter: &'static str,
    selected: usize,
    icon_cache: Arc<Mutex<IconCache>>,
}

impl NewPlayerApp {
    fn new(cc: &eframe::CreationContext<'_>) -> Result<Self, Box<dyn std::error::Error + Send + Sync>> {
        ammini::fonts::install(&cc.egui_ctx);
        egui_material_icons::initialize(&cc.egui_ctx);
        ammini::style::install(&cc.egui_ctx);
        egui_extras::install_image_loaders(&cc.egui_ctx);

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

        let mut telegram_fsm = TelegramFsm::new().state_machine();
        telegram_fsm.init();

        Ok(Self {
            show_telegram: false,
            telegram_fsm,
            telegram_panel: TelegramPanel::new(),
            bg_tx,
            ui_rx,
            active_tab: "telegram",
            nav_rail_open: true,
            chats_open: true,
            inspector_open: true,
            search_query: String::new(),
            active_filter: "All",
            selected: 0,
            icon_cache: IconCache::new_shared(),
        })
    }
}

impl App for NewPlayerApp {
    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut Frame) {
        let ctx = ui.ctx().clone();

        // drain telegram messages
        while let Ok(msg) = self.ui_rx.try_recv() {
            if let Some(ev) = ammini::telegram::state_machine::ui_message_to_event(&msg) {
                self.telegram_fsm.handle(&ev);
            }
        }

        ctx.set_visuals(egui::Visuals::dark());

        // Top toolbar
        egui::Panel::top("toolbar").frame(egui::Frame::new().fill(C_TOOLBAR)).show(ui, |ui| {
            ui.set_height(36.0);
            ui.horizontal(|ui| {
                // server badge
                let (r, _) = ui.allocate_exact_size(egui::vec2(150.0, 20.0), egui::Sense::hover());
                ui.painter().rect_filled(r, 4.0, C_RAISED);
                ui.painter().text(r.left_center() + egui::vec2(6.0,0.0), egui::Align2::LEFT_CENTER, "127.0.0.1:54941", egui::FontId::monospace(10.0), C_MUTED);
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    for (id, icon, on) in [("nav", ICON_LAYERS, self.nav_rail_open),
                                           ("chat", ICON_MESSAGE, self.chats_open),
                                           ("insp", ICON_INFO, self.inspector_open)] {
                        let fill = if on { C_RAISED2 } else { C_RAISED };
                        let stroke = if on { egui::Stroke::new(1.0, egui::Color32::from_rgb(0x33,0x33,0x42)) } else { egui::Stroke::new(1.0, C_BORDER2) };
                        let txt_col = if on { C_TEXT } else { C_DIM };
                        if ui.add(egui::Button::new(icon.rich_text().size(14.0).color(txt_col)).fill(fill).stroke(stroke).min_size(egui::vec2(24.0,24.0))).clicked() {
                            match id {
                                "nav" => self.nav_rail_open = !self.nav_rail_open,
                                "chat" => self.chats_open = !self.chats_open,
                                "insp" => self.inspector_open = !self.inspector_open,
                                _ => {}
                            }
                        }
                    }
                    ui.add_space(6.0);
                    if ui.add(egui::Button::new("Open URL").fill(C_RAISED).min_size(egui::vec2(0.0,24.0))).clicked() { self.show_telegram = true; self.chats_open = true; self.active_tab = "telegram"; }
                    if ui.add(egui::Button::new("Open File").fill(C_RAISED).min_size(egui::vec2(0.0,24.0))).clicked() {}
                });
            });
        });

        // Nav rail
        if self.nav_rail_open {
            egui::Panel::left("navrail").resizable(false).default_size(56.0).frame(egui::Frame::new().fill(C_RAIL)).show(ui, |ui| {
                ui.set_width(56.0);
                ui.vertical_centered(|ui| {
                    ui.add_space(8.0);
                    for item in NAV_TOP {
                        let active = self.active_tab == item.id;
                        let fill = if active { C_RAISED2 } else { egui::Color32::TRANSPARENT };
                        let stroke = if active { egui::Stroke::new(1.0, egui::Color32::from_rgb(0x33,0x33,0x42)) } else { egui::Stroke::NONE };
                        let col = if active { C_TEXT } else { C_DIM };
                        let btn = ui.add(egui::Button::new(item.icon.rich_text().size(16.0).color(col)).fill(fill).stroke(stroke).min_size(egui::vec2(40.0,40.0)));
                        if btn.clicked() { self.active_tab = item.id; self.chats_open = true; if item.id == "telegram" { self.show_telegram = true; } }
                        btn.on_hover_text(item.label);
                        ui.add_space(4.0);
                    }
                    ui.add_space(8.0);
                    ui.with_layout(egui::Layout::bottom_up(egui::Align::Center), |ui| {
                        ui.add_space(8.0);
                        let (r, _) = ui.allocate_exact_size(egui::vec2(32.0,32.0), egui::Sense::click());
                        ui.painter().circle_filled(r.center(), 16.0, C_RAISED2);
                        ui.painter().text(r.center(), egui::Align2::CENTER_CENTER, "A", egui::FontId::proportional(12.0), C_TEXT);
                        ui.add_space(8.0);
                        let btn = ui.add(egui::Button::new(ICON_SETTINGS.rich_text().size(16.0).color(C_DIM)).fill(egui::Color32::TRANSPARENT).min_size(egui::vec2(40.0,40.0)));
                        if btn.clicked() { self.active_tab = "settings"; }
                        btn.on_hover_text("Preferences");
                    });
                });
            });
        }

        // Central viewport
        egui::CentralPanel::default().frame(egui::Frame::new().fill(C_VIEWPORT)).show(ui, |ui| {
            let m = &MEDIA[self.selected];
            let avail = ui.available_size();
            let (rect, _) = ui.allocate_exact_size(egui::vec2(avail.x, avail.y - 50.0), egui::Sense::click());
            ui.painter().rect_filled(rect, 0.0, C_CANVAS);
            ui.painter().text(rect.center(), egui::Align2::CENTER_CENTER,
                format!("▶  {} — {}", m.title, m.subtitle),
                egui::FontId::proportional(20.0), C_MUTED);
            ui.add_space(4.0);
            let _ = iconify_icon(ui, &self.icon_cache, &ctx, "material-symbols-light", "play-arrow", "#ffffff", 24);
        });

        // Floating chats overlay (Telegram)
        if self.chats_open {
            let x = if self.nav_rail_open { 60.0 } else { 8.0 };
            egui::Area::new("chats_overlay".into()).fixed_pos(egui::pos2(x, 40.0)).show(&ctx, |ui| {
                egui::Frame::new().fill(C_SIDEBAR).stroke(egui::Stroke::new(1.0, C_BORDER2)).corner_radius(8.0).inner_margin(egui::Margin::symmetric(12, 10)).show(ui, |ui| {
                    ui.set_width(300.0);
                    ui.set_min_height(ctx.viewport_rect().height() - 44.0);
                    ui.set_max_height(ctx.viewport_rect().height() - 44.0);
                    let title = match self.active_tab {
                        "telegram" => "TELEGRAM",
                        "library" => "LIBRARY",
                        "audio" => "AUDIO TRACKS",
                        "subtitles" => "SUBTITLES",
                        "recent" => "RECENT",
                        "settings" => "SETTINGS",
                        _ => "PANEL",
                    };
                    ui.horizontal(|ui| {
                        ui.label(egui::RichText::new(title).monospace().size(11.0).strong().color(C_DIM));
                        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                            if ui.small_button("✕").clicked() { self.chats_open = false; }
                        });
                    });
                    match self.active_tab {
                        "telegram" => {
                            self.telegram_panel.ui(ui, &mut self.telegram_fsm, &self.bg_tx);
                        }
                        "audio" => {
                            ui.label(egui::RichText::new("Audio tracks").color(C_MUTED));
                        }
                        "subtitles" => {
                            ui.label(egui::RichText::new("Subtitle tracks").color(C_MUTED));
                        }
                        "recent" => {
                            ui.label(egui::RichText::new("Recent media").color(C_MUTED));
                        }
                        "settings" => {
                            ui.label(egui::RichText::new("Settings").color(C_MUTED));
                        }
                        _ => {
                            ui.label(egui::RichText::new("Media library").color(C_MUTED));
                        }
                    }
                });
            });
        }

        // Floating inspector overlay
        if self.inspector_open {
            let screen = ctx.viewport_rect();
            egui::Area::new("inspector_overlay".into()).fixed_pos(egui::pos2(screen.width() - 296.0, 40.0)).show(&ctx, |ui| {
                egui::Frame::new().fill(C_SIDEBAR).stroke(egui::Stroke::new(1.0, C_BORDER2)).corner_radius(8.0).inner_margin(egui::Margin::symmetric(12, 10)).show(ui, |ui| {
                    ui.set_width(280.0);
                    ui.set_min_height(ctx.viewport_rect().height() - 44.0);
                    ui.set_max_height(ctx.viewport_rect().height() - 44.0);
                    ui.horizontal(|ui| {
                        ui.label(egui::RichText::new("MEDIA INSPECTOR").monospace().size(11.0).strong().color(C_DIM));
                        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                            if ui.small_button("✕").clicked() { self.inspector_open = false; }
                        });
                    });
                    let m = &MEDIA[self.selected];
                    ui.label(egui::RichText::new(m.title).strong().color(C_TEXT));
                    ui.label(egui::RichText::new(m.subtitle).monospace().size(11.0).color(C_MUTED));
                });
            });
        }
    }
}

fn main() -> eframe::Result<()> {
    let options = NativeOptions {
        renderer: eframe::Renderer::Glow,
        viewport: egui::ViewportBuilder::default().with_inner_size([1280.0, 800.0]),
        ..Default::default()
    };
    eframe::run_native("newplayer", options, Box::new(|cc| {
        let app = NewPlayerApp::new(cc)?;
        Ok(Box::new(app))
    }))
}
