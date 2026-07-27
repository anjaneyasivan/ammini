use eframe::{App, Frame, NativeOptions, egui};
use egui_sharkplayer::{PlayerState, SharkPlayer};
use rfd::FileDialog;
use serde::{Deserialize, Serialize};

const APP_KEY: &str = "min_mpv_state";

#[derive(Default, Serialize, Deserialize)]
struct PersistentState {
    recent_files: Vec<String>,
}

struct MinMpvApp {
    player: PlayerState,
    playlist: Vec<String>,
    current_index: Option<usize>,
    show_playlist: bool,
    persistent: PersistentState,
    status: String,
}

impl MinMpvApp {
    fn new(
        cc: &eframe::CreationContext<'_>,
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

        Ok(Self {
            player,
            playlist: Vec::new(),
            current_index: None,
            show_playlist: false,
            persistent,
            status: String::from("Drop a video or press Cmd+O / Ctrl+O to open"),
        })
    }

    fn load_path(&mut self, path: String) {
        match self.player.load_file(&path) {
            Ok(()) => {
                self.status = format!("Loaded: {path}");
                self.add_recent(path.clone());
                if !self.playlist.contains(&path) {
                    self.playlist.push(path.clone());
                }
                self.current_index = self.playlist.iter().position(|p| p == &path);
            }
            Err(e) => self.status = format!("Error loading {path}: {e}"),
        }
    }

    fn add_recent(&mut self, path: String) {
        self.persistent.recent_files.retain(|p| p != &path);
        self.persistent.recent_files.insert(0, path);
        self.persistent.recent_files.truncate(10);
    }

    fn open_file(&mut self) {
        if let Some(path) = FileDialog::new()
            .add_filter(
                "Video files",
                &["mp4", "mkv", "avi", "mov", "webm", "ogv", "flv"],
            )
            .pick_file()
        {
            self.load_path(path.to_string_lossy().to_string());
        }
    }

    fn open_folder(&mut self) {
        if let Some(folder) = FileDialog::new().pick_folder() {
            let mut videos: Vec<String> = std::fs::read_dir(&folder)
                .ok()
                .map(|iter| {
                    iter.filter_map(|e| e.ok())
                        .filter(|e| {
                            e.path()
                                .extension()
                                .and_then(|ext| ext.to_str())
                                .map(|ext| {
                                    matches!(
                                        ext.to_lowercase().as_str(),
                                        "mp4" | "mkv" | "avi" | "mov" | "webm" | "ogv" | "flv"
                                    )
                                })
                                .unwrap_or(false)
                        })
                        .map(|e| e.path().to_string_lossy().to_string())
                        .collect()
                })
                .unwrap_or_default();
            videos.sort();

            if !videos.is_empty() {
                self.playlist.extend(videos.clone());
                self.load_path(videos[0].clone());
            } else {
                self.status = "No video files found in folder".into();
            }
        }
    }

    fn play_index(&mut self, index: usize) {
        if let Some(path) = self.playlist.get(index) {
            self.load_path(path.clone());
        }
    }

    fn next(&mut self) {
        if let Some(i) = self.current_index {
            if i + 1 < self.playlist.len() {
                self.play_index(i + 1);
            }
        }
    }

    fn previous(&mut self) {
        if let Some(i) = self.current_index {
            if i > 0 {
                self.play_index(i - 1);
            }
        }
    }

    fn handle_dropped_files(&mut self, ctx: &egui::Context) {
        let dropped: Vec<_> = ctx.input(|i| {
            i.raw
                .dropped_files
                .iter()
                .filter_map(|f| f.path.clone())
                .collect()
        });
        if let Some(first) = dropped.into_iter().next() {
            self.load_path(first.to_string_lossy().to_string());
        }
    }

    fn handle_shortcuts(&mut self, ctx: &egui::Context) {
        if ctx.input(|i| i.key_pressed(egui::Key::O) && i.modifiers.command) {
            self.open_file();
        }
        if ctx.input(|i| i.key_pressed(egui::Key::P) && i.modifiers.command) {
            self.show_playlist = !self.show_playlist;
        }
        if ctx.input(|i| i.key_pressed(egui::Key::ArrowRight) && i.modifiers.command) {
            self.next();
        }
        if ctx.input(|i| i.key_pressed(egui::Key::ArrowLeft) && i.modifiers.command) {
            self.previous();
        }
    }

    fn top_bar(&mut self, ui: &mut egui::Ui) {
        ui.horizontal(|ui| {
            if ui.button("Open File").clicked() {
                self.open_file();
            }
            if ui.button("Open Folder").clicked() {
                self.open_folder();
            }
            ui.separator();
            if ui.button("Prev").clicked() {
                self.previous();
            }
            if ui.button("Next").clicked() {
                self.next();
            }
            ui.separator();
            if ui.button("Playlist").clicked() {
                self.show_playlist = !self.show_playlist;
            }

            ui.menu_button("Recent", |ui| {
                if self.persistent.recent_files.is_empty() {
                    ui.weak("No recent files");
                } else {
                    for path in self.persistent.recent_files.clone() {
                        if ui.button(truncate_path(&path)).clicked() {
                            self.load_path(path);
                            ui.close();
                        }
                    }
                }
            });

            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                ui.add(egui::Label::new(&self.status).truncate());
            });
        });
    }

    fn playlist_panel(&mut self, ui: &mut egui::Ui) {
        ui.label("Playlist");
        ui.separator();
        egui::ScrollArea::vertical().show(ui, |ui| {
            let mut play_index = None;
            for (i, path) in self.playlist.iter().enumerate() {
                let selected = self.current_index == Some(i);
                if ui.selectable_label(selected, truncate_path(path)).clicked() {
                    play_index = Some(i);
                }
            }
            if let Some(i) = play_index {
                self.play_index(i);
            }
        });
    }
}

impl App for MinMpvApp {
    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut Frame) {
        let ctx = ui.ctx().clone();
        self.handle_dropped_files(&ctx);
        self.handle_shortcuts(&ctx);

        egui::Panel::top("top_bar").show(ui, |ui| {
            self.top_bar(ui);
        });

        if self.show_playlist {
            egui::Panel::left("playlist").show(ui, |ui| {
                self.playlist_panel(ui);
            });
        }

        egui::CentralPanel::default().show(ui, |ui| {
            ui.add(SharkPlayer::new(&mut self.player));
        });
    }

    fn on_exit(&mut self, _gl: Option<&eframe::glow::Context>) {
        self.player.destroy_gl_resources();
    }

    fn save(&mut self, storage: &mut dyn eframe::Storage) {
        if let Ok(s) = serde_json::to_string(&self.persistent) {
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

fn main() {
    let options = NativeOptions {
        viewport: egui::ViewportBuilder::default().with_inner_size([960.0, 640.0]),
        renderer: eframe::Renderer::Glow,
        ..Default::default()
    };

    eframe::run_native(
        "min-mpv",
        options,
        Box::new(|cc| {
            let app = MinMpvApp::new(cc)?;
            Ok(Box::new(app) as Box<dyn App>)
        }),
    )
    .unwrap();
}
