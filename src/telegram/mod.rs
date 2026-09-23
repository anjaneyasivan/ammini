pub mod cache;
pub mod client;
pub mod config;
pub mod panel;
pub mod proxy;
pub mod session;
pub mod state_machine;

use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::mpsc::{UnboundedReceiver, UnboundedSender};

pub use client::{DialogInfo, MessageInfo, PeerRef};
pub use state_machine::{TelegramEvent, TelegramFsm, TelegramState};

use client::TelegramClient;
use config::TelegramConfig;
use proxy::{ProxyState, start_server};

/// Messages sent from the background thread to the UI.
#[derive(Debug)]
pub enum UiMessage {
    /// The combined proxy server is ready. Contains the local port.
    ProxyReady {
        port: u16,
    },
    NeedsAuth,
    AuthSuccess,
    CodeRequested,
    PasswordRequired(String),
    AuthError(String),
    DialogsPageLoaded {
        dialogs: Vec<DialogInfo>,
        has_more: bool,
        replace: bool,
    },
    MessagesPageLoaded {
        messages: Vec<MessageInfo>,
        has_more: bool,
        replace: bool,
    },
    VideoReady {
        msg_id: i32,
        url: String,
    },
    VideoError(String),
    Error(String),
}

/// Commands sent from the UI to the background thread.
#[derive(Debug)]
pub enum BgCommand {
    SubmitPhone(String),
    SubmitCode(String),
    SubmitPassword(Vec<u8>),
    LoadDialogs,
    LoadMoreDialogs,
    SelectChat(PeerRef),
    LoadMoreMessages,
    BackToChatList,
    PlayVideo(i32),
    StopVideo,
    SignOut,
}

/// Start the Telegram background thread and return the command sender and UI receiver.
pub fn start(config: TelegramConfig) -> (UnboundedSender<BgCommand>, UnboundedReceiver<UiMessage>) {
    let (ui_tx, ui_rx) = tokio::sync::mpsc::unbounded_channel::<UiMessage>();
    let (bg_tx, mut bg_rx) = tokio::sync::mpsc::unbounded_channel::<BgCommand>();

    std::thread::spawn(move || {
        let rt = match tokio::runtime::Runtime::new() {
            Ok(rt) => rt,
            Err(e) => {
                tracing::error!("telegram runtime failed: {e}");
                return;
            }
        };
        rt.block_on(run_telegram(config, ui_tx, &mut bg_rx));
    });

    (bg_tx, ui_rx)
}

async fn run_telegram(
    config: TelegramConfig,
    ui_tx: UnboundedSender<UiMessage>,
    bg_rx: &mut UnboundedReceiver<BgCommand>,
) {
    let session = match session::load_or_create_session().await {
        Ok(s) => s,
        Err(e) => {
            tracing::error!("failed to load Telegram session: {e}");
            let _ = ui_tx.send(UiMessage::Error(e.to_string()));
            return;
        }
    };

    let client = match TelegramClient::connect(&config, session).await {
        Ok(c) => c,
        Err(e) => {
            tracing::error!("failed to connect to Telegram: {e}");
            let _ = ui_tx.send(UiMessage::Error(e.to_string()));
            return;
        }
    };

    let client = Arc::new(client);
    let mut login_token: Option<client::LoginToken> = None;
    let mut password_token: Option<grammers_client::client::PasswordToken> = None;
    let mut dialogs_iter: Option<client::DialogIter> = None;
    let mut messages_iter: Option<client::MessageIter> = None;
    let mut selected_chat_id: Option<i64> = None;

    let video_registry: Arc<tokio::sync::Mutex<HashMap<i32, client::VideoDownloadInfo>>> =
        Arc::new(tokio::sync::Mutex::new(HashMap::new()));
    let video_cache: Arc<tokio::sync::Mutex<HashMap<i32, cache::BlockCache>>> =
        Arc::new(tokio::sync::Mutex::new(HashMap::new()));

    // Cache directory for the disk block cache. Per-video files are created lazily on
    // first request and evicted with `video_registry` on chat switch/sign-out.
    let cache_dir = dirs::cache_dir()
        .unwrap_or_else(std::env::temp_dir)
        .join("min-mpv")
        .join("telegram_cache");

    // One GC pass per launch: expire stale videos and keep the directory under budget
    // before the proxy can create new files.
    match cache::sweep_cache_dir(&cache_dir, cache::CACHE_MAX_BYTES, cache::CACHE_MAX_AGE) {
        Ok(removed) => tracing::info!("cache: swept {removed} bytes from {}", cache_dir.display()),
        Err(e) => tracing::warn!("cache: sweep failed: {e}"),
    }

    // Start the combined proxy server (remote URLs + Telegram videos). Telegram videos are
    // streamed through the per-video disk block cache.
    let proxy_state = ProxyState {
        reqwest_client: reqwest::Client::new(),
        video_registry: video_registry.clone(),
        video_source: Arc::new(client::GrammersVideoSource {
            client: client.clone_inner(),
        }),
        cache_dir: cache_dir.clone(),
        video_cache: video_cache.clone(),
    };

    let proxy_port = match start_server(proxy_state).await {
        Ok(port) => {
            let _ = ui_tx.send(UiMessage::ProxyReady { port });
            port
        }
        Err(e) => {
            tracing::error!("failed to start proxy server: {e}");
            let _ = ui_tx.send(UiMessage::Error(e.to_string()));
            return;
        }
    };

    match client.is_authorized().await {
        Ok(true) => {
            tracing::info!("telegram: already authorized");
            let _ = ui_tx.send(UiMessage::AuthSuccess);
            let mut iter = client.iter_dialogs();
            match client::next_dialogs_page(&mut iter, client::DIALOG_PAGE_SIZE).await {
                Ok((dialogs, has_more)) => {
                    dialogs_iter = Some(iter);
                    let _ = ui_tx.send(UiMessage::DialogsPageLoaded {
                        dialogs,
                        has_more,
                        replace: true,
                    });
                }
                Err(e) => {
                    let _ = ui_tx.send(UiMessage::Error(e.to_string()));
                }
            }
        }
        Ok(false) => {
            tracing::info!("telegram: not authorized, showing login");
            let _ = ui_tx.send(UiMessage::NeedsAuth);
        }
        Err(e) => {
            let _ = ui_tx.send(UiMessage::Error(e.to_string()));
        }
    }

    while let Some(command) = bg_rx.recv().await {
        match command {
            BgCommand::SubmitPhone(phone) => match client.request_login_code(&phone).await {
                Ok(token) => {
                    login_token = Some(token);
                    let _ = ui_tx.send(UiMessage::CodeRequested);
                }
                Err(e) => {
                    let _ =
                        ui_tx.send(UiMessage::AuthError(format!("Failed to request code: {e}")));
                }
            },
            BgCommand::SubmitCode(code) => {
                let token = match login_token.as_ref() {
                    Some(t) => t,
                    None => {
                        let _ = ui_tx.send(UiMessage::AuthError(
                            "No login session. Please start over.".to_string(),
                        ));
                        continue;
                    }
                };
                match client.sign_in(token, &code).await {
                    Ok(user) => {
                        tracing::info!(
                            "telegram: signed in (user {})",
                            user.id().bare_id_unchecked()
                        );
                        let _ = ui_tx.send(UiMessage::AuthSuccess);
                        let mut iter = client.iter_dialogs();
                        match client::next_dialogs_page(&mut iter, client::DIALOG_PAGE_SIZE).await {
                            Ok((dialogs, has_more)) => {
                                dialogs_iter = Some(iter);
                                let _ = ui_tx.send(UiMessage::DialogsPageLoaded {
                                    dialogs,
                                    has_more,
                                    replace: true,
                                });
                            }
                            Err(e) => {
                                let _ = ui_tx.send(UiMessage::Error(e.to_string()));
                            }
                        }
                    }
                    Err(client::SignInError::PasswordRequired(pw_token)) => {
                        let hint = pw_token.hint().map(|h| h.to_string()).unwrap_or_default();
                        password_token = Some(pw_token);
                        let _ = ui_tx.send(UiMessage::PasswordRequired(hint));
                    }
                    Err(client::SignInError::InvalidCode) => {
                        let _ = ui_tx.send(UiMessage::AuthError(
                            "Invalid verification code.".to_string(),
                        ));
                    }
                    Err(client::SignInError::SignUpRequired) => {
                        let _ = ui_tx.send(UiMessage::AuthError(
                            "Phone number not registered.".to_string(),
                        ));
                    }
                    Err(client::SignInError::InvalidPassword(_)) => {
                        let _ =
                            ui_tx.send(UiMessage::AuthError("Invalid 2FA password.".to_string()));
                    }
                    Err(client::SignInError::Other(e)) => {
                        let _ = ui_tx.send(UiMessage::AuthError(format!("Sign in error: {e}")));
                    }
                }
            }
            BgCommand::SubmitPassword(password) => {
                let pw_token = match password_token.take() {
                    Some(t) => t,
                    None => {
                        let _ = ui_tx.send(UiMessage::AuthError("No 2FA session.".to_string()));
                        continue;
                    }
                };
                match client.check_password(pw_token, password).await {
                    Ok(user) => {
                        tracing::info!("telegram: 2FA ok (user {})", user.id().bare_id_unchecked());
                        let _ = ui_tx.send(UiMessage::AuthSuccess);
                        let mut iter = client.iter_dialogs();
                        match client::next_dialogs_page(&mut iter, client::DIALOG_PAGE_SIZE).await {
                            Ok((dialogs, has_more)) => {
                                dialogs_iter = Some(iter);
                                let _ = ui_tx.send(UiMessage::DialogsPageLoaded {
                                    dialogs,
                                    has_more,
                                    replace: true,
                                });
                            }
                            Err(e) => {
                                let _ = ui_tx.send(UiMessage::Error(e.to_string()));
                            }
                        }
                    }
                    Err(client::SignInError::InvalidPassword(pw_token)) => {
                        password_token = Some(pw_token);
                        let _ =
                            ui_tx.send(UiMessage::AuthError("Invalid 2FA password.".to_string()));
                    }
                    Err(client::SignInError::Other(e)) => {
                        let _ = ui_tx.send(UiMessage::AuthError(format!("2FA error: {e}")));
                    }
                    _ => {
                        let _ =
                            ui_tx.send(UiMessage::AuthError("Unexpected 2FA error.".to_string()));
                    }
                }
            }
            BgCommand::LoadDialogs => {
                let mut iter = client.iter_dialogs();
                match client::next_dialogs_page(&mut iter, client::DIALOG_PAGE_SIZE).await {
                    Ok((dialogs, has_more)) => {
                        dialogs_iter = Some(iter);
                        let _ = ui_tx.send(UiMessage::DialogsPageLoaded {
                            dialogs,
                            has_more,
                            replace: true,
                        });
                    }
                    Err(e) => {
                        let _ = ui_tx.send(UiMessage::Error(e.to_string()));
                    }
                }
            }
            BgCommand::LoadMoreDialogs => {
                if let Some(iter) = dialogs_iter.as_mut() {
                    match client::next_dialogs_page(iter, client::DIALOG_PAGE_SIZE).await {
                        Ok((dialogs, has_more)) => {
                            let _ = ui_tx.send(UiMessage::DialogsPageLoaded {
                                dialogs,
                                has_more,
                                replace: false,
                            });
                        }
                        Err(e) => {
                            let _ = ui_tx.send(UiMessage::Error(e.to_string()));
                        }
                    }
                }
            }
            BgCommand::SelectChat(peer_ref) => {
                let chat_id = peer_ref.id.bot_api_dialog_id().unwrap_or(0);
                selected_chat_id = Some(chat_id);
                video_registry.lock().await.clear();
                video_cache.lock().await.clear();

                let mut iter = client.iter_messages(peer_ref);
                match client::next_messages_page(&mut iter, client::MESSAGE_PAGE_SIZE, chat_id)
                    .await
                {
                    Ok((messages, videos, has_more)) => {
                        messages_iter = Some(iter);
                        {
                            let mut vr = video_registry.lock().await;
                            for v in &videos {
                                vr.insert(v.msg_id, v.clone());
                            }
                        }
                        let _ = ui_tx.send(UiMessage::MessagesPageLoaded {
                            messages,
                            has_more,
                            replace: true,
                        });
                    }
                    Err(e) => {
                        let _ = ui_tx.send(UiMessage::Error(e.to_string()));
                    }
                }
            }
            BgCommand::LoadMoreMessages => {
                let chat_id = match selected_chat_id {
                    Some(id) => id,
                    None => continue,
                };
                if let Some(iter) = messages_iter.as_mut() {
                    match client::next_messages_page(iter, client::MESSAGE_PAGE_SIZE, chat_id).await
                    {
                        Ok((messages, videos, has_more)) => {
                            {
                                let mut vr = video_registry.lock().await;
                                for v in &videos {
                                    vr.insert(v.msg_id, v.clone());
                                }
                            }
                            let _ = ui_tx.send(UiMessage::MessagesPageLoaded {
                                messages,
                                has_more,
                                replace: false,
                            });
                        }
                        Err(e) => {
                            let _ = ui_tx.send(UiMessage::Error(e.to_string()));
                        }
                    }
                }
            }
            BgCommand::BackToChatList => {
                messages_iter = None;
                selected_chat_id = None;
                video_registry.lock().await.clear();
                video_cache.lock().await.clear();
            }
            BgCommand::PlayVideo(msg_id) => {
                let video = {
                    let vr = video_registry.lock().await;
                    vr.get(&msg_id).cloned()
                };
                let video = match video {
                    Some(v) => v,
                    None => {
                        let _ = ui_tx.send(UiMessage::VideoError(
                            "Video not found in registry".to_string(),
                        ));
                        continue;
                    }
                };

                let size = video.size as u64;
                if size == 0 {
                    let _ = ui_tx.send(UiMessage::VideoError("Unknown video size".to_string()));
                    continue;
                }

                // No cache and no background download: the proxy streams the video straight
                // from Telegram to the player as the player requests bytes.
                let url = format!("http://127.0.0.1:{}/telegram/{}", proxy_port, msg_id);
                let _ = ui_tx.send(UiMessage::VideoReady { msg_id, url });
            }
            BgCommand::StopVideo => {
                tracing::debug!("telegram: StopVideo (no-op)");
            }
            BgCommand::SignOut => {
                tracing::info!("telegram: signing out");
                dialogs_iter = None;
                messages_iter = None;
                selected_chat_id = None;
                video_registry.lock().await.clear();
                video_cache.lock().await.clear();
                login_token = None;
                password_token = None;
                // Revoke the session server-side and delete the local session file and
                // cached videos, so the next launch starts truly logged out.
                if let Err(e) = client.sign_out().await {
                    tracing::warn!("sign_out: {e}");
                }
                if let Err(e) = session::delete_session() {
                    tracing::warn!("failed to delete session file: {e}");
                }
                if let Err(e) = std::fs::remove_dir_all(&cache_dir) {
                    tracing::warn!("failed to wipe cache dir {}: {e}", cache_dir.display());
                }
                let _ = ui_tx.send(UiMessage::NeedsAuth);
            }
        }
    }
}
