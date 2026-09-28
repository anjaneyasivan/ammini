use std::collections::HashMap;
use std::time::{Duration, Instant};

use statig::prelude::*;
use tracing::{error, info};

use crate::proxy::local_url_for;
use egui_sharkplayer::PlayerState;
use serde::{Deserialize, Serialize};

/// How often the playing state writes the current position to `resume`.
const RESUME_WRITE_INTERVAL: Duration = Duration::from_secs(1);
/// Positions below this are not worth resuming (near the start anyway).
const RESUME_MIN_SECONDS: f64 = 3.0;
/// Upper bound on tracked resume positions; the map is cleared when exceeded.
const RESUME_MAX_ENTRIES: usize = 200;
/// Cap for the recent-files list.
const RECENT_FILES_MAX: usize = 10;
/// Cap for the recent-Telegram list (same as recent files).
const RECENT_TELEGRAM_MAX: usize = 10;

/// A recently played Telegram video. The proxy URL is session-scoped (the port changes
/// every launch), so we persist the peer (holds the access hash) + message id and
/// refetch on replay.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct RecentTelegram {
    pub peer: grammers_session::types::PeerRef,
    pub chat_name: String,
    pub msg_id: i32,
    pub name: String,
}

#[derive(Default, Serialize, Deserialize)]
pub struct PersistentState {
    pub recent_files: Vec<String>,
    /// Playlist restored at startup; kept in step with the live playlist on every change.
    #[serde(default)]
    pub playlist: Vec<String>,
    /// Selected track within the persisted playlist.
    #[serde(default)]
    pub current_index: Option<usize>,
    /// Last volume (mpv units, 0–100), applied at startup.
    #[serde(default)]
    pub volume: Option<f64>,
    /// Per-file resume positions in seconds (local files only; proxy URLs are
    /// session-scoped and would be stale on the next launch).
    #[serde(default)]
    pub resume: HashMap<String, f64>,
    /// Recently played Telegram videos (most recent first), replayed by refetching the
    /// message via its chat + message ids.
    #[serde(default)]
    pub recent_telegram: Vec<RecentTelegram>,
}

pub struct PlayerFsm {
    pub player: PlayerState,
    pub proxy_url: Option<String>,
    pub playlist: Vec<String>,
    pub current_index: Option<usize>,
    pub show_playlist: bool,
    pub persistent: PersistentState,
    pub status: String,
    /// Resume position to seek to as soon as the current media finishes loading.
    pub resume_pending: Option<f64>,
    /// Last time `persistent.resume` was written, to throttle the per-frame polls.
    pub last_resume_write: Option<Instant>,
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
                    if let Some(pos) = self.resume_pending.take() {
                        match self.player.seek_to(pos) {
                            Ok(()) => info!("resumed at {pos:.1}s"),
                            Err(e) => error!("failed to seek to resume position {pos}: {e}"),
                        }
                    }
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
                    self.record_resume_position();
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
                self.current_index = Some(playlist_index_for(&mut self.playlist, path));

                self.persistent.recent_files.retain(|p| p != path);
                self.persistent.recent_files.insert(0, path.clone());
                self.persistent.recent_files.truncate(RECENT_FILES_MAX);

                sync_persistent_playlist(self);
                load_media(self, path.clone())
            }
            PlayerEvent::OpenUrl(url) => {
                if let Some(proxy_url) = &self.proxy_url {
                    let local = local_url_for(proxy_url, url);
                    self.current_index = Some(playlist_index_for(&mut self.playlist, &local));
                    sync_persistent_playlist(self);
                    load_media(self, local)
                } else {
                    error!("proxy unavailable, cannot open remote URL");
                    self.status = "Remote URL playback unavailable".into();
                    Transition(FsmState::error())
                }
            }
            PlayerEvent::OpenTelegramUrl(url) => {
                self.current_index = Some(playlist_index_for(&mut self.playlist, url));
                sync_persistent_playlist(self);
                load_media(self, url.clone())
            }
            PlayerEvent::AddToPlaylist(paths) => {
                for p in paths {
                    if !self.playlist.contains(p) {
                        self.playlist.push(p.clone());
                    }
                }
                sync_persistent_playlist(self);
                Handled
            }
            PlayerEvent::SelectTrack(index) => {
                if let Some(path) = self.playlist.get(*index).cloned() {
                    self.current_index = Some(*index);
                    sync_persistent_playlist(self);
                    load_media(self, path)
                } else {
                    Handled
                }
            }
            PlayerEvent::Next => {
                if let Some(i) = self.current_index
                    && i + 1 < self.playlist.len()
                {
                    let path = self.playlist[i + 1].clone();
                    self.current_index = Some(i + 1);
                    sync_persistent_playlist(self);
                    return load_media(self, path);
                }
                Handled
            }
            PlayerEvent::Previous => {
                if let Some(i) = self.current_index
                    && i > 0
                {
                    let path = self.playlist[i - 1].clone();
                    self.current_index = Some(i - 1);
                    sync_persistent_playlist(self);
                    return load_media(self, path);
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
    // Proxy URLs are session-scoped (the proxy port changes every launch), so only
    // local files can resume. Any local open — the Open File dialog, the playlist,
    // or a Recent-menu pick — resumes from the saved offset when one exists.
    fsm.resume_pending = resume_offset(&fsm.persistent.resume, &path);
    match fsm.player.load_file(&path) {
        Ok(()) => {
            info!("loading media: {path}");
            crate::telemetry::emit(crate::telemetry::TelemetryEvent::PlaybackStarted {
                source: playback_source(&path),
                file_name: file_name_of(&path),
            });
            fsm.status = format!("Loading: {path}");
            Transition(FsmState::loading())
        }
        Err(e) => {
            error!("failed to load {path}: {e}");
            crate::telemetry::emit(crate::telemetry::TelemetryEvent::PlaybackFailed {
                source: playback_source(&path),
                file_name: Some(file_name_of(&path)),
                error: e.to_string(),
            });
            fsm.status = format!("Error loading {path}: {e}");
            Transition(FsmState::error())
        }
    }
}

/// The offset to resume `path` from, if any, and only when the saved position is above
/// `RESUME_MIN_SECONDS` — anything smaller is not worth resuming.
fn resume_offset(resume: &HashMap<String, f64>, path: &str) -> Option<f64> {
    let key = resume_key(path)?;
    resume
        .get(&key)
        .copied()
        .filter(|&p| p > RESUME_MIN_SECONDS)
}

/// Stable key for the `resume` map. Local files key by their path; Telegram proxy URLs
/// are session-scoped (the proxy port changes every launch), so they key by the video's
/// message id as `telegram:{msg_id}` — the same per-message identity the proxy registry
/// and disk cache use. Any other remote URL has no stable identity and is not resumable.
fn resume_key(path: &str) -> Option<String> {
    if let Some(key) = telegram_resume_key(path) {
        return Some(key);
    }
    if path.starts_with("http") {
        None
    } else {
        Some(path.to_owned())
    }
}

/// The resume key for a Telegram video proxy URL, or `None` for anything else. Only local
/// proxy URLs count: a generic `/url?url=…` proxy could embed a `/telegram/…` path in its
/// query, which must not be mistaken for a Telegram video.
fn telegram_resume_key(path: &str) -> Option<String> {
    let (head, _query) = path.split_once('?').unwrap_or((path, ""));
    if !head.starts_with("http://127.0.0.1:") && !head.starts_with("http://localhost:") {
        return None;
    }
    crate::telegram::telegram_url_msg_id(head).map(|msg_id| format!("telegram:{msg_id}"))
}

/// Attribute a media path to the source it came from. Playlist replays re-enter
/// through `SelectTrack`/`Next`/`Previous`, so this is derived from the path shape
/// rather than the triggering event: proxy URLs carry the route they were built from.
fn playback_source(path: &str) -> crate::telemetry::PlaybackSource {
    if path.starts_with("http") && path.contains("/telegram/") {
        crate::telemetry::PlaybackSource::Telegram
    } else if path.starts_with("http") {
        crate::telemetry::PlaybackSource::Url
    } else {
        crate::telemetry::PlaybackSource::LocalFile
    }
}

/// Base name of a media path — file names only, never full paths (no PII from
/// directory structure).
fn file_name_of(path: &str) -> String {
    path.rsplit(['/', '\\']).next().unwrap_or(path).to_owned()
}

/// Keep `persistent.playlist`/`current_index` in step with the live playlist so the
/// queue is restored on the next launch.
fn sync_persistent_playlist(fsm: &mut PlayerFsm) {
    fsm.persistent.playlist = fsm.playlist.clone();
    fsm.persistent.current_index = fsm.current_index;
}

/// Append `path` to the playlist if absent and return its index. Reusing an existing
/// entry matters: a replayed Telegram URL (same proxy port) or a reopened file must
/// select *that* entry, not the last one, or the cache overlay and Next/Previous would
/// key off the wrong track.
fn playlist_index_for(playlist: &mut Vec<String>, path: &str) -> usize {
    if let Some(index) = playlist.iter().position(|p| p == path) {
        index
    } else {
        playlist.push(path.to_owned());
        playlist.len() - 1
    }
}

impl PlayerFsm {
    /// Write the current position to `persistent.resume` (throttled, local files only).
    fn record_resume_position(&mut self) {
        let now = Instant::now();
        let write_due = self
            .last_resume_write
            .is_none_or(|t| now.duration_since(t) >= RESUME_WRITE_INTERVAL);
        if !write_due {
            return;
        }
        self.last_resume_write = Some(now);
        self.record_current_position();
    }

    /// Write the current playback position to `persistent.resume` (unthrottled). The
    /// `playing` state goes through the throttled `record_resume_position`;
    /// `finalize_session` calls this directly on quit so the saved offset is never stale.
    /// Covers local files and Telegram videos (keyed by message id — see `resume_key`).
    fn record_current_position(&mut self) {
        let Some(path) = self
            .current_index
            .and_then(|i| self.playlist.get(i))
            .cloned()
        else {
            return;
        };
        let Some(key) = resume_key(&path) else {
            return;
        };
        let Ok(Some(pos)) = self.player.time_pos() else {
            return;
        };
        if pos <= RESUME_MIN_SECONDS {
            return;
        }
        if self.persistent.resume.len() >= RESUME_MAX_ENTRIES {
            self.persistent.resume.clear();
        }
        self.persistent.resume.insert(key, pos);
    }

    /// Finalize the session before the state is persisted (called from `App::save`,
    /// so it runs on Cmd+W/Cmd+Q/the close button and every autosave): record the
    /// current playing offset unthrottled and pin the current file in the recent list.
    /// The offset is recorded for local files and Telegram videos; only local files are
    /// added to the recent-files list (Telegram has its own).
    pub fn finalize_session(&mut self) {
        self.record_current_position();
        let Some(path) = self
            .current_index
            .and_then(|i| self.playlist.get(i))
            .cloned()
        else {
            return;
        };
        if path.starts_with("http") {
            return;
        }
        record_recent_file(&mut self.persistent.recent_files, &path);
    }
}

fn is_loaded(player: &PlayerState) -> bool {
    player.duration().ok().flatten().is_some() && player.time_pos().ok().flatten().is_some()
}

fn is_paused(player: &PlayerState) -> bool {
    player.paused().unwrap_or(true)
}

/// Human label for a media track (audio or subtitle): title, else language, else
/// "Track {n}". When both title and language exist, the language is only appended if the
/// title doesn't already mention it.
pub fn track_label(title: Option<String>, lang: Option<String>, index: usize) -> String {
    match (title, lang) {
        (Some(t), Some(l)) if !t.to_lowercase().contains(&l.to_lowercase()) => format!("{t} ({l})"),
        (Some(t), _) => t,
        (None, Some(l)) => l,
        (None, None) => format!("Track {}", index + 1),
    }
}

/// Move `path` to the front of the recent-files list (deduped), capped at
/// `RECENT_FILES_MAX` entries. Local files only; callers skip proxy/remote URLs.
pub fn record_recent_file(recent: &mut Vec<String>, path: &str) {
    recent.retain(|p| p != path);
    recent.insert(0, path.to_owned());
    recent.truncate(RECENT_FILES_MAX);
}

/// Move `entry` to the front of the recent-Telegram list (deduped by msg_id), capped at
/// `RECENT_TELEGRAM_MAX` entries.
pub fn record_recent_telegram(recent: &mut Vec<RecentTelegram>, entry: RecentTelegram) {
    recent.retain(|r| r.msg_id != entry.msg_id);
    recent.insert(0, entry);
    recent.truncate(RECENT_TELEGRAM_MAX);
}

#[cfg(test)]
mod tests {
    use super::{
        RecentTelegram, playlist_index_for, record_recent_file, record_recent_telegram, resume_key,
        resume_offset, track_label,
    };
    use grammers_session::types::{PeerAuth, PeerId, PeerRef};
    use std::collections::HashMap;

    fn entry(msg_id: i32, name: &str) -> RecentTelegram {
        RecentTelegram {
            peer: PeerRef {
                id: PeerId::user(1).unwrap(),
                auth: PeerAuth::default(),
            },
            chat_name: "Chat".into(),
            msg_id,
            name: name.into(),
        }
    }

    #[test]
    fn record_recent_file_dedupes_and_moves_to_front() {
        let mut recent = vec!["b.mp4".into(), "a.mp4".into()];
        record_recent_file(&mut recent, "b.mp4");
        assert_eq!(recent, vec!["b.mp4", "a.mp4"]);
        record_recent_file(&mut recent, "c.mp4");
        assert_eq!(recent, vec!["c.mp4", "b.mp4", "a.mp4"]);
    }

    #[test]
    fn playlist_index_for_reuses_an_existing_entry() {
        let mut playlist = vec!["a.mp4".to_owned(), "b.mp4".to_owned()];
        assert_eq!(playlist_index_for(&mut playlist, "b.mp4"), 1);
        assert_eq!(playlist, vec!["a.mp4", "b.mp4"], "no duplicate pushed");
        assert_eq!(playlist_index_for(&mut playlist, "c.mp4"), 2);
        assert_eq!(playlist, vec!["a.mp4", "b.mp4", "c.mp4"]);
    }

    #[test]
    fn record_recent_file_is_capped() {
        let mut recent = Vec::new();
        for i in 0..25 {
            record_recent_file(&mut recent, &format!("v{i}.mp4"));
        }
        assert_eq!(recent.len(), 10);
        assert_eq!(recent.first().unwrap(), "v24.mp4");
        assert_eq!(recent.last().unwrap(), "v15.mp4");
    }

    #[test]
    fn resume_offset_reads_saved_position() {
        let mut resume = HashMap::new();
        resume.insert("/videos/a.mp4".into(), 42.5);
        assert_eq!(resume_offset(&resume, "/videos/a.mp4"), Some(42.5));
        assert_eq!(resume_offset(&resume, "/videos/missing.mp4"), None);
    }

    #[test]
    fn resume_offset_skips_remote_and_tiny_positions() {
        let mut resume = HashMap::new();
        resume.insert("http://example.com/movie.mp4".into(), 9.0);
        resume.insert("/videos/b.mp4".into(), 2.0); // below RESUME_MIN_SECONDS
        assert_eq!(resume_offset(&resume, "http://example.com/movie.mp4"), None);
        assert_eq!(resume_offset(&resume, "/videos/b.mp4"), None);
    }

    #[test]
    fn telegram_resume_survives_proxy_port_change() {
        // Proxy URLs are session-scoped: the port changes every launch, so the key must
        // ignore it and follow the message id.
        let mut resume = HashMap::new();
        resume.insert("telegram:12345".into(), 612.0);
        assert_eq!(
            resume_offset(&resume, "http://127.0.0.1:51482/telegram/12345"),
            Some(612.0)
        );
        assert_eq!(
            resume_offset(&resume, "http://127.0.0.1:64353/telegram/12345"),
            Some(612.0)
        );
        // A different message is a different key.
        assert_eq!(
            resume_offset(&resume, "http://127.0.0.1:51482/telegram/999"),
            None
        );
    }

    #[test]
    fn resume_key_classifies_local_remote_and_telegram() {
        assert_eq!(resume_key("/videos/a.mp4"), Some("/videos/a.mp4".into()));
        assert_eq!(resume_key("http://example.com/a.mp4"), None);
        assert_eq!(
            resume_key("http://127.0.0.1:51482/telegram/12345"),
            Some("telegram:12345".into())
        );
        // A generic URL proxy whose query embeds a /telegram/ path is not a Telegram video.
        assert_eq!(
            resume_key("http://127.0.0.1:8080/url?url=http://127.0.0.1:9/telegram/5"),
            None
        );
    }

    #[test]
    fn track_label_prefers_title() {
        assert_eq!(
            track_label(Some("Commentary".into()), Some("de".into()), 0),
            "Commentary (de)"
        );
        assert_eq!(
            track_label(Some("English".into()), Some("English".into()), 1),
            "English"
        );
        assert_eq!(
            track_label(Some("Directors Cut".into()), None, 2),
            "Directors Cut"
        );
    }

    #[test]
    fn track_label_falls_back_to_lang_and_index() {
        assert_eq!(track_label(None, Some("ja".into()), 3), "ja");
        assert_eq!(track_label(None, None, 4), "Track 5");
    }

    #[test]
    fn recent_telegram_dedupes_and_moves_to_front() {
        let mut recent = Vec::new();
        record_recent_telegram(&mut recent, entry(1, "a.mp4"));
        record_recent_telegram(&mut recent, entry(2, "b.mp4"));
        record_recent_telegram(&mut recent, entry(1, "a.mp4"));
        let ids: Vec<i32> = recent.iter().map(|r| r.msg_id).collect();
        assert_eq!(ids, vec![1, 2]);
    }

    #[test]
    fn recent_telegram_is_capped() {
        let mut recent = Vec::new();
        for i in 0..25 {
            record_recent_telegram(&mut recent, entry(i, "v.mp4"));
        }
        assert_eq!(recent.len(), 10);
        assert_eq!(recent.first().unwrap().msg_id, 24);
        assert_eq!(recent.last().unwrap().msg_id, 15);
    }

    #[test]
    fn recent_telegram_survives_serde_round_trip() {
        let entry = entry(7, "clip.mkv");
        let json = serde_json::to_string(&entry).unwrap();
        let back: RecentTelegram = serde_json::from_str(&json).unwrap();
        assert_eq!(back.peer, entry.peer);
        assert_eq!(back.msg_id, 7);
        assert_eq!(back.name, "clip.mkv");
    }
}
