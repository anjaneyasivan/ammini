use statig::prelude::*;
use tracing::{error, info};

use crate::proxy::local_url_for;
use egui_sharkplayer::PlayerState;
use serde::{Deserialize, Serialize};

#[derive(Default, Serialize, Deserialize)]
pub struct PersistentState {
    pub recent_files: Vec<String>,
}

pub struct PlayerFsm {
    pub player: PlayerState,
    pub proxy_url: Option<String>,
    pub playlist: Vec<String>,
    pub current_index: Option<usize>,
    pub show_playlist: bool,
    pub persistent: PersistentState,
    pub status: String,
}

#[derive(Clone, Debug)]
pub enum PlayerEvent {
    OpenFile(String),
    OpenUrl(String),
    OpenTelegramUrl(String),
    AddToPlaylist(Vec<String>),
    SelectTrack(usize),
    Next,
    Previous,
    TogglePlaylist,
    Poll,
}

#[state_machine(state(name = "FsmState"), initial = "FsmState::idle()")]
impl PlayerFsm {
    #[state(superstate = "media")]
    fn idle(&mut self, event: &PlayerEvent) -> Outcome<FsmState> {
        match event {
            PlayerEvent::Poll => Handled,
            _ => Super,
        }
    }

    #[state(superstate = "media")]
    fn loading(&mut self, event: &PlayerEvent) -> Outcome<FsmState> {
        match event {
            PlayerEvent::Poll => {
                if is_loaded(&self.player) {
                    info!("media loaded; entering playing state");
                    self.status = "Playing".into();
                    Transition(FsmState::playing())
                } else {
                    Handled
                }
            }
            _ => Super,
        }
    }

    #[state(superstate = "media")]
    fn playing(&mut self, event: &PlayerEvent) -> Outcome<FsmState> {
        match event {
            PlayerEvent::Poll => {
                if is_paused(&self.player) {
                    info!("playback paused");
                    self.status = "Paused".into();
                    Transition(FsmState::paused())
                } else {
                    Handled
                }
            }
            _ => Super,
        }
    }

    #[state(superstate = "media")]
    fn paused(&mut self, event: &PlayerEvent) -> Outcome<FsmState> {
        match event {
            PlayerEvent::Poll => {
                if !is_paused(&self.player) {
                    info!("playback resumed");
                    self.status = "Playing".into();
                    Transition(FsmState::playing())
                } else {
                    Handled
                }
            }
            _ => Super,
        }
    }

    #[state(superstate = "media")]
    fn error(&mut self, event: &PlayerEvent) -> Outcome<FsmState> {
        match event {
            PlayerEvent::Poll => Handled,
            _ => Super,
        }
    }

    #[superstate]
    fn media(&mut self, event: &PlayerEvent) -> Outcome<FsmState> {
        match event {
            PlayerEvent::OpenFile(path) => {
                if !self.playlist.contains(path) {
                    self.playlist.push(path.clone());
                }
                self.current_index = self
                    .playlist
                    .iter()
                    .position(|p| p == path)
                    .or(Some(self.playlist.len() - 1));

                self.persistent.recent_files.retain(|p| p != path);
                self.persistent.recent_files.insert(0, path.clone());
                self.persistent.recent_files.truncate(10);

                load_media(self, path.clone())
            }
            PlayerEvent::OpenUrl(url) => {
                if let Some(proxy_url) = &self.proxy_url {
                    let local = local_url_for(proxy_url, url);
                    if !self.playlist.contains(&local) {
                        self.playlist.push(local.clone());
                    }
                    self.current_index = Some(self.playlist.len() - 1);
                    load_media(self, local)
                } else {
                    error!("proxy unavailable, cannot open remote URL");
                    self.status = "Remote URL playback unavailable".into();
                    Transition(FsmState::error())
                }
            }
            PlayerEvent::OpenTelegramUrl(url) => {
                if !self.playlist.contains(url) {
                    self.playlist.push(url.clone());
                }
                self.current_index = Some(self.playlist.len() - 1);
                load_media(self, url.clone())
            }
            PlayerEvent::AddToPlaylist(paths) => {
                for p in paths {
                    if !self.playlist.contains(p) {
                        self.playlist.push(p.clone());
                    }
                }
                Handled
            }
            PlayerEvent::SelectTrack(index) => {
                if let Some(path) = self.playlist.get(*index).cloned() {
                    self.current_index = Some(*index);
                    load_media(self, path)
                } else {
                    Handled
                }
            }
            PlayerEvent::Next => {
                if let Some(i) = self.current_index {
                    if i + 1 < self.playlist.len() {
                        let path = self.playlist[i + 1].clone();
                        self.current_index = Some(i + 1);
                        return load_media(self, path);
                    }
                }
                Handled
            }
            PlayerEvent::Previous => {
                if let Some(i) = self.current_index {
                    if i > 0 {
                        let path = self.playlist[i - 1].clone();
                        self.current_index = Some(i - 1);
                        return load_media(self, path);
                    }
                }
                Handled
            }
            PlayerEvent::TogglePlaylist => {
                self.show_playlist = !self.show_playlist;
                Handled
            }
            PlayerEvent::Poll => Super,
        }
    }
}

fn load_media(fsm: &mut PlayerFsm, path: String) -> Outcome<FsmState> {
    match fsm.player.load_file(&path) {
        Ok(()) => {
            info!("loading media: {path}");
            fsm.status = format!("Loading: {path}");
            Transition(FsmState::loading())
        }
        Err(e) => {
            error!("failed to load {path}: {e}");
            fsm.status = format!("Error loading {path}: {e}");
            Transition(FsmState::error())
        }
    }
}

fn is_loaded(player: &PlayerState) -> bool {
    player.duration().ok().flatten().is_some() && player.time_pos().ok().flatten().is_some()
}

fn is_paused(player: &PlayerState) -> bool {
    player.paused().unwrap_or(true)
}
