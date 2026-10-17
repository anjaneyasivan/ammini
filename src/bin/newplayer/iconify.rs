//! Iconify async SVG icon loader + cache.
use crate::theme::C_DIM;
use eframe::egui;
use std::collections::HashMap;
use std::sync::{Arc, Mutex};

#[derive(Clone)]
#[allow(dead_code)]
pub enum IconState {
    Loading,
    Ready(Vec<u8>),
    Failed(String),
}

pub struct IconCache {
    dir: std::path::PathBuf,
    entries: HashMap<String, IconState>,
}

impl IconCache {
    fn new() -> Self {
        let dir = dirs::cache_dir()
            .unwrap_or_else(std::env::temp_dir)
            .join("newplayer_icon_cache");
        let _ = std::fs::create_dir_all(&dir);
        Self {
            dir,
            entries: HashMap::new(),
        }
    }
    pub fn new_shared() -> Arc<Mutex<Self>> {
        Arc::new(Mutex::new(Self::new()))
    }
    fn key(prefix: &str, name: &str, color: &str, size: u32) -> String {
        format!("{prefix}_{name}_{color}_{size}").replace(
            ['/', '\\', ':', '*', '?', '"', '<', '>', '|', '#', '%'],
            "_",
        )
    }
    fn cache_path(&self, key: &str) -> std::path::PathBuf {
        self.dir.join(format!("{key}.svg"))
    }
    fn state(
        &mut self,
        ctx: &egui::Context,
        prefix: &str,
        name: &str,
        color: &str,
        size: u32,
    ) -> IconState {
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
                self.entries
                    .insert(key.clone(), IconState::Ready(bytes.clone()));
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
            let out = std::process::Command::new("curl")
                .args(["-sS", "-L", &url])
                .output();
            if let Ok(o) = out {
                if o.status.success() && !o.stdout.is_empty() {
                    let _ = std::fs::write(cache_dir.join(format!("{key2}.svg")), &o.stdout);
                }
            }
            ctx.request_repaint();
        });
        IconState::Loading
    }
}

pub fn iconify_icon(
    ui: &mut egui::Ui,
    cache: &Arc<Mutex<IconCache>>,
    ctx: &egui::Context,
    prefix: &str,
    name: &str,
    color: &str,
    size: u32,
    bg: Option<egui::Color32>,
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
            let mut btn = egui::Button::image(image).min_size(desired + egui::vec2(8.0, 8.0));
            if let Some(b) = bg {
                btn = btn.fill(b).corner_radius(egui::CornerRadius::same(100));
            } else {
                btn = btn.frame(false);
            }
            let btn = ui.add(btn);
            clicked = btn.clicked();
        }
        IconState::Failed(_) => {
            let btn = ui.add(egui::Button::new("!").min_size(desired + egui::vec2(8.0, 8.0)));
            clicked = btn.clicked();
        }
        IconState::Loading => {
            let (rect, _) =
                ui.allocate_exact_size(desired + egui::vec2(8.0, 8.0), egui::Sense::click());
            ui.painter().text(
                rect.center(),
                egui::Align2::CENTER_CENTER,
                "…",
                egui::FontId::proportional(16.0),
                C_DIM,
            );
        }
    }
    clicked
}
