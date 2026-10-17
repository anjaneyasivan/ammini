use eframe::{App, Frame, NativeOptions, egui};
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use egui_material_icons::{MaterialIcon, icons::*};

struct HelloWorldApp {
    selected: usize,
    search_open: bool,
    search_text: String,
    icon_cache: Arc<Mutex<IconCache>>,
}

impl Default for HelloWorldApp {
    fn default() -> Self {
        Self {
            selected: 0,
            search_open: false,
            search_text: String::new(),
            icon_cache: IconCache::new_shared(),
        }
    }
}

/// Stub sidebar entries: (icon, label).
const SIDEBAR: &[(MaterialIcon, &str)] = &[
    (ICON_HOME, "Home"),
    (ICON_SEARCH, "Search"),
    (ICON_SETTINGS, "Settings"),
    (ICON_INFO, "Info"),
];

const LOREM: &str = "Lorem ipsum dolor sit amet, consectetur adipiscing elit. \
    Sed do eiusmod tempor incididunt ut labore et dolore magna aliqua. Ut enim ad \
    minim veniam, quis nostrud exercitation ullamco laboris nisi ut aliquip ex ea \
    commodo consequat. Duis aute irure dolor in reprehenderit in voluptate velit \
    esse cillum dolore eu fugiat nulla pariatur.";

impl App for HelloWorldApp {
    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut Frame) {
        let ctx = ui.ctx().clone();

        // Top search bar.
        egui::Panel::top("topbar").show(ui, |ui| {
            ui.horizontal(|ui| {
                ui.label(ICON_SEARCH.rich_text().size(18.0));
                let resp = ui.add(
                    egui::TextEdit::singleline(&mut self.search_text)
                        .hint_text("Search…")
                        .desired_width(240.0),
                );
                // Open the overlay when the search bar is clicked/focused.
                if resp.gained_focus() {
                    self.search_open = true;
                }
            });
        });

        // Vertical icon sidebar on the left.
        egui::Panel::left("sidebar")
            .resizable(false)
            .default_size(56.0)
            .show(ui, |ui| {
                ui.vertical_centered(|ui| {
                    for (i, (icon, label)) in SIDEBAR.iter().enumerate() {
                        let selected = self.selected == i;
                        let btn = ui.selectable_label(selected, icon.rich_text().size(22.0));
                        if btn.clicked() {
                            self.selected = i;
                            if *label == "Search" {
                                self.search_open = true;
                            }
                        }
                        btn.on_hover_text(*label);
                    }
                    ui.add_space(8.0);
                    // Custom icon component (arcticons:telegram) in the sidebar.
                    if iconify_icon(ui, &self.icon_cache, &ctx, "arcticons", "telegram", "#ffffff", 32) {
                        println!("sidebar: arcticons:telegram clicked");
                    }
                });
            });

        egui::CentralPanel::default().show(ui, |ui| {
            // Video player placeholder occupying the main content area.
            let (rect, _response) = ui.allocate_exact_size(
                ui.available_size(),
                egui::Sense::click(),
            );
            ui.painter().rect_filled(rect, 8.0, egui::Color32::from_rgb(20, 20, 24));
            ui.painter().text(
                rect.center(),
                egui::Align2::CENTER_CENTER,
                "▶  Video player placeholder",
                egui::FontId::proportional(24.0),
                egui::Color32::from_gray(180),
            );

            ui.add_space(12.0);
            // Custom icon component: arcticons:telegram, white, with click handler.
            if iconify_icon(ui, &self.icon_cache, &ctx, "arcticons", "telegram", "#ffffff", 32) {
                println!("arcticons:telegram clicked");
            }
        });

        // Overlay panel sliding in from the left with lorem ipsum content.
        if self.search_open {
            // Close on Escape.
            if ctx.input(|i| i.key_pressed(egui::Key::Escape)) {
                self.search_open = false;
            }
            egui::Area::new("search_overlay".into())
                .fixed_pos(egui::pos2(64.0, 40.0))
                .show(&ctx, |ui| {
                    egui::Frame::window(ui.style())
                        .fill(egui::Color32::from_rgb(30, 30, 36))
                        .show(ui, |ui| {
                            ui.set_min_width(280.0);
                            ui.set_max_width(320.0);
                            ui.horizontal(|ui| {
                                ui.heading("Search");
                                ui.with_layout(
                                    egui::Layout::right_to_left(egui::Align::Center),
                                    |ui| {
                                        if ui.small_button("✕").clicked() {
                                            self.search_open = false;
                                        }
                                    },
                                );
                            });
                            ui.separator();
                            ui.add_space(4.0);
                            ui.label(LOREM);
                            ui.add_space(4.0);
                            ui.label(LOREM);
                        });
                });
        }
    }
}



// ---------------------------------------------------------------------------
// Async disk-cached Iconify icon component
// ---------------------------------------------------------------------------

#[derive(Clone)]
#[allow(dead_code)]
enum IconState {
    Loading,
    Ready(Vec<u8>),
    Failed(String),
}

struct IconCache {
    dir: PathBuf,
    entries: HashMap<String, IconState>,
}

impl IconCache {
    fn new() -> Self {
        let dir = dirs::cache_dir()
            .unwrap_or_else(std::env::temp_dir)
            .join("newplayer_icon_cache");
        let _ = std::fs::create_dir_all(&dir);
        Self { dir, entries: HashMap::new() }
    }

    fn new_shared() -> Arc<Mutex<Self>> {
        Arc::new(Mutex::new(Self::new()))
    }

    fn key(prefix: &str, name: &str, color: &str, size: u32) -> String {
        format!("{prefix}_{name}_{color}_{size}")
            .replace(['/', '\\', ':', '*', '?', '"', '<', '>', '|', '#', '%'], "_")
    }

    fn cache_path(&self, key: &str) -> PathBuf {
        self.dir.join(format!("{key}.svg"))
    }

    fn state(&mut self, ctx: &egui::Context, prefix: &str, name: &str, color: &str, size: u32) -> IconState {
        let key = Self::key(prefix, name, color, size);
        if let Some(st) = self.entries.get(&key).cloned() {
            match st {
                IconState::Ready(b) => return IconState::Ready(b),
                IconState::Failed(e) => return IconState::Failed(e),
                IconState::Loading => {
                    let path = self.cache_path(&key);
                    if let Ok(bytes) = std::fs::read(&path) {
                        if !bytes.is_empty() {
                            self.entries.insert(key, IconState::Ready(bytes.clone()));
                            return IconState::Ready(bytes);
                        }
                    }
                    return IconState::Loading;
                }
            }
        }
        let path = self.cache_path(&key);
        if let Ok(bytes) = std::fs::read(&path) {
            if !bytes.is_empty() {
                self.entries.insert(key.clone(), IconState::Ready(bytes.clone()));
                return IconState::Ready(bytes);
            }
        }
        self.entries.insert(key.clone(), IconState::Loading);
        let ctx = ctx.clone();
        let cache_dir = self.dir.clone();
        let url = format!(
            "https://api.iconify.design/{prefix}/{name}.svg?color={color}&height={size}",
            color = color.replace('#', "%23")
        );
        let key2 = key.clone();
        std::thread::spawn(move || {
            let bytes = curl_get(&url);
            let path = cache_dir.join(format!("{key2}.svg"));
            if let Ok(b) = bytes {
                let _ = std::fs::write(&path, &b);
            }
            ctx.request_repaint();
        });
        IconState::Loading
    }
}

fn curl_get(url: &str) -> Result<Vec<u8>, String> {
    let output = std::process::Command::new("curl")
        .args(["-sS", "-L", url])
        .output();
    match output {
        Ok(o) if o.status.success() && !o.stdout.is_empty() => Ok(o.stdout),
        Ok(o) => Err(String::from_utf8_lossy(&o.stderr).to_string()),
        Err(e) => Err(e.to_string()),
    }
}

fn iconify_icon(
    ui: &mut egui::Ui,
    cache: &Arc<Mutex<IconCache>>,
    ctx: &egui::Context,
    prefix: &str,
    name: &str,
    color: &str,
    size: u32,
) -> bool {
    let state = {
        let mut c = cache.lock().unwrap();
        c.state(ctx, prefix, name, color, size)
    };
    let desired = egui::vec2(size as f32, size as f32);
    let mut clicked = false;
    match state {
        IconState::Ready(bytes) => {
            let image = egui::Image::from_bytes(format!("{prefix}_{name}.svg"), bytes)
                .max_size(desired)
                .fit_to_original_size(1.0);
            let btn = ui.add(
                egui::Button::image(image)
                    .frame(false)
                    .min_size(desired + egui::vec2(8.0, 8.0)),
            );
            clicked = btn.clicked();
        }
        IconState::Failed(_) => {
            let btn = ui.add(egui::Button::new("!").min_size(desired + egui::vec2(8.0, 8.0)));
            clicked = btn.clicked();
            btn.on_hover_text("Failed to load icon");
        }
        IconState::Loading => {
            let (rect, _) = ui.allocate_exact_size(
                desired + egui::vec2(8.0, 8.0),
                egui::Sense::click(),
            );
            ui.painter().text(
                rect.center(),
                egui::Align2::CENTER_CENTER,
                "…",
                egui::FontId::proportional(16.0),
                egui::Color32::GRAY,
            );
        }
    }
    clicked
}

fn main() -> eframe::Result<()> {
    let options = NativeOptions {
        renderer: eframe::Renderer::Glow,
        ..Default::default()
    };
    eframe::run_native(
        "newplayer",
        options,
        Box::new(|cc| {
            egui_material_icons::initialize(&cc.egui_ctx);
            egui_extras::install_image_loaders(&cc.egui_ctx);
            Ok(Box::new(HelloWorldApp::default()))
        }),
    )
}
