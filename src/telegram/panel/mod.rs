//! Telegram sidebar UI: sign-in flow, chat list, and conversation view.
//!
//! Each screen is assembled from the standard `egui` layout structure:
//!
//! * a compact in-flow header (title, or back-button + chat name),
//! * a [`egui::Panel::bottom`] for the sign-out action, so it stays pinned
//!   while the list scrolls instead of being pushed off-screen,
//! * a [`egui::CentralPanel`] holding the scrolling body.
//!
//! Every [`egui::ScrollArea`] is given an explicit, unique `id_salt`. egui
//! persists scroll state per id, so two areas that share an id (or silently
//! inherit the same default salt) read and overwrite each other's scroll
//! offset. That is what previously wedged the chat list: its area shared the
//! default salt with the playlist's, a bad offset propagated between them, and
//! the list could never recover.

mod auth;
mod chat_list;
mod components;
mod messages;

use eframe::egui::{self, Align2, Color32, FontId};
use tokio::sync::mpsc::UnboundedSender;

use crate::style;

// Redesign palette (mirrors docs/ui-redesign-reference.md)
const C_TEXT: egui::Color32 = egui::Color32::from_rgb(0xf4, 0xf4, 0xf5);
const C_MUTED: egui::Color32 = egui::Color32::from_rgb(0xa1, 0xa1, 0xaa);
const C_RAISED: egui::Color32 = egui::Color32::from_rgb(0x1f, 0x1f, 0x26);
const C_RAISED2: egui::Color32 = egui::Color32::from_rgb(0x22, 0x22, 0x2b);
const C_BORDER2: egui::Color32 = egui::Color32::from_rgb(0x26, 0x26, 0x2e);
const C_ACCENT: egui::Color32 = egui::Color32::from_rgb(0x34, 0xc8, 0xb0);
use crate::telegram::BgCommand;
use crate::telegram::state_machine::{TelegramData, TelegramFsm, TelegramState};
use statig::blocking::StateMachine;

/// Height of a chat-list row (avatar + two lines of text).
const CHAT_ROW_HEIGHT: f32 = 64.0;
/// Avatar diameter in the chat list.
const AVATAR_SIZE: f32 = 40.0;
/// Horizontal padding inside a chat row.
const ROW_PADDING: f32 = 12.0;
/// Widest a message bubble may grow, as a fraction of the conversation width.
const BUBBLE_MAX_WIDTH: f32 = 0.72;

/// Video availability shown on a media card, derived from the Telegram FSM state.
#[derive(Clone, Copy, PartialEq, Eq)]
pub(crate) enum VideoStatus {
    /// Not the current video: playable.
    Ready,
    /// The current video is still being prepared.
    Loading,
    /// The current video is playing.
    Playing,
}

impl VideoStatus {
    fn label(self) -> &'static str {
        match self {
            Self::Ready => "Ready",
            Self::Loading => "Preparing…",
            Self::Playing => "Playing",
        }
    }
}

/// Sidebar widget for the Telegram chat UI.
pub struct TelegramPanel {
    open_chat: Option<String>,
    search_deadline: Option<std::time::Instant>,
}

impl TelegramPanel {
    pub fn new() -> Self {
        Self {
            open_chat: None,
            search_deadline: None,
        }
    }

    pub fn ui(
        &mut self,
        ui: &mut egui::Ui,
        fsm: &mut StateMachine<TelegramFsm>,
        bg: &Option<UnboundedSender<BgCommand>>,
    ) {
        // Keep the Telegram scrollbars slim so they don't cover the chat/message content.
        ui.style_mut().spacing.scroll.bar_width = 4.0;
        let state = fsm.state();
        match state {
            TelegramState::Connecting {} => self.connecting_ui(ui),
            TelegramState::Unauthenticated {} => self.auth_ui(ui, fsm, bg),
            TelegramState::Error {} => self.error_ui(ui, fsm),
            TelegramState::AwaitingCode {} => self.code_ui(ui, fsm, bg),
            TelegramState::AwaitingPassword {} => self.password_ui(ui, fsm, bg),
            TelegramState::ChatList {} => self.chat_list_ui(ui, fsm, bg),
            TelegramState::MessageList {} => self.message_list_ui(ui, fsm, bg),
        }
    }

    // ----------------------------------------------------------------- actions

    fn send(bg: &Option<UnboundedSender<BgCommand>>, cmd: BgCommand) {
        if let Some(bg) = bg {
            let _ = bg.send(cmd);
        }
    }

    /// Mutable access to the UI data stored inside the state machine.
    /// SAFETY: we only mutate plain UI fields, never the FSM's invariants.
    fn data_mut(fsm: &mut StateMachine<TelegramFsm>) -> &mut TelegramData {
        unsafe { &mut fsm.inner_mut().data }
    }

    // ------------------------------------------------------------------ screens
}

impl Default for TelegramPanel {
    fn default() -> Self {
        Self::new()
    }
}

/// Paint the initials-avatar used by chat rows.
fn paint_avatar(painter: &egui::Painter, rect: egui::Rect, name: &str) {
    painter.circle_filled(rect.center(), rect.width() / 2.0, style::avatar_fill(name));
    let initial = name
        .chars()
        .next()
        .unwrap_or('?')
        .to_uppercase()
        .to_string();
    painter.text(
        rect.center(),
        Align2::CENTER_CENTER,
        initial,
        FontId::proportional(15.0),
        Color32::WHITE,
    );
}
