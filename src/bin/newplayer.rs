use eframe::{App, Frame, NativeOptions, egui};
use egui_material_icons::{MaterialIcon, icons::*};
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};

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
const C_BORDER: egui::Color32 = egui::Color32::from_rgb(0x2b, 0x2b, 0x36);
const C_BORDER2: egui::Color32 = egui::Color32::from_rgb(0x26, 0x26, 0x2e);
const C_TEXT: egui::Color32 = egui::Color32::from_rgb(0xf4, 0xf4, 0xf5);
const C_TEXT2: egui::Color32 = egui::Color32::from_rgb(0xe4, 0xe4, 0xe7);
const C_MUTED: egui::Color32 = egui::Color32::from_rgb(0xa1, 0xa1, 0xaa);
const C_DIM: egui::Color32 = egui::Color32::from_rgb(0x71, 0x71, 0x7a);

// ---------------------------------------------------------------------------
// Media data
// ---------------------------------------------------------------------------
#[derive(Clone)]
struct Media {
    id: &'static str,
    channel: &'static str,
    filename: &'static str,
    title: &'static str,
    subtitle: &'static str,
    mtype: &'static str,
    quality: &'static str,
    codec: &'static str,
    container: &'static str,
    size: &'static str,
    bitrate: &'static str,
    fps: &'static str,
    resolution: &'static str,
    audio_codec: &'static str,
    progress: u8,
    unread: u8,
    status: &'static str,
}

const MEDIA: &[Media] = &[
    Media { id: "tg-447277", channel: "11 TVSeries @ProSearch Bot", filename: "VisionQuest.S01E02.1080p.10bit.WEBRip.x265.mkv", title: "VisionQuest", subtitle: "S01 E02 - The Attachment", mtype: "Series", quality: "1080p", codec: "HEVC / x265 10-bit", container: "MKV", size: "1.42 GB", bitrate: "3.8 Mbps", fps: "23.976", resolution: "1920x1080", audio_codec: "EAC3 5.1 (488 kbps)", progress: 68, unread: 3, status: "Streaming" },
    Media { id: "tg-902114", channel: "Amritha Private Share", filename: "Headhunters.2011.DUAL-AUDIO.NOR-ENG.1080p.mkv", title: "Headhunters", subtitle: "Feature Film (2011)", mtype: "Movies", quality: "1080p", codec: "H.264 / AVC", container: "MKV", size: "2.85 GB", bitrate: "5.2 Mbps", fps: "24.000", resolution: "1920x804", audio_codec: "DTS 5.1 / AAC Stereo", progress: 100, unread: 0, status: "Cached" },
    Media { id: "tg-110293", channel: "Anjaneya Drive", filename: "Hope.2026.KOREAN.1080p.10bit.WEBRip.AV1.mkv", title: "Hope", subtitle: "Feature Film (2026)", mtype: "Movies", quality: "1080p", codec: "AV1 10-bit", container: "MKV", size: "3.10 GB", bitrate: "4.1 Mbps", fps: "23.976", resolution: "1920x1080", audio_codec: "Opus 5.1 (320 kbps)", progress: 42, unread: 1, status: "Downloading" },
    Media { id: "tg-334910", channel: "ലൂസി Cinema Vault", filename: "Deepasthambham.Mahabharatham.1999.720p.mkv", title: "Deepasthambham Mahabharatham", subtitle: "Classic Comedy (1999)", mtype: "Movies", quality: "720p", codec: "H.264", container: "MKV", size: "850 MB", bitrate: "1.8 Mbps", fps: "25.000", resolution: "1280x720", audio_codec: "AAC 2.0 (128 kbps)", progress: 100, unread: 0, status: "Cached" },
];

// ---------------------------------------------------------------------------
// App state
// ---------------------------------------------------------------------------
struct App2 {
    active_tab: &'static str,
    selected: usize,
    nav_rail_open: bool,
    chats_open: bool,
    inspector_open: bool,
    search_query: String,
    active_filter: &'static str,
    icon_cache: Arc<Mutex<IconCache>>,
}

impl Default for App2 {
    fn default() -> Self {
        Self {
            active_tab: "telegram",
            selected: 0,
            nav_rail_open: true,
            chats_open: true,
            inspector_open: true,
            search_query: String::new(),
            active_filter: "All",
            icon_cache: IconCache::new_shared(),
        }
    }
}

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
// Iconify icon cache (async, disk-backed)
// ---------------------------------------------------------------------------
#[derive(Clone)]
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
// Helpers
// ---------------------------------------------------------------------------
fn hline(ui: &mut egui::Ui, color: egui::Color32) {
    let rect = ui.available_rect_before_wrap();
    ui.painter().hline(rect.x_range(), rect.top(), egui::Stroke::new(1.0, color));
    ui.add_space(1.0);
}

// ---------------------------------------------------------------------------
// App
// ---------------------------------------------------------------------------
impl App for App2 {
    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut Frame) {
        let ctx = ui.ctx().clone();

        // Force dark visuals
        ctx.set_visuals(egui::Visuals::dark());

        // Top toolbar
        egui::Panel::top("toolbar").frame(egui::Frame::new().fill(C_TOOLBAR)).show(ui, |ui| {
            ui.set_height(40.0);
            ui.horizontal(|ui| {
                // traffic lights
                for c in [egui::Color32::from_rgb(0xff,0x5f,0x56),
                          egui::Color32::from_rgb(0xff,0xbd,0x2e),
                          egui::Color32::from_rgb(0x27,0xc9,0x3f)] {
                    let (r, _) = ui.allocate_exact_size(egui::vec2(12.0,12.0), egui::Sense::click());
                    ui.painter().circle_filled(r.center(), 6.0, c);
                }
                ui.add_space(8.0);
                ui.label(egui::RichText::new("Ammini").strong().color(C_TEXT));
                ui.label(egui::RichText::new("v2.5.0").monospace().size(10.0).color(C_DIM));
                ui.add_space(8.0);
                // server badge
                let (r, _) = ui.allocate_exact_size(egui::vec2(150.0, 20.0), egui::Sense::hover());
                ui.painter().rect_filled(r, 4.0, C_RAISED);
                ui.painter().text(r.left_center() + egui::vec2(6.0,0.0), egui::Align2::LEFT_CENTER, "127.0.0.1:54941", egui::FontId::monospace(10.0), C_MUTED);
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    let toggles = [("nav", ICON_LAYERS, self.nav_rail_open),
                                   ("chat", ICON_MESSAGE, self.chats_open),
                                   ("insp", ICON_INFO, self.inspector_open)];
                    for (id, icon, on) in toggles {
                        let fill = if on { C_RAISED2 } else { C_RAISED };
                        let txt_col = if on { C_TEXT } else { C_DIM };
                        if ui.add(egui::Button::new(icon.rich_text().size(14.0).color(txt_col)).fill(fill).min_size(egui::vec2(24.0,24.0))).clicked() {
                            match id {
                                "nav" => self.nav_rail_open = !self.nav_rail_open,
                                "chat" => self.chats_open = !self.chats_open,
                                "insp" => self.inspector_open = !self.inspector_open,
                                _ => {}
                            }
                        }
                    }
                    ui.add_space(6.0);
                    for (label, icon) in [("Open URL", ICON_GLOBE), ("Open File", ICON_FOLDER)] {
                        if ui.add(egui::Button::new(format!("{}", label)).fill(C_RAISED).min_size(egui::vec2(0.0,24.0))).clicked() {}
                        let _ = icon;
                    }
                });
            });
        });

        // Nav rail (left, outermost)
        if self.nav_rail_open {
            egui::Panel::left("navrail").resizable(false).default_size(56.0).frame(egui::Frame::new().fill(C_RAIL)).show(ui, |ui| {
                ui.set_width(56.0);
                ui.vertical_centered(|ui| {
                    ui.add_space(8.0);
                    for item in NAV_TOP {
                        let active = self.active_tab == item.id;
                        let fill = if active { C_RAISED2 } else { egui::Color32::TRANSPARENT };
                        let col = if active { C_TEXT } else { C_DIM };
                        let btn = ui.add(egui::Button::new(item.icon.rich_text().size(16.0).color(col)).fill(fill).min_size(egui::vec2(40.0,40.0)));
                        if btn.clicked() { self.active_tab = item.id; }
                        btn.on_hover_text(item.label);
                        ui.add_space(4.0);
                    }
                    ui.add_space(8.0);
                    // bottom settings + avatar
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
            ui.set_width(ui.available_width());
            let m = &MEDIA[self.selected];
            // poster canvas
            let avail = ui.available_size();
            let (rect, _) = ui.allocate_exact_size(egui::vec2(avail.x, avail.y - 60.0), egui::Sense::click());
            ui.painter().rect_filled(rect, 0.0, C_CANVAS);
            ui.painter().text(rect.center(), egui::Align2::CENTER_CENTER,
                format!("▶  {} — {}", m.title, m.subtitle),
                egui::FontId::proportional(20.0), C_MUTED);
            // HUD toolbar
            let (hud, _) = ui.allocate_exact_size(egui::vec2(avail.x, 60.0), egui::Sense::hover());
            ui.painter().rect_filled(hud, 0.0, C_VIEWPORT);
            // timeline
            let tl = egui::Rect::from_min_size(hud.min + egui::vec2(12.0, 8.0), egui::vec2(hud.width() - 24.0, 6.0));
            ui.painter().rect_filled(tl, 3.0, C_RAISED);
            let played = tl.width() * 0.35;
            ui.painter().rect_filled(egui::Rect::from_min_size(tl.min, egui::vec2(played, tl.height())), 3.0, C_TEXT);
            // controls row
            ui.painter().text(hud.min + egui::vec2(12.0, 36.0), egui::Align2::LEFT_CENTER,
                format!("{} / {}", "12:34", "53:21"), egui::FontId::monospace(11.0), C_MUTED);
            // play button
            let play_rect = egui::Rect::from_center_size(hud.center() + egui::vec2(0.0, 12.0), egui::vec2(32.0,32.0));
            ui.painter().circle_filled(play_rect.center(), 16.0, C_RAISED2);
            ui.painter().text(play_rect.center(), egui::Align2::CENTER_CENTER, "▶", egui::FontId::proportional(14.0), C_TEXT);
            // arcticons telegram icon demo
            ui.add_space(4.0);
            let _ = iconify_icon(ui, &self.icon_cache, &ctx, "arcticons", "telegram", "#ffffff", 24);
        });

        // Floating chats overlay (does not resize the viewport)
        if self.chats_open {
            let x = if self.nav_rail_open { 60.0 } else { 8.0 };
            egui::Area::new("chats_overlay".into())
                .fixed_pos(egui::pos2(x, 44.0))
                .show(&ctx, |ui| {
                    egui::Frame::new().fill(C_SIDEBAR).stroke(egui::Stroke::new(1.0, C_BORDER2)).corner_radius(8.0).show(ui, |ui| {
                        ui.set_width(280.0);
                        ui.set_max_height(520.0);
                        ui.horizontal(|ui| {
                            ui.label(egui::RichText::new("CHATS & FILES").monospace().size(11.0).strong().color(C_DIM));
                            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                                if ui.small_button("✕").clicked() { self.chats_open = false; }
                            });
                        });
                        ui.add(egui::TextEdit::singleline(&mut self.search_query).hint_text("Filter media...").desired_width(f32::INFINITY));
                        ui.horizontal(|ui| {
                            for f in ["All", "Movies", "Series"] {
                                let active = self.active_filter == f;
                                let fill = if active { C_RAISED2 } else { egui::Color32::TRANSPARENT };
                                if ui.add(egui::Button::new(f).fill(fill).min_size(egui::vec2(0.0,20.0))).clicked() {
                                    self.active_filter = f;
                                }
                            }
                        });
                        egui::ScrollArea::vertical().max_height(400.0).show(ui, |ui| {
                            for (i, m) in MEDIA.iter().enumerate() {
                                let matches = m.filename.to_lowercase().contains(&self.search_query.to_lowercase())
                                    || m.channel.to_lowercase().contains(&self.search_query.to_lowercase())
                                    || m.title.to_lowercase().contains(&self.search_query.to_lowercase());
                                let type_ok = self.active_filter == "All"
                                    || (self.active_filter == "Movies" && m.mtype == "Movies")
                                    || (self.active_filter == "Series" && m.mtype == "Series");
                                if !(matches && type_ok) { continue; }
                                let selected = self.selected == i;
                                let bg = if selected { C_RAISED2 } else { egui::Color32::TRANSPARENT };
                                egui::Frame::new().fill(bg).inner_margin(egui::Margin::symmetric(8, 6)).show(ui, |ui| {
                                    ui.label(egui::RichText::new(m.channel).size(12.0).color(if selected { C_TEXT } else { C_TEXT2 }).strong());
                                    ui.label(egui::RichText::new(m.filename).monospace().size(10.0).color(C_MUTED));
                                    ui.horizontal(|ui| {
                                        ui.label(egui::RichText::new(m.quality).monospace().size(9.0).color(C_MUTED));
                                        ui.label(egui::RichText::new(m.container).monospace().size(9.0).color(C_MUTED));
                                        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                                            ui.label(egui::RichText::new(m.size).monospace().size(10.0).color(C_DIM));
                                        });
                                    });
                                });
                                let r = ui.interact(ui.min_rect(), ui.make_persistent_id(m.id), egui::Sense::click());
                                if r.clicked() { self.selected = i; }
                                ui.add_space(2.0);
                            }
                        });
                    });
                });
        }

        // Floating inspector overlay
        if self.inspector_open {
            let screen = ctx.viewport_rect();
            egui::Area::new("inspector_overlay".into())
                .fixed_pos(egui::pos2(screen.width() - 296.0, 44.0))
                .show(&ctx, |ui| {
                    egui::Frame::new().fill(C_SIDEBAR).stroke(egui::Stroke::new(1.0, C_BORDER2)).corner_radius(8.0).show(ui, |ui| {
                        ui.set_width(280.0);
                        ui.set_max_height(520.0);
                        ui.horizontal(|ui| {
                            ui.label(egui::RichText::new("MEDIA INSPECTOR").monospace().size(11.0).strong().color(C_DIM));
                            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                                if ui.small_button("✕").clicked() { self.inspector_open = false; }
                            });
                        });
                        let m = &MEDIA[self.selected];
                        ui.label(egui::RichText::new(m.title).strong().color(C_TEXT));
                        ui.label(egui::RichText::new(m.subtitle).monospace().size(11.0).color(C_MUTED));
                        ui.add_space(4.0);
                        for (k, v) in [("Codec", m.codec), ("Resolution", m.resolution), ("Container", m.container),
                                        ("Bitrate", m.bitrate), ("Framerate", m.fps), ("Audio", m.audio_codec)] {
                            ui.horizontal(|ui| {
                                ui.label(egui::RichText::new(format!("{k}:")).monospace().size(11.0).color(C_DIM));
                                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                                    ui.label(egui::RichText::new(v).monospace().size(11.0).color(C_TEXT2).strong());
                                });
                            });
                        }
                        ui.add_space(6.0);
                        if ui.add(egui::Button::new("Copy Local Proxy Link").fill(C_RAISED).min_size(egui::vec2(ui.available_width(), 26.0))).clicked() {
                            ctx.copy_text(format!("http://127.0.0.1:54941/telegram/{}", m.id));
                        }
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
        egui_material_icons::initialize(&cc.egui_ctx);
        egui_extras::install_image_loaders(&cc.egui_ctx);
        Ok(Box::new(App2::default()))
    }))
}
