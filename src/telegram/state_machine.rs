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
            older.append(&mut self.messages);
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
            TelegramEvent::AuthSucceeded => Transition(TelegramState::chat_list()),
            TelegramEvent::DialogsLoaded(dialogs, has_more, replace) => {
                // A session may resolve as authorized while the login screen is up;
                // keep the dialogs so the chat list isn't empty when we land on it.
                self.data.update_dialogs(dialogs, *has_more, *replace);
                Handled
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
            TelegramEvent::DialogsLoaded(dialogs, has_more, replace) => {
                // Dialogs may already be in flight while auth completes; keep them so
                // the chat list isn't empty once we transition.
                self.data.update_dialogs(dialogs, *has_more, *replace);
                Handled
            }
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
            TelegramEvent::DialogsLoaded(dialogs, has_more, replace) => {
                self.data.update_dialogs(dialogs, *has_more, *replace);
                Handled
            }
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
        UiMessage::CacheCoverage { .. } => {
            // Seekbar cache-coverage shading; no state change needed.
            return None;
        }
    })
}

#[cfg(test)]
mod tests {
    use super::{TelegramData, TelegramEvent, TelegramFsm, TelegramState, ui_message_to_event};
    use crate::telegram::UiMessage;
    use crate::telegram::client::{DialogInfo, MessageInfo, PeerRef};
    use grammers_session::types::{PeerAuth, PeerId};
    use statig::prelude::IntoStateMachineExt;

    fn msg(id: i32) -> MessageInfo {
        MessageInfo {
            id,
            sender: String::new(),
            sender_is_self: false,
            text: format!("msg {id}"),
            time: String::new(),
            has_video: false,
        }
    }

    fn peer_ref(id: i64) -> PeerRef {
        PeerRef {
            id: PeerId::from_bot_api_dialog_id(id).unwrap(),
            auth: PeerAuth::from_hash(0),
        }
    }

    #[test]
    fn pages_store_messages_oldest_first() {
        let mut data = TelegramData::default();

        // API returns newest-first; the first (fresh) page is stored oldest-first.
        data.update_messages(&[msg(3), msg(2), msg(1)], true, true);
        let ids: Vec<i32> = data.messages.iter().map(|m| m.id).collect();
        assert_eq!(ids, vec![1, 2, 3], "stored oldest-first for display");

        // An older page arrives newest-first ([5, 4]); it is reversed and prepended
        // above the current messages.
        data.update_messages(&[msg(5), msg(4)], true, false);
        let ids: Vec<i32> = data.messages.iter().map(|m| m.id).collect();
        assert_eq!(
            ids,
            vec![4, 5, 1, 2, 3],
            "older messages prepended, oldest first"
        );
    }

    #[test]
    fn startup_sequence_populates_dialogs_then_messages() {
        let mut fsm = TelegramFsm::new().state_machine();
        fsm.init();

        // Replays the startup sequence: already authorized, then a page of dialogs.
        fsm.handle(&ui_message_to_event(&UiMessage::AuthSuccess).unwrap());
        let dialogs: Vec<DialogInfo> = (0..20)
            .map(|i| DialogInfo {
                peer_ref: peer_ref(i + 1),
                name: format!("Chat {i}"),
                last_message: None,
            })
            .collect();
        fsm.handle(&TelegramEvent::DialogsLoaded(dialogs.clone(), true, true));

        assert!(matches!(fsm.state(), TelegramState::ChatList {}));
        assert_eq!(fsm.data.dialogs.len(), 20, "20 dialogs must reach the UI");

        // Open a chat: it transitions to the message list and remembers the name.
        fsm.handle(&TelegramEvent::ChatSelected(dialogs[0].peer_ref));
        assert!(matches!(fsm.state(), TelegramState::MessageList {}));
        assert_eq!(fsm.data.selected_chat_name.as_deref(), Some("Chat 0"));

        // The fetched page arrives and populates the messages.
        fsm.handle(&TelegramEvent::MessagesLoaded(
            vec![msg(3), msg(2), msg(1)],
            true,
            true,
        ));
        assert_eq!(fsm.data.messages.len(), 3);
    }

    #[test]
    fn auth_succeeded_in_unauthenticated_opens_chat_list() {
        let mut fsm = TelegramFsm::new().state_machine();
        fsm.init();
        fsm.handle(&TelegramEvent::NeedsAuth);
        assert!(matches!(fsm.state(), TelegramState::Unauthenticated {}));

        fsm.handle(&TelegramEvent::AuthSucceeded);
        assert!(
            matches!(fsm.state(), TelegramState::ChatList {}),
            "AuthSucceeded must not be dropped in Unauthenticated"
        );
    }

    #[test]
    fn dialogs_loaded_while_awaiting_code_are_kept() {
        let mut fsm = TelegramFsm::new().state_machine();
        fsm.init();
        fsm.handle(&TelegramEvent::NeedsAuth);
        fsm.handle(&TelegramEvent::PhoneSubmitted("+1 234".into()));
        assert!(matches!(fsm.state(), TelegramState::AwaitingCode {}));

        let dialogs: Vec<DialogInfo> = (0..3)
            .map(|i| DialogInfo {
                peer_ref: peer_ref(i + 1),
                name: format!("Chat {i}"),
                last_message: None,
            })
            .collect();
        fsm.handle(&TelegramEvent::DialogsLoaded(dialogs.clone(), true, true));
        // Still mid-login: stay in the auth flow, but keep the dialogs so the chat
        // list isn't empty once auth completes.
        assert!(matches!(fsm.state(), TelegramState::AwaitingCode {}));
        assert_eq!(fsm.data.dialogs.len(), 3);

        fsm.handle(&TelegramEvent::AuthSucceeded);
        assert!(matches!(fsm.state(), TelegramState::ChatList {}));
        assert_eq!(
            fsm.data.dialogs.len(),
            3,
            "dialogs must survive into the chat list"
        );
    }
}
