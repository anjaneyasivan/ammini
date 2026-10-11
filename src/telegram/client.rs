// Re-export grammers types used in our public API
pub use grammers_client::SignInError;
pub use grammers_client::client::LoginToken;
pub use grammers_client::client::{DialogIter, MessageIter};
pub use grammers_client::media::Document;
pub use grammers_session::types::PeerRef;

use anyhow::Result;
use grammers_client::client::Client;
use grammers_client::peer::User;
use grammers_client::sender::SenderPool;
use grammers_session::storages::SqliteSession;
use grammers_session::updates::UpdatesLike;
use std::collections::HashMap;
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::sync::Mutex;
use tokio::sync::mpsc::UnboundedReceiver;

use crate::telegram::config::TelegramConfig;
use crate::telegram::media_meta::MediaMeta;

/// How many dialogs to fetch per page.
pub const DIALOG_PAGE_SIZE: usize = 20;
/// How many messages to fetch per page.
pub const MESSAGE_PAGE_SIZE: usize = 50;

/// The sender pool's raw update channel, handed to the realtime listener.
pub type UpdatesReceiver = UnboundedReceiver<UpdatesLike>;

/// File extensions treated as playable video. Some files (e.g. `.mkv` sent as a plain
/// attachment) carry no Telegram video attributes, so we fall back on the file name.
const VIDEO_EXTENSIONS: &[&str] = &["mp4", "mkv", "avi", "mov", "webm", "ogv", "flv"];

/// How long a refetched document is reused for other block downloads of the same video
/// before another expiry would be refetched. Covers a streaming window (4 prefetched
/// blocks plus the one in flight) comfortably.
const REFRESH_REUSE_AFTER: Duration = Duration::from_secs(60);

/// Wrapper around the grammers Client providing high-level async operations.
pub struct TelegramClient {
    client: Client,
    api_hash: String,
}

impl TelegramClient {
    pub async fn connect(
        config: &TelegramConfig,
        session: Arc<SqliteSession>,
    ) -> Result<(Self, UpdatesReceiver)> {
        tracing::debug!("tg: connecting (api_id={})", config.api_id);
        let SenderPool {
            runner,
            handle,
            updates,
        } = SenderPool::new(session, config.api_id);
        let client = Client::new(handle);
        tokio::spawn(runner.run());
        tracing::debug!("tg: connected, runner spawned");
        Ok((
            Self {
                client,
                api_hash: config.api_hash.clone(),
            },
            updates,
        ))
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

    // grammers' SignInError is a large enum; boxing it would ripple through the state
    // machine for no gain on this infrequent path.
    #[allow(clippy::result_large_err)]
    pub async fn sign_in(&self, token: &LoginToken, code: &str) -> Result<User, SignInError> {
        tracing::debug!("tg: signing in with code");
        let result = self.client.sign_in(token, code).await;
        match &result {
            Ok(user) => tracing::debug!("tg: sign_in ok (user={})", user.id().bare_id_unchecked()),
            Err(e) => tracing::debug!("tg: sign_in failed: {:?}", e),
        }
        result
    }

    #[allow(clippy::result_large_err)]
    pub async fn check_password(
        &self,
        token: grammers_client::client::PasswordToken,
        password: Vec<u8>,
    ) -> Result<User, SignInError> {
        tracing::debug!("tg: submitting 2FA password");
        let result = self.client.check_password(token, password).await;
        match &result {
            Ok(user) => tracing::debug!(
                "tg: check_password ok (user={})",
                user.id().bare_id_unchecked()
            ),
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

    /// Revoke the session server-side (best effort — the local file is deleted
    /// separately by `session::delete_session`).
    pub async fn sign_out(&self) -> Result<(), anyhow::Error> {
        self.client
            .sign_out()
            .await
            .map(|_| ())
            .map_err(|e| anyhow::anyhow!("telegram sign-out failed: {e}"))
    }

    pub fn clone_inner(&self) -> Client {
        self.client.clone()
    }

    pub fn inner(&self) -> &Client {
        &self.client
    }
}

/// Abstraction over fetching one 512 KiB block of a Telegram document, so the proxy can
/// be served from synthetic sources in offline tests instead of a live client.
/// (Methods return boxed futures rather than RPITIT so the trait stays `dyn`-compatible.)
pub trait VideoSource: Send + Sync {
    /// Download the whole block `block` of `video` (the final block of the file may be
    /// shorter if the file size is not a multiple of the block size).
    fn download_block<'a>(
        &'a self,
        video: &'a VideoDownloadInfo,
        block: u64,
    ) -> std::pin::Pin<
        Box<dyn std::future::Future<Output = Result<Vec<u8>, anyhow::Error>> + Send + 'a>,
    >;
}

/// Real [`VideoSource`] backed by a grammers client's chunked download iterator.
///
/// Holds a handle to the video registry so a document refreshed mid-download (see
/// [`download_block`]) is written back for subsequent range requests. `refreshed`
/// remembers recently refreshed documents and `refresh_gate` serializes the refetches, so
/// the burst of blocks that fails together on one expired reference costs a single
/// `get_messages_by_id` rather than one per block.
pub struct GrammersVideoSource {
    pub client: Client,
    pub registry: Arc<Mutex<HashMap<i32, VideoDownloadInfo>>>,
    /// msg_id -> (when it was refreshed, the fresh document).
    refreshed: Arc<Mutex<HashMap<i32, (Instant, Document)>>>,
    /// Held across a refetch so only one `get_messages_by_id` runs at a time.
    refresh_gate: Arc<Mutex<()>>,
}

impl GrammersVideoSource {
    pub fn new(client: Client, registry: Arc<Mutex<HashMap<i32, VideoDownloadInfo>>>) -> Self {
        Self {
            client,
            registry,
            refreshed: Arc::new(Mutex::new(HashMap::new())),
            refresh_gate: Arc::new(Mutex::new(())),
        }
    }
}

impl VideoSource for GrammersVideoSource {
    fn download_block<'a>(
        &'a self,
        video: &'a VideoDownloadInfo,
        block: u64,
    ) -> std::pin::Pin<
        Box<dyn std::future::Future<Output = Result<Vec<u8>, anyhow::Error>> + Send + 'a>,
    > {
        Box::pin(async move { download_block(self, video, block).await })
    }
}

/// Download a whole block from Telegram via the client's chunked iterator. `skip_chunks`
/// positions the iterator exactly at block `block`.
///
/// Telegram expires a document's `file_reference` after a few hours, and grammers-client
/// does not refresh it (`files.rs`: "TODO handle maybe FILEREF_UPGRADE_NEEDED"), so a
/// long-lived registry entry eventually fails every request with
/// `FILE_REFERENCE_EXPIRED`/`FILE_REFERENCE_INVALID`. When that happens the document is
/// re-fetched from the message (yielding a fresh reference) and the block is retried once.
pub async fn download_block(
    source: &GrammersVideoSource,
    video: &VideoDownloadInfo,
    block: u64,
) -> Result<Vec<u8>, anyhow::Error> {
    match download_block_once(&source.client, &video.document, block).await {
        Ok(chunk) => Ok(chunk),
        Err(e) if e.is::<ExpiredFileReference>() => {
            tracing::warn!(
                "telegram: msg_id={} block {block} hit an expired file reference ({e:#}); \
                 refetching the document",
                video.msg_id
            );
            match refreshed_document(source, video).await {
                Ok(document) => download_block_once(&source.client, &document, block)
                    .await
                    .map_err(|retry| {
                        anyhow::anyhow!("block {block}: retry after refetch failed: {retry}")
                    }),
                Err(refresh_err) => Err(anyhow::anyhow!(
                    "block {block}: could not refresh document for msg_id={}: {refresh_err}",
                    video.msg_id
                )),
            }
        }
        Err(e) => Err(e),
    }
}

/// A document refetched recently enough to reuse, fetching a new one only if this is the
/// first request since the expiry. Concurrent callers queue on the gate and the loser of
/// the race finds the winner's document waiting, so one expiry costs one RPC.
async fn refreshed_document(
    source: &GrammersVideoSource,
    video: &VideoDownloadInfo,
) -> Result<Document, anyhow::Error> {
    if let Some(document) = recently_refreshed(source, video.msg_id).await {
        return Ok(document);
    }
    let _gate = source.refresh_gate.lock().await;
    // Someone may have refreshed while we waited for the gate.
    if let Some(document) = recently_refreshed(source, video.msg_id).await {
        return Ok(document);
    }

    let fresh = fetch_video_info(&source.client, video.peer, video.msg_id).await?;
    tracing::info!("telegram: msg_id={} document refetched", video.msg_id);
    source
        .registry
        .lock()
        .await
        .insert(video.msg_id, fresh.clone());
    source
        .refreshed
        .lock()
        .await
        .insert(video.msg_id, (Instant::now(), fresh.document.clone()));
    Ok(fresh.document)
}

/// The cached refresh for `msg_id`, if one happened inside [`REFRESH_REUSE_AFTER`].
///
/// Without this window, blocks streaming concurrently would each refetch: they hold the
/// stale document and all fail at once, but only one refetch is ever needed.
async fn recently_refreshed(source: &GrammersVideoSource, msg_id: i32) -> Option<Document> {
    let mut refreshed = source.refreshed.lock().await;
    match refreshed.get(&msg_id) {
        Some((at, document)) if at.elapsed() < REFRESH_REUSE_AFTER => Some(document.clone()),
        Some(_) => {
            refreshed.remove(&msg_id);
            None
        }
        None => None,
    }
}

/// Download a whole block from Telegram via the client's chunked iterator. `skip_chunks`
/// positions the iterator exactly at block `block`.
async fn download_block_once(
    client: &Client,
    document: &Document,
    block: u64,
) -> Result<Vec<u8>, anyhow::Error> {
    let skip = u32::try_from(block).map_err(|_| anyhow::anyhow!("block {block} out of range"))?;
    let mut iter = client
        .iter_download(document)
        .chunk_size(crate::telegram::cache::BLOCK_SIZE as i32)
        .skip_chunks(skip as i32);
    match iter.next().await {
        Ok(Some(chunk)) => Ok(chunk),
        Ok(None) => Err(anyhow::anyhow!(
            "block {block}: telegram download returned no data"
        )),
        Err(e) => {
            // File-reference expiry is an RPC error we can recover from by refetching the
            // message; tag it so the caller knows to try exactly once more.
            let reason = match &e {
                grammers_client::InvocationError::Rpc(rpc)
                    if rpc.code == 400 && rpc.name.starts_with("FILE_REFERENCE") =>
                {
                    return Err(anyhow::Error::new(ExpiredFileReference)
                        .context(format!("block {block}: telegram download failed: {e}")));
                }
                _ => format!("block {block}: telegram download failed: {e}"),
            };
            Err(anyhow::anyhow!(reason))
        }
    }
}

/// Marker for a Telegram `FILE_REFERENCE_*` RPC error, which a document refetch fixes.
/// Typed so callers can match on it without string-matching the error text.
#[derive(Debug)]
struct ExpiredFileReference;

impl std::fmt::Display for ExpiredFileReference {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "expired file reference")
    }
}

impl std::error::Error for ExpiredFileReference {}

/// Fetch a single message by id and build its [`VideoDownloadInfo`]. Used to replay a
/// recently played video after its registry entry was cleared (chat switch/sign-out),
/// and to refresh a document whose `file_reference` expired mid-playback.
/// `peer` must carry the session's authority for the chat (e.g. the `PeerRef` stored
/// when the video was first played) — an ambient/default-authority ref is rejected for
/// channels.
pub async fn fetch_video_info(
    client: &Client,
    peer: PeerRef,
    msg_id: i32,
) -> Result<VideoDownloadInfo, anyhow::Error> {
    let chat_id = peer.id.bot_api_dialog_id().unwrap_or(0);
    let mut fetched = client
        .get_messages_by_id(peer, &[msg_id])
        .await
        .map_err(|e| anyhow::anyhow!("failed to fetch message {msg_id}: {e}"))?;
    let msg = fetched
        .pop()
        .flatten()
        .ok_or_else(|| anyhow::anyhow!("message {msg_id} not found"))?;
    let (is_video, video) = extract_video(&msg, chat_id, Some(peer));
    let video = video.ok_or_else(|| anyhow::anyhow!("message {msg_id} contains no video"))?;
    if !is_video || video.size == 0 {
        return Err(anyhow::anyhow!(
            "message {msg_id} contains no playable video"
        ));
    }
    Ok(video)
}

/// The signed-in account's own bare user id, for marking own messages. Best effort:
/// returns None when the session isn't authorized or the request fails (own messages
/// then render as neutral until the next sign-in).
pub async fn self_user_id(client: &TelegramClient) -> Option<i64> {
    let request = grammers_client::tl::functions::users::GetUsers {
        id: vec![grammers_client::tl::enums::InputUser::UserSelf],
    };
    match client.inner().invoke(&request).await.map(
        |users: Vec<grammers_client::tl::enums::User>| {
            users.into_iter().find_map(|user| match user {
                grammers_client::tl::enums::User::User(u) => Some(u.id),
                grammers_client::tl::enums::User::Empty(_) => None,
            })
        },
    ) {
        Ok(id) => {
            tracing::debug!("tg: self user id = {id:?}");
            id
        }
        Err(e) => {
            tracing::warn!("tg: failed to fetch self user id: {e}");
            None
        }
    }
}

/// Display name (full name, falling back to @username) and phone number of a
/// Telegram account — for the telemetry identification log. `None` when the
/// account has neither a name nor a username.
pub fn user_identity(user: &User) -> Option<(String, Option<String>)> {
    let full_name = user.full_name();
    let name = if full_name.trim().is_empty() {
        user.username()?.to_owned()
    } else {
        full_name
    };
    Some((name, user.phone().map(str::to_owned)))
}

/// Fetch the signed-in account's identity (display name + phone) for telemetry.
/// Used when a saved session resumes at launch, where no `User` object is around.
pub async fn self_user_identity(client: &TelegramClient) -> Option<(String, Option<String>)> {
    let request = grammers_client::tl::functions::users::GetUsers {
        id: vec![grammers_client::tl::enums::InputUser::UserSelf],
    };
    match client.inner().invoke(&request).await {
        Ok(users) => users
            .into_iter()
            .find_map(|raw| user_identity(&User::from_raw(client.inner(), raw))),
        Err(e) => {
            tracing::warn!("tg: failed to fetch self user identity: {e}");
            None
        }
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

    tracing::debug!(
        "tg: fetched {} dialogs (has_more={})",
        dialogs.len(),
        has_more
    );
    Ok((dialogs, has_more))
}

/// Fetch the next page of messages from the iterator.
/// Returns (messages, videos, has_more). Messages are newest-first.
/// `peer` is the chat being paged; it is attached to any videos found so their documents
/// can be refetched later (see [`VideoDownloadInfo::peer`]).
pub async fn next_messages_page(
    iter: &mut MessageIter,
    page_size: usize,
    chat_id: i64,
    self_user_id: Option<i64>,
    peer: PeerRef,
) -> Result<(Vec<MessageInfo>, Vec<VideoDownloadInfo>, bool)> {
    let mut messages = Vec::with_capacity(page_size);
    let mut videos = Vec::new();
    let mut has_more = true;

    for _ in 0..page_size {
        match iter.next().await {
            Ok(Some(msg)) => {
                let (info, video) = map_message(&msg, chat_id, self_user_id, Some(peer));
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

/// Map one incoming message for the UI. `peer` is the chat's `PeerRef` when the caller
/// has it (the open chat); a video is only reported when it is known, because the
/// registry entry needs the access hash to refetch the document later (see
/// [`VideoDownloadInfo::peer`]).
pub(crate) fn map_message(
    msg: &grammers_client::message::Message,
    chat_id: i64,
    self_user_id: Option<i64>,
    peer: Option<PeerRef>,
) -> (MessageInfo, Option<VideoDownloadInfo>) {
    let sender = msg
        .sender()
        .and_then(|p| p.name())
        .unwrap_or("")
        .to_string();
    let sender_is_self = self_user_id.is_some_and(|uid| {
        msg.sender()
            .and_then(|p| p.id().bare_id())
            .is_some_and(|sid| sid == uid)
    });
    let text = msg.text().to_string();
    let time = format_datetime(&msg.date());

    let (has_video, video) = extract_video(msg, chat_id, peer);

    // Parse the file name once here (background thread) so the UI never re-parses per
    // frame; the document supplies the size/duration the parser can't know.
    let media = video.as_ref().map(|v| {
        let mut media = MediaMeta::parse(v.document.name().unwrap_or("video"));
        media.size_bytes = v.document.size().map(|s| s as u64);
        media.duration_secs = v.document.duration();
        media
    });

    (
        MessageInfo {
            id: msg.id(),
            sender,
            sender_is_self,
            text,
            time,
            has_video,
            media,
        },
        video,
    )
}

/// Detect if a message contains a playable video document. The document is only returned
/// when `peer` (the chat's `PeerRef`, carrying the access hash) is known, since the
/// registry entry needs it to refetch the video later; the `bool` reports playability
/// either way, so the UI can still render the attachment.
fn extract_video(
    msg: &grammers_client::message::Message,
    chat_id: i64,
    peer: Option<PeerRef>,
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
        || doc.name().map(is_video_filename).unwrap_or(false)
        || doc
            .mime_type()
            .map(|m| m.to_lowercase().starts_with("video/"))
            .unwrap_or(false);
    if !is_video {
        return (false, None);
    }

    let size = doc.size().unwrap_or(0);
    let msg_id = msg.id();

    let video = peer.map(|peer| VideoDownloadInfo {
        msg_id,
        chat_id,
        peer,
        document: doc.clone(),
        size,
    });

    (true, video)
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
/// Find the first HEVC video in the user's chats, returning its dialog peer (needed to
/// refetch the message later) alongside the video info.
pub async fn find_first_hevc_video(
    client: &TelegramClient,
) -> Result<Option<(PeerRef, VideoDownloadInfo)>> {
    let mut dialogs_iter = client.iter_dialogs();
    loop {
        let (dialogs, has_more_dialogs) =
            next_dialogs_page(&mut dialogs_iter, DIALOG_PAGE_SIZE).await?;
        for dialog in dialogs {
            let peer = dialog.peer_ref;
            let chat_id = peer.id.bot_api_dialog_id().unwrap_or(0);
            let mut messages_iter = client.iter_messages(peer);
            loop {
                let (_, videos, has_more_messages) =
                    next_messages_page(&mut messages_iter, MESSAGE_PAGE_SIZE, chat_id, None, peer)
                        .await?;
                if let Some(video) = videos.into_iter().find(is_hevc_video) {
                    return Ok(Some((peer, video)));
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
    /// Whether the message was sent by the signed-in account (for bubble styling).
    pub sender_is_self: bool,
    pub text: String,
    pub time: String,
    pub has_video: bool,
    /// Presentation metadata for a video attachment (parsed file name + document size
    /// and duration). `None` for messages without a playable video.
    pub media: Option<MediaMeta>,
}

/// Everything needed to download a video from a Telegram message.
#[derive(Debug, Clone)]
pub struct VideoDownloadInfo {
    pub msg_id: i32,
    pub chat_id: i64,
    /// The chat this video came from, including the session's access hash. Kept so the
    /// document can be refetched when Telegram expires its `file_reference` (see
    /// [`download_block`]); a bare dialog id would be rejected for channels.
    pub peer: PeerRef,
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
        for name in [
            "doc.pdf",
            "archive.zip",
            "song.mp3",
            "no_extension",
            "video.mkv.bak",
        ] {
            assert!(
                !is_video_filename(name),
                "expected {name} not to be a video"
            );
        }
    }
}
