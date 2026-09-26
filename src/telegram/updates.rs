//! Realtime Telegram message updates.
//!
//! The sender pool delivers account updates through a single `updates` channel; the
//! client's [`UpdateStream`](grammers_client::client::UpdateStream) decodes them into
//! typed [`Update`]s. Ammini only cares about **new messages**: each is mapped to a
//! [`MessageInfo`], forwarded to the UI to append to the open chat, and used to refresh
//! the chat-list preview. Edits, deletions, callbacks, inline results and raw updates
//! are intentionally ignored — the UI does not surface them.
//!
//! This lives in its own task (and file) so the main background loop stays a plain
//! command dispatcher: the listener owns the update stream and the shared read-only
//! state it needs (own user id, selected chat id, the video registry).

use std::collections::HashMap;
use std::sync::Arc;
use std::sync::atomic::{AtomicI64, Ordering};
use std::time::Duration;

use grammers_client::client::UpdatesConfiguration;
use grammers_client::update::{Message, Update};
use tokio::sync::Mutex;
use tokio::sync::mpsc::UnboundedSender;
use tracing::{debug, warn};

use crate::telegram::UiMessage;
use crate::telegram::client::{self, TelegramClient, UpdatesReceiver, VideoDownloadInfo};

/// How many updates the client may buffer before dropping them. The library default is
/// 100; subscribed channels can burst, so keep headroom (the listener drains promptly).
const UPDATE_QUEUE_LIMIT: usize = 1000;
/// Backoff after an update-stream error, so a persistent failure can't spin the task.
const RETRY_BACKOFF: Duration = Duration::from_secs(1);
/// Sentinel for the shared atomics: 0 means "unknown"/"none".
const UNSET: i64 = 0;

/// Spawn the task that listens for new Telegram messages and forwards them to the UI.
///
/// `catch_up` is enabled so messages sent while the app was offline are delivered too
/// (Telegram only guarantees message updates; peer hashes needed to resolve channel gaps
/// are already cached by the startup dialog fetch).
pub fn spawn_listener(
    client: Arc<TelegramClient>,
    updates: UpdatesReceiver,
    self_user_id: Arc<AtomicI64>,
    selected_chat_id: Arc<AtomicI64>,
    video_registry: Arc<Mutex<HashMap<i32, VideoDownloadInfo>>>,
    ui_tx: UnboundedSender<UiMessage>,
) {
    tokio::spawn(async move {
        let configuration = UpdatesConfiguration {
            catch_up: true,
            update_queue_limit: Some(UPDATE_QUEUE_LIMIT),
        };
        let mut stream = match client.inner().stream_updates(updates, configuration).await {
            Ok(stream) => stream,
            Err(e) => {
                warn!("telegram: update stream unavailable: {e}");
                return;
            }
        };
        debug!("telegram: realtime update listener started");

        loop {
            match stream.next().await {
                Ok(Update::NewMessage(message)) => {
                    forward_message(
                        &message,
                        &self_user_id,
                        &selected_chat_id,
                        &video_registry,
                        &ui_tx,
                    )
                    .await;
                }
                // Edits, deletions, callbacks, inline results and raw updates: ignored.
                Ok(_) => {}
                Err(e) => {
                    warn!("telegram: update stream error: {e}");
                    tokio::time::sleep(RETRY_BACKOFF).await;
                }
            }
        }
    });
}

/// Map one incoming message and hand it to the UI.
async fn forward_message(
    message: &Message,
    self_user_id: &AtomicI64,
    selected_chat_id: &AtomicI64,
    video_registry: &Mutex<HashMap<i32, VideoDownloadInfo>>,
    ui_tx: &UnboundedSender<UiMessage>,
) {
    let chat_id = message.peer_id().bot_api_dialog_id().unwrap_or(UNSET);
    let self_id = self_user_id.load(Ordering::Relaxed);
    let self_id = (self_id != UNSET).then_some(self_id);

    let (info, video) = client::map_message(message, chat_id, self_id);

    // Register playable documents only for the open chat: the registry is cleared on
    // chat switch and holds the current chat's videos, so clicking Play on a live
    // message works. Videos from other chats would just be dropped on the next switch.
    if let Some(video) = &video
        && selected_chat_id.load(Ordering::Relaxed) == chat_id
    {
        video_registry
            .lock()
            .await
            .insert(video.msg_id, video.clone());
    }

    let _ = ui_tx.send(UiMessage::MessageReceived {
        chat_id,
        message: info,
    });
}
