// Re-export grammers types used in our public API
pub use grammers_client::client::LoginToken;
pub use grammers_client::client::{DialogIter, MessageIter};
pub use grammers_client::SignInError;
pub use grammers_client::media::Document;
pub use grammers_session::types::PeerRef;

use anyhow::Result;
use grammers_client::client::Client;
use grammers_client::peer::User;
use grammers_client::sender::SenderPool;
use grammers_session::storages::SqliteSession;
use std::sync::Arc;

use crate::telegram::config::TelegramConfig;

/// How many dialogs to fetch per page.
pub const DIALOG_PAGE_SIZE: usize = 20;
/// How many messages to fetch per page.
pub const MESSAGE_PAGE_SIZE: usize = 50;

/// File extensions treated as playable video. Some files (e.g. `.mkv` sent as a plain
/// attachment) carry no Telegram video attributes, so we fall back on the file name.
const VIDEO_EXTENSIONS: &[&str] = &["mp4", "mkv", "avi", "mov", "webm", "ogv", "flv"];

/// Wrapper around the grammers Client providing high-level async operations.
pub struct TelegramClient {
    client: Client,
    api_hash: String,
}

impl TelegramClient {
    pub async fn connect(config: &TelegramConfig, session: Arc<SqliteSession>) -> Result<Self> {
        tracing::debug!("tg: connecting (api_id={})", config.api_id);
        let pool = SenderPool::new(session, config.api_id);
        let client = Client::new(pool.handle);
        tokio::spawn(pool.runner.run());
        tracing::debug!("tg: connected, runner spawned");
        Ok(Self {
            client,
            api_hash: config.api_hash.clone(),
        })
    }

    pub async fn is_authorized(&self) -> Result<bool> {
        tracing::debug!("tg: checking authorization");
        let authorized = self
            .client
            .is_authorized()
            .await
            .map_err(|e| anyhow::anyhow!("Failed to check authorization status: {e:?}"))?;
        tracing::debug!("tg: authorized={}", authorized);
        Ok(authorized)
    }

    pub async fn request_login_code(&self, phone: &str) -> Result<LoginToken> {
        tracing::debug!("tg: requesting login code for {}", phone);
        let token = self
            .client
            .request_login_code(phone, &self.api_hash)
            .await
            .map_err(|e| anyhow::anyhow!("Failed to request login code: {e:?}"))?;
        tracing::debug!("tg: login code requested successfully");
        Ok(token)
    }

    pub async fn sign_in(&self, token: &LoginToken, code: &str) -> Result<User, SignInError> {
        tracing::debug!("tg: signing in with code");
        let result = self.client.sign_in(token, code).await;
        match &result {
            Ok(user) => tracing::debug!("tg: sign_in ok (user={})", user.id().bare_id_unchecked()),
            Err(e) => tracing::debug!("tg: sign_in failed: {:?}", e),
        }
        result
    }

    pub async fn check_password(
        &self,
        token: grammers_client::client::PasswordToken,
        password: Vec<u8>,
    ) -> Result<User, SignInError> {
        tracing::debug!("tg: submitting 2FA password");
        let result = self.client.check_password(token, password).await;
        match &result {
            Ok(user) => tracing::debug!("tg: check_password ok (user={})", user.id().bare_id_unchecked()),
            Err(e) => tracing::debug!("tg: check_password failed: {:?}", e),
        }
        result
    }

    pub fn iter_dialogs(&self) -> DialogIter {
        self.client.iter_dialogs()
    }

    pub fn iter_messages(&self, peer: PeerRef) -> MessageIter {
        self.client.iter_messages(peer)
    }

    pub fn clone_inner(&self) -> Client {
        self.client.clone()
    }

    pub fn inner(&self) -> &Client {
        &self.client
    }
}

/// Fetch the next page of dialogs from the iterator.
/// Returns (dialogs, has_more).
pub async fn next_dialogs_page(
    iter: &mut DialogIter,
    page_size: usize,
) -> Result<(Vec<DialogInfo>, bool)> {
    let mut dialogs = Vec::with_capacity(page_size);
    let mut has_more = true;

    for _ in 0..page_size {
        match iter.next().await {
            Ok(Some(dialog)) => dialogs.push(map_dialog(&dialog)),
            Ok(None) => {
                has_more = false;
                break;
            }
            Err(e) => return Err(anyhow::anyhow!("Failed to fetch dialog: {e}")),
        }
    }

    if dialogs.is_empty() {
        has_more = false;
    }

    tracing::debug!("tg: fetched {} dialogs (has_more={})", dialogs.len(), has_more);
    Ok((dialogs, has_more))
}

/// Fetch the next page of messages from the iterator.
/// Returns (messages, videos, has_more). Messages are newest-first.
pub async fn next_messages_page(
    iter: &mut MessageIter,
    page_size: usize,
    chat_id: i64,
) -> Result<(Vec<MessageInfo>, Vec<VideoDownloadInfo>, bool)> {
    let mut messages = Vec::with_capacity(page_size);
    let mut videos = Vec::new();
    let mut has_more = true;

    for _ in 0..page_size {
        match iter.next().await {
            Ok(Some(msg)) => {
                let (info, video) = map_message(&msg, chat_id);
                messages.push(info);
                if let Some(v) = video {
                    videos.push(v);
                }
            }
            Ok(None) => {
                has_more = false;
                break;
            }
            Err(e) => return Err(anyhow::anyhow!("Failed to fetch message: {e}")),
        }
    }

    if messages.is_empty() {
        has_more = false;
    }

    tracing::debug!(
        "tg: fetched {} messages, {} videos (has_more={})",
        messages.len(),
        videos.len(),
        has_more
    );
    Ok((messages, videos, has_more))
}

fn map_dialog(dialog: &grammers_client::peer::Dialog) -> DialogInfo {
    let peer_ref = dialog.peer_ref();
    let name = dialog.peer().name().unwrap_or("Unknown").to_string();
    let last_message = dialog.last_message.as_ref().map(|m| {
        let text = m.text();
        if text.is_empty() {
            "(media)".to_string()
        } else {
            text.to_string()
        }
    });
    DialogInfo {
        peer_ref,
        name,
        last_message,
    }
}

fn map_message(
    msg: &grammers_client::message::Message,
    chat_id: i64,
) -> (MessageInfo, Option<VideoDownloadInfo>) {
    let sender = msg
        .sender()
        .and_then(|p| p.name())
        .unwrap_or("")
        .to_string();
    let text = msg.text().to_string();
    let time = format_datetime(&msg.date());

    let (has_video, video) = extract_video(msg, chat_id);

    (
        MessageInfo {
            id: msg.id(),
            sender,
            text,
            time,
            has_video,
        },
        video,
    )
}

/// Detect if a message contains a playable video document.
fn extract_video(
    msg: &grammers_client::message::Message,
    chat_id: i64,
) -> (bool, Option<VideoDownloadInfo>) {
    let media = match msg.media() {
        Some(m) => m,
        None => return (false, None),
    };

    let doc = match &media {
        grammers_client::media::Media::Document(d) => d,
        _ => return (false, None),
    };

    // Video documents usually carry a duration or resolution attribute, but files sent as
    // plain attachments (e.g. `.mkv`) may not, so also accept video file names / MIME types.
    let is_video = doc.duration().is_some()
        || doc.resolution().is_some()
        || doc
            .name()
            .map(|n| is_video_filename(n))
            .unwrap_or(false)
        || doc
            .mime_type()
            .map(|m| m.to_lowercase().starts_with("video/"))
            .unwrap_or(false);
    if !is_video {
        return (false, None);
    }

    let size = doc.size().unwrap_or(0);
    let msg_id = msg.id();

    (
        true,
        Some(VideoDownloadInfo {
            msg_id,
            chat_id,
            document: doc.clone(),
            size,
        }),
    )
}

/// Returns true if the filename ends with a known video extension (case-insensitive).
fn is_video_filename(name: &str) -> bool {
    name.rsplit('.')
        .next()
        .map(|ext| VIDEO_EXTENSIONS.contains(&ext.to_lowercase().as_str()))
        .unwrap_or(false)
}

/// Returns true if the filename or MIME type suggests HEVC/H.265 content.
pub fn is_hevc_video(video: &VideoDownloadInfo) -> bool {
    let name_hevc = video
        .document
        .name()
        .map(|n| n.to_lowercase().contains("hevc"))
        .unwrap_or(false);
    let mime_hevc = video
        .document
        .mime_type()
        .map(|m| m.to_lowercase().contains("hevc") || m.to_lowercase().contains("h265"))
        .unwrap_or(false);
    name_hevc || mime_hevc
}

/// Search all dialogs for the first message containing an HEVC video.
/// Returns `Ok(None)` if authorized but no HEVC video was found.
pub async fn find_first_hevc_video(
    client: &TelegramClient,
) -> Result<Option<VideoDownloadInfo>> {
    let mut dialogs_iter = client.iter_dialogs();
    loop {
        let (dialogs, has_more_dialogs) =
            next_dialogs_page(&mut dialogs_iter, DIALOG_PAGE_SIZE).await?;
        for dialog in dialogs {
            let chat_id = dialog.peer_ref.id.bot_api_dialog_id().unwrap_or(0);
            let mut messages_iter = client.iter_messages(dialog.peer_ref);
            loop {
                let (_, videos, has_more_messages) =
                    next_messages_page(&mut messages_iter, MESSAGE_PAGE_SIZE, chat_id).await?;
                if let Some(video) = videos.into_iter().find(is_hevc_video) {
                    return Ok(Some(video));
                }
                if !has_more_messages {
                    break;
                }
            }
        }
        if !has_more_dialogs {
            break;
        }
    }
    Ok(None)
}

/// ponytail: formats as local time HH:MM, no date. Fine for a chat view;
/// add date separators if the thread spans multiple days.
fn format_datetime(dt: &chrono::DateTime<chrono::Utc>) -> String {
    use chrono::Local;
    dt.with_timezone(&Local).format("%H:%M").to_string()
}

/// Information about a dialog (chat).
#[derive(Debug, Clone)]
pub struct DialogInfo {
    pub peer_ref: PeerRef,
    pub name: String,
    pub last_message: Option<String>,
}

/// Information about a message.
#[derive(Debug, Clone)]
pub struct MessageInfo {
    pub id: i32,
    pub sender: String,
    pub text: String,
    pub time: String,
    pub has_video: bool,
}

/// Everything needed to download a video from a Telegram message.
#[derive(Debug, Clone)]
pub struct VideoDownloadInfo {
    pub msg_id: i32,
    pub chat_id: i64,
    pub document: Document,
    pub size: usize,
}

#[cfg(test)]
mod tests {
    use super::is_video_filename;

    #[test]
    fn recognizes_common_video_extensions() {
        for name in [
            "movie.mkv",
            "clip.MP4",
            "file.avi",
            "video.mov",
            "webm.webm",
            "thing.ogv",
            "x.flv",
        ] {
            assert!(is_video_filename(name), "expected {name} to be a video");
        }
    }

    #[test]
    fn rejects_non_video_filenames() {
        for name in ["doc.pdf", "archive.zip", "song.mp3", "no_extension", "video.mkv.bak"] {
            assert!(!is_video_filename(name), "expected {name} not to be a video");
        }
    }
}
