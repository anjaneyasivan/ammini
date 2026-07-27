use statig::prelude::*;
use tracing::info;

use crate::telegram::client::{DialogInfo, MessageInfo, PeerRef};

/// Events that drive the Telegram state machine.
#[derive(Debug, Clone)]
pub enum TelegramEvent {
    /// Session exists but user is not authenticated; show phone entry.
    NeedsAuth,
    /// Authentication succeeded.
    AuthSucceeded,
    /// Authentication failed with an error message.
    AuthFailed(String),
    /// Server requested a login code.
    CodeRequested,
    /// Server requested 2FA password, with an optional hint.
    PasswordRequired(String),
    /// User submitted a phone number.
    PhoneSubmitted(String),
    /// User submitted a verification code.
    CodeSubmitted(String),
    /// User submitted a 2FA password.
    PasswordSubmitted(String),
    /// Dialogs loaded from Telegram. (dialogs, has_more, replace)
    DialogsLoaded(Vec<DialogInfo>, bool, bool),
    /// Messages loaded for a chat. (messages, has_more, replace)
    MessagesLoaded(Vec<MessageInfo>, bool, bool),
    /// A chat was selected from the list.
    ChatSelected(PeerRef),
    /// Go back to chat list from message view.
    BackToChatList,
    /// Sign out and return to unauthenticated state.
    SignOut,
    /// An unexpected error occurred.
    Error(String),
    /// Reset from error state back to the phone-entry form.
    ResetAuth,
    /// A Telegram video started loading.
    VideoLoading(i32),
    /// A Telegram video is ready to play.
    VideoReady,
    /// A Telegram video failed to prepare.
    VideoError(String),
}

/// Data held by the Telegram state machine.
#[derive(Default)]
pub struct TelegramData {
    pub phone: String,
    pub code: String,
    pub password: String,
    pub password_hint: Option<String>,
    pub error: Option<String>,
    pub dialogs: Vec<DialogInfo>,
    pub messages: Vec<MessageInfo>,
    pub selected_chat: Option<PeerRef>,
    pub selected_chat_name: Option<String>,
    pub has_more_dialogs: bool,
    pub has_more_messages: bool,
    pub loading_video: bool,
    pub current_video: Option<i32>,
}

impl TelegramData {
    pub fn clear(&mut self) {
        *self = Self::default();
    }

    fn update_dialogs(&mut self, dialogs: &[DialogInfo], has_more: bool, replace: bool) {
        if replace {
            self.dialogs = dialogs.to_vec();
        } else {
            self.dialogs.extend_from_slice(dialogs);
        }
        self.has_more_dialogs = has_more;
    }

    fn update_messages(&mut self, messages: &[MessageInfo], has_more: bool, replace: bool) {
        if replace {
            // API returns newest-first; store oldest-first for display.
            self.messages = messages.iter().rev().cloned().collect();
        } else {
            // Older pages arrive newest-first; prepend them in reversed order.
            let mut older: Vec<MessageInfo> = messages.iter().rev().cloned().collect();
            older.extend(self.messages.drain(..));
            self.messages = older;
        }
        self.has_more_messages = has_more;
    }
}

/// State machine wrapper for Telegram UI.
pub struct TelegramFsm {
    pub data: TelegramData,
}

impl TelegramFsm {
    pub fn new() -> Self {
        Self {
            data: TelegramData::default(),
        }
    }
}

impl Default for TelegramFsm {
    fn default() -> Self {
        Self::new()
    }
}

#[state_machine(state(name = "TelegramState"), initial = "TelegramState::connecting()")]
impl TelegramFsm {
    #[state]
    fn connecting(&mut self, event: &TelegramEvent) -> Outcome<TelegramState> {
        match event {
            TelegramEvent::NeedsAuth => {
                self.data.error = None;
                Transition(TelegramState::unauthenticated())
            }
            TelegramEvent::AuthSucceeded => Transition(TelegramState::chat_list()),
            TelegramEvent::DialogsLoaded(dialogs, has_more, replace) => {
                self.data.update_dialogs(dialogs, *has_more, *replace);
                Transition(TelegramState::chat_list())
            }
            TelegramEvent::AuthFailed(msg) => {
                self.data.error = Some(msg.clone());
                Transition(TelegramState::unauthenticated())
            }
            TelegramEvent::CodeRequested => Transition(TelegramState::awaiting_code()),
            TelegramEvent::PasswordRequired(hint) => {
                self.data.password_hint = Some(hint.clone());
                Transition(TelegramState::awaiting_password())
            }
            TelegramEvent::Error(msg) => {
                self.data.error = Some(msg.clone());
                Transition(TelegramState::error())
            }
            _ => Handled,
        }
    }

    #[state]
    fn unauthenticated(&mut self, event: &TelegramEvent) -> Outcome<TelegramState> {
        match event {
            TelegramEvent::PhoneSubmitted(phone) => {
                self.data.phone = phone.clone();
                self.data.error = None;
                info!("telegram: phone submitted, awaiting code");
                Transition(TelegramState::awaiting_code())
            }
            TelegramEvent::NeedsAuth => {
                self.data.error = None;
                Handled
            }
            TelegramEvent::Error(msg) => {
                self.data.error = Some(msg.clone());
                Transition(TelegramState::error())
            }
            _ => Handled,
        }
    }

    #[state]
    fn awaiting_code(&mut self, event: &TelegramEvent) -> Outcome<TelegramState> {
        match event {
            TelegramEvent::CodeSubmitted(code) => {
                self.data.code = code.clone();
                self.data.error = None;
                Handled
            }
            TelegramEvent::PasswordRequired(hint) => {
                self.data.password_hint = Some(hint.clone());
                self.data.error = None;
                Transition(TelegramState::awaiting_password())
            }
            TelegramEvent::AuthSucceeded => Transition(TelegramState::chat_list()),
            TelegramEvent::AuthFailed(msg) => {
                self.data.error = Some(msg.clone());
                Transition(TelegramState::unauthenticated())
            }
            TelegramEvent::NeedsAuth => {
                self.data.error = None;
                Transition(TelegramState::unauthenticated())
            }
            TelegramEvent::Error(msg) => {
                self.data.error = Some(msg.clone());
                Transition(TelegramState::error())
            }
            _ => Handled,
        }
    }

    #[state]
    fn awaiting_password(&mut self, event: &TelegramEvent) -> Outcome<TelegramState> {
        match event {
            TelegramEvent::PasswordSubmitted(pw) => {
                self.data.password = pw.clone();
                self.data.error = None;
                Handled
            }
            TelegramEvent::AuthSucceeded => Transition(TelegramState::chat_list()),
            TelegramEvent::AuthFailed(msg) => {
                self.data.error = Some(msg.clone());
                Transition(TelegramState::unauthenticated())
            }
            TelegramEvent::NeedsAuth => {
                self.data.error = None;
                Transition(TelegramState::unauthenticated())
            }
            TelegramEvent::Error(msg) => {
                self.data.error = Some(msg.clone());
                Transition(TelegramState::error())
            }
            _ => Handled,
        }
    }

    #[state]
    fn chat_list(&mut self, event: &TelegramEvent) -> Outcome<TelegramState> {
        match event {
            TelegramEvent::DialogsLoaded(dialogs, has_more, replace) => {
                self.data.update_dialogs(dialogs, *has_more, *replace);
                Handled
            }
            TelegramEvent::ChatSelected(peer) => {
                self.data.selected_chat = Some(*peer);
                self.data.selected_chat_name = self
                    .data
                    .dialogs
                    .iter()
                    .find(|d| d.peer_ref == *peer)
                    .map(|d| d.name.clone());
                Transition(TelegramState::message_list())
            }
            TelegramEvent::SignOut | TelegramEvent::NeedsAuth => {
                self.data.clear();
                Transition(TelegramState::unauthenticated())
            }
            TelegramEvent::Error(msg) => {
                self.data.error = Some(msg.clone());
                Transition(TelegramState::error())
            }
            _ => Handled,
        }
    }

    #[state]
    fn message_list(&mut self, event: &TelegramEvent) -> Outcome<TelegramState> {
        match event {
            TelegramEvent::MessagesLoaded(messages, has_more, replace) => {
                self.data.update_messages(messages, *has_more, *replace);
                Handled
            }
            TelegramEvent::VideoLoading(msg_id) => {
                self.data.loading_video = true;
                self.data.current_video = Some(*msg_id);
                self.data.error = None;
                Handled
            }
            TelegramEvent::VideoReady => {
                self.data.loading_video = false;
                Handled
            }
            TelegramEvent::VideoError(msg) => {
                self.data.loading_video = false;
                self.data.error = Some(msg.clone());
                Handled
            }
            TelegramEvent::BackToChatList => {
                self.data.messages.clear();
                self.data.selected_chat = None;
                self.data.selected_chat_name = None;
                self.data.loading_video = false;
                self.data.current_video = None;
                self.data.error = None;
                Transition(TelegramState::chat_list())
            }
            TelegramEvent::SignOut | TelegramEvent::NeedsAuth => {
                self.data.clear();
                Transition(TelegramState::unauthenticated())
            }
            TelegramEvent::Error(msg) => {
                self.data.error = Some(msg.clone());
                Transition(TelegramState::error())
            }
            _ => Handled,
        }
    }

    #[state]
    fn error(&mut self, event: &TelegramEvent) -> Outcome<TelegramState> {
        match event {
            TelegramEvent::ResetAuth | TelegramEvent::NeedsAuth => {
                self.data.error = None;
                Transition(TelegramState::unauthenticated())
            }
            _ => Handled,
        }
    }
}

/// Helper to map a background error to a state-machine event.
pub fn ui_message_to_event(msg: &crate::telegram::UiMessage) -> Option<TelegramEvent> {
    use crate::telegram::UiMessage;
    Some(match msg {
        UiMessage::NeedsAuth => TelegramEvent::NeedsAuth,
        UiMessage::AuthSuccess => TelegramEvent::AuthSucceeded,
        UiMessage::CodeRequested => TelegramEvent::CodeRequested,
        UiMessage::PasswordRequired(hint) => TelegramEvent::PasswordRequired(hint.clone()),
        UiMessage::AuthError(e) => TelegramEvent::AuthFailed(e.clone()),
        UiMessage::DialogsPageLoaded {
            dialogs,
            has_more,
            replace,
        } => TelegramEvent::DialogsLoaded(dialogs.clone(), *has_more, *replace),
        UiMessage::MessagesPageLoaded {
            messages,
            has_more,
            replace,
        } => TelegramEvent::MessagesLoaded(messages.clone(), *has_more, *replace),
        UiMessage::Error(e) => TelegramEvent::Error(e.clone()),
        UiMessage::VideoError(e) => TelegramEvent::VideoError(e.clone()),
        UiMessage::VideoReady { .. } => TelegramEvent::VideoReady,
        UiMessage::ProxyReady { .. } => {
            // The UI uses the proxy port directly; no state change needed.
            return None;
        }
    })
}
