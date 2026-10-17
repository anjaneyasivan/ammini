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

use eframe::egui::{
    self, Align, Align2, Color32, CornerRadius, FontId, Key, Layout, Margin, RichText, Sense,
    UiBuilder,
};
use egui_material_icons::{MaterialIcon, icons::*};
use tokio::sync::mpsc::UnboundedSender;

use crate::fonts::{icon_label, icon_label_colored, icon_only};
use crate::style;

// Redesign palette (mirrors docs/ui-redesign-reference.md)
const C_TEXT: egui::Color32 = egui::Color32::from_rgb(0xf4, 0xf4, 0xf5);
const C_MUTED: egui::Color32 = egui::Color32::from_rgb(0xa1, 0xa1, 0xaa);
const C_RAISED: egui::Color32 = egui::Color32::from_rgb(0x1f, 0x1f, 0x26);
const C_RAISED2: egui::Color32 = egui::Color32::from_rgb(0x22, 0x22, 0x2b);
const C_BORDER2: egui::Color32 = egui::Color32::from_rgb(0x26, 0x26, 0x2e);
const C_ACCENT: egui::Color32 = egui::Color32::from_rgb(0x34, 0xc8, 0xb0);
use crate::telegram::media_meta::{format_bytes, format_duration};
use crate::telegram::state_machine::{TelegramData, TelegramEvent, TelegramFsm, TelegramState};
use crate::telegram::{BgCommand, DialogInfo, MediaMeta, MessageInfo};
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
enum VideoStatus {
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

    fn connecting_ui(&self, ui: &mut egui::Ui) {
        ui.vertical_centered(|ui| {
            ui.add_space(ui.available_height() * 0.35);
            ui.spinner();
            ui.add_space(10.0);
            ui.label(
                RichText::new("Connecting to Telegram…")
                    .size(13.0)
                    .color(C_MUTED),
            );
        });
    }

    fn auth_ui(
        &mut self,
        ui: &mut egui::Ui,
        fsm: &mut StateMachine<TelegramFsm>,
        bg: &Option<UnboundedSender<BgCommand>>,
    ) {
        let dark = true;
        ui.add_space(10.0);
        ui.label(RichText::new("Sign in").strong().size(17.0));
        ui.add_space(3.0);
        ui.label(
            RichText::new("Enter the phone number linked to your Telegram account.")
                .size(12.5)
                .color(C_MUTED),
        );
        ui.add_space(14.0);

        let mut submit = false;
        let phone = {
            let data = Self::data_mut(fsm);
            if let Some(err) = data.error.clone() {
                Self::error_banner(ui, dark, &err);
                ui.add_space(10.0);
            }
            let field = Self::labeled_field(
                ui,
                "Phone number",
                "+1 555 123 4567",
                false,
                &mut data.phone,
            );
            if ui.memory(|m| m.focused().is_none()) {
                field.request_focus();
            }
            if field.lost_focus() && ui.input(|i| i.key_pressed(Key::Enter)) {
                submit = true;
            }
            data.phone.trim().to_owned()
        };

        ui.add_space(12.0);
        if Self::primary_button(ui, ICON_SEND, "Send code", !phone.is_empty()).clicked() {
            submit = true;
        }
        if submit && !phone.is_empty() {
            fsm.handle(&TelegramEvent::PhoneSubmitted(phone.clone()));
            Self::send(bg, BgCommand::SubmitPhone(phone));
        }
    }

    fn code_ui(
        &mut self,
        ui: &mut egui::Ui,
        fsm: &mut StateMachine<TelegramFsm>,
        bg: &Option<UnboundedSender<BgCommand>>,
    ) {
        let dark = true;
        ui.add_space(10.0);
        ui.label(RichText::new("Verification").strong().size(17.0));
        ui.add_space(3.0);
        ui.label(
            RichText::new("Enter the code Telegram sent to your app.")
                .size(12.5)
                .color(C_MUTED),
        );
        ui.add_space(14.0);

        let mut submit = false;
        let code = {
            let data = Self::data_mut(fsm);
            if let Some(err) = data.error.clone() {
                Self::error_banner(ui, dark, &err);
                ui.add_space(10.0);
            }
            let field = Self::labeled_field(ui, "Login code", "12345", false, &mut data.code);
            if ui.memory(|m| m.focused().is_none()) {
                field.request_focus();
            }
            if field.lost_focus() && ui.input(|i| i.key_pressed(Key::Enter)) {
                submit = true;
            }
            data.code.trim().to_owned()
        };

        ui.add_space(12.0);
        if Self::primary_button(ui, ICON_CHECK, "Verify", !code.is_empty()).clicked() {
            submit = true;
        }
        if submit && !code.is_empty() {
            fsm.handle(&TelegramEvent::CodeSubmitted(code.clone()));
            Self::send(bg, BgCommand::SubmitCode(code));
        }

        ui.add_space(6.0);
        if Self::ghost_button(ui, ICON_ARROW_BACK, "Change phone number").clicked() {
            Self::data_mut(fsm).code.clear();
            fsm.handle(&TelegramEvent::NeedsAuth);
        }
    }

    fn password_ui(
        &mut self,
        ui: &mut egui::Ui,
        fsm: &mut StateMachine<TelegramFsm>,
        bg: &Option<UnboundedSender<BgCommand>>,
    ) {
        let dark = true;
        ui.add_space(10.0);
        ui.label(
            RichText::new("Two-factor authentication")
                .strong()
                .size(17.0),
        );
        if let Some(hint) = fsm.data.password_hint.clone()
            && !hint.is_empty()
        {
            ui.add_space(3.0);
            ui.label(
                RichText::new(format!("Hint: {hint}"))
                    .size(12.5)
                    .color(C_MUTED),
            );
        }
        ui.add_space(14.0);

        let mut submit = false;
        let (password, display) = {
            let data = Self::data_mut(fsm);
            if let Some(err) = data.error.clone() {
                Self::error_banner(ui, dark, &err);
                ui.add_space(10.0);
            }
            let field = Self::labeled_field(
                ui,
                "Password",
                "Your 2FA password",
                true,
                &mut data.password,
            );
            if ui.memory(|m| m.focused().is_none()) {
                field.request_focus();
            }
            if field.lost_focus() && ui.input(|i| i.key_pressed(Key::Enter)) {
                submit = true;
            }
            (data.password.clone().into_bytes(), data.password.clone())
        };

        ui.add_space(12.0);
        if Self::primary_button(ui, ICON_LOGIN, "Sign in", !password.is_empty()).clicked() {
            submit = true;
        }
        if submit && !password.is_empty() {
            fsm.handle(&TelegramEvent::PasswordSubmitted(display));
            Self::send(bg, BgCommand::SubmitPassword(password));
        }

        ui.add_space(6.0);
        if Self::ghost_button(ui, ICON_ARROW_BACK, "Change phone number").clicked() {
            Self::data_mut(fsm).password.clear();
            fsm.handle(&TelegramEvent::NeedsAuth);
        }
    }

    fn error_ui(&self, ui: &mut egui::Ui, fsm: &mut StateMachine<TelegramFsm>) {
        let dark = true;
        ui.add_space(10.0);
        ui.label(RichText::new("Something went wrong").strong().size(17.0));
        ui.add_space(10.0);

        let message = fsm
            .data
            .error
            .clone()
            .unwrap_or_else(|| "Unknown error.".to_owned());
        Self::error_banner(ui, dark, &message);
        ui.add_space(14.0);

        if Self::primary_button(ui, ICON_REFRESH, "Try again", true).clicked() {
            fsm.handle(&TelegramEvent::ResetAuth);
        }
    }

    fn chat_list_ui(
        &mut self,
        ui: &mut egui::Ui,
        fsm: &mut StateMachine<TelegramFsm>,
        bg: &Option<UnboundedSender<BgCommand>>,
    ) {
        ui.add_space(8.0);
        // Search input (interactive, with debounce)
        ui.horizontal(|ui| {
            ui.add_space(12.0);
            let mut q = fsm.data.search_query.clone();
            let resp = ui.add(
                egui::TextEdit::singleline(&mut q)
                    .hint_text("Search Telegram chats or files...")
                    .margin(Margin::symmetric(8, 6))
                    .desired_width(ui.available_width() - 12.0),
            );
            if resp.changed() {
                unsafe { &mut fsm.inner_mut().data }.search_query = q.clone();
                self.search_deadline =
                    Some(std::time::Instant::now() + std::time::Duration::from_millis(400));
                if q.trim().is_empty() {
                    unsafe { &mut fsm.inner_mut().data }.search_results.clear();
                    unsafe { &mut fsm.inner_mut().data }.search_loading = false;
                } else {
                    unsafe { &mut fsm.inner_mut().data }.search_loading = true;
                }
                fsm.handle(&TelegramEvent::SearchInputChanged(q));
            }
        });
        // Debounce: fire search when deadline passes
        if let Some(dl) = self.search_deadline {
            if std::time::Instant::now() >= dl {
                self.search_deadline = None;
                let q = fsm.data.search_query.clone();
                if !q.trim().is_empty() {
                    Self::send(bg, BgCommand::SearchChats(q));
                }
            }
        }
        ui.add_space(6.0);

        if fsm.data.dialog_filter.is_empty() {
            unsafe { &mut fsm.inner_mut().data }.dialog_filter = "All".to_string();
        }
        let is_searching = !fsm.data.search_query.trim().is_empty();

        if !is_searching {
            // Filter pills (horizontally scrollable)
            ui.add_space(0.0);
            egui::ScrollArea::horizontal()
                .id_salt("tg_filter_pills")
                .max_width(ui.available_width())
                .show(ui, |ui| {
                    ui.horizontal(|ui| {
                        ui.add_space(12.0);
                        for f in ["All", "Channels", "Groups", "Directs", "Unread", "Media"] {
                            let active = fsm.data.dialog_filter == f;
                            let fill = if active {
                                C_RAISED2
                            } else {
                                Color32::TRANSPARENT
                            };
                            let stroke = if active {
                                egui::Stroke::new(1.0, C_BORDER2)
                            } else {
                                egui::Stroke::NONE
                            };
                            let btn = ui.add(
                                egui::Button::new(RichText::new(f).size(10.0).color(if active {
                                    C_TEXT
                                } else {
                                    C_MUTED
                                }))
                                .fill(fill)
                                .stroke(stroke)
                                .corner_radius(CornerRadius::same(4))
                                .min_size(egui::vec2(0.0, 20.0)),
                            );
                            if btn.clicked() {
                                unsafe { &mut fsm.inner_mut().data }.dialog_filter = f.to_string();
                            }
                            ui.add_space(4.0);
                        }
                    });
                });
            ui.add_space(6.0);
        }

        ui.separator();

        // Footer (pinned) ----------------------------------------------------
        let mut sign_out = false;
        egui::Panel::bottom("tg_chats_footer")
            .frame(egui::Frame::NONE)
            .show(ui, |ui| {
                ui.add_space(6.0);
                ui.add_space(2.0);
                ui.horizontal(|ui| {
                    ui.add_space(12.0);
                    ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                        ui.add_space(12.0);
                        if Self::ghost_button(ui, ICON_LOGOUT, "Sign out").clicked() {
                            sign_out = true;
                        }
                    });
                });
                ui.add_space(4.0);
            });

        // Body (scrolls) -----------------------------------------------------
        let mut selected = None;
        let mut load_more = false;
        egui::CentralPanel::no_frame().show(ui, |ui| {
            if is_searching {
                if fsm.data.search_loading {
                    ui.add_space(8.0);
                    ui.label(RichText::new("Searching...").color(C_MUTED));
                    return;
                }
                if fsm.data.search_results.is_empty() {
                    ui.add_space(8.0);
                    ui.label(RichText::new("No results").color(C_MUTED));
                    return;
                }
                egui::ScrollArea::vertical()
                    .id_salt("tg_search_results")
                    .auto_shrink([false, false])
                    .show(ui, |ui| {
                        for res in fsm.data.search_results.clone() {
                            let (rect, resp) = ui.allocate_exact_size(
                                egui::vec2(ui.available_width(), 52.0),
                                Sense::click(),
                            );
                            ui.painter().rect_filled(
                                rect,
                                4.0,
                                if resp.hovered() {
                                    C_RAISED
                                } else {
                                    Color32::TRANSPARENT
                                },
                            );
                            ui.painter().text(
                                rect.left_top() + egui::vec2(8.0, 6.0),
                                Align2::LEFT_TOP,
                                &res.chat_name,
                                FontId::proportional(12.0),
                                C_TEXT,
                            );
                            let snippet = if res.snippet.chars().count() > 46 {
                                format!("{}…", res.snippet.chars().take(46).collect::<String>())
                            } else {
                                res.snippet.clone()
                            };
                            ui.painter().text(
                                rect.left_top() + egui::vec2(8.0, 24.0),
                                Align2::LEFT_TOP,
                                &snippet,
                                FontId::monospace(10.0),
                                C_MUTED,
                            );
                            if resp.clicked() {
                                selected = Some(res.peer);
                                unsafe { &mut fsm.inner_mut().data }.search_target_message =
                                    Some(res.message_id);
                            }
                        }
                    });
                if let Some(peer) = selected {
                    let msg_id = fsm.data.search_target_message.unwrap_or(0);
                    fsm.handle(&TelegramEvent::SearchResultSelected { peer, msg_id });
                    Self::send(bg, BgCommand::SelectChat(peer));
                }
                return;
            }

            if fsm.data.dialogs.is_empty() {
                Self::empty_state(ui, "No chats yet", true);
                return;
            }
            egui::ScrollArea::vertical()
                .id_salt("tg_chat_list")
                .auto_shrink([false, false])
                .show(ui, |ui| {
                    for dialog in &fsm.data.dialogs {
                        let passes = match fsm.data.dialog_filter.as_str() {
                            "Channels" => dialog.dialog_type == "channel",
                            "Groups" => dialog.dialog_type == "group",
                            "Directs" => dialog.dialog_type == "direct",
                            "Unread" => dialog.unread_count > 0,
                            "Media" => true,
                            _ => true,
                        };
                        if !passes {
                            continue;
                        }
                        let is_selected = fsm.data.selected_chat == Some(dialog.peer_ref);
                        if Self::chat_row(ui, dialog, is_selected).clicked() {
                            selected = Some(dialog.peer_ref);
                        }
                    }
                    if fsm.data.has_more_dialogs {
                        ui.add_space(4.0);
                        if Self::load_more_button(ui, "Load more chats").clicked() {
                            load_more = true;
                        }
                        ui.add_space(6.0);
                    }
                });
        });

        if let Some(peer) = selected {
            fsm.handle(&TelegramEvent::ChatSelected(peer));
            Self::send(bg, BgCommand::SelectChat(peer));
        }
        if load_more {
            Self::send(bg, BgCommand::LoadMoreDialogs);
        }
        if sign_out {
            Self::send(bg, BgCommand::SignOut);
        }
    }

    fn message_list_ui(
        &mut self,
        ui: &mut egui::Ui,
        fsm: &mut StateMachine<TelegramFsm>,
        bg: &Option<UnboundedSender<BgCommand>>,
    ) {
        let dark = true;
        // Header ------------------------------------------------------------
        let mut back = false;
        ui.add_space(8.0);
        ui.horizontal(|ui| {
            ui.add_space(2.0);
            if ui
                .add(egui::Button::new(icon_only(ui, ICON_ARROW_BACK)).frame(false))
                .on_hover_text("Back to chats")
                .clicked()
            {
                back = true;
            }
            let title = fsm
                .data
                .selected_chat_name
                .clone()
                .unwrap_or_else(|| "Chat".to_owned());
            ui.add(egui::Label::new(RichText::new(title).strong().size(16.0)).truncate());
        });
        ui.add_space(6.0);
        ui.separator();

        // Inline error (e.g. a failed video preparation) ---------------------
        if let Some(err) = fsm.data.error.clone() {
            ui.add_space(8.0);
            Self::error_banner(ui, dark, &err);
        }

        // Body (scrolls, pinned to newest) -----------------------------------
        let chat_key = fsm
            .data
            .selected_chat
            .map(|p| format!("{:?}", p.id))
            .unwrap_or_default();
        let just_opened = self.open_chat.as_deref() != Some(chat_key.as_str());
        self.open_chat = Some(chat_key.clone());

        let mut play_msg: Option<i32> = None;
        let mut load_older = false;
        let highlight = fsm.data.highlight_message;
        let highlight_active = highlight.is_some()
            && fsm
                .data
                .highlight_until
                .map(|t| std::time::Instant::now() < t)
                .unwrap_or(false);
        egui::CentralPanel::default()
            .frame(egui::Frame::NONE.inner_margin(Margin::symmetric(12, 0)))
            .show(ui, |ui| {
                if fsm.data.messages.is_empty() {
                    Self::empty_state(ui, "No messages", true);
                    return;
                }
                egui::ScrollArea::vertical()
                    .id_salt(("tg_messages", &chat_key))
                    .auto_shrink([false, false])
                    .max_height(10000.0)
                    .stick_to_bottom(true)
                    .show(ui, |ui| {
                        let scroll_target = fsm.data.search_target_message;
                        if fsm.data.has_more_messages {
                            if Self::load_more_button(ui, "Load older messages").clicked() {
                                load_older = true;
                            }
                            ui.add_space(8.0);
                        }

                        let messages = &fsm.data.messages;
                        for (i, msg) in messages.iter().enumerate() {
                            let is_me = msg.sender_is_self;
                            let starts_group = i == 0
                                || messages[i.saturating_sub(1)].sender != msg.sender
                                || messages[i.saturating_sub(1)].sender_is_self != is_me;
                            if starts_group && !msg.sender.is_empty() {
                                ui.horizontal(|ui| {
                                    ui.label(RichText::new(&msg.sender).size(12.0).color(C_MUTED));
                                    ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                                        ui.label(
                                            RichText::new(&msg.time).size(11.0).color(C_MUTED),
                                        );
                                    });
                                });
                                ui.add_space(3.0);
                            }

                            let status = if fsm.data.current_video == Some(msg.id) {
                                if fsm.data.loading_video {
                                    VideoStatus::Loading
                                } else {
                                    VideoStatus::Playing
                                }
                            } else {
                                VideoStatus::Ready
                            };
                            let before = ui.min_rect();
                            if Self::message_bubble(ui, msg, status) {
                                play_msg = Some(msg.id);
                            }
                            if highlight_active && highlight == Some(msg.id) {
                                let after = ui.min_rect();
                                let rect = egui::Rect::from_min_max(before.min, after.max);
                                ui.painter().rect_stroke(
                                    rect,
                                    8.0,
                                    egui::Stroke::new(
                                        2.0,
                                        egui::Color32::from_rgb(0x9b, 0x59, 0xb6),
                                    ),
                                    egui::StrokeKind::Outside,
                                );
                            }
                            if scroll_target == Some(msg.id) {
                                ui.scroll_to_cursor(Some(Align::Center));
                            }

                            let next_same_group = messages.get(i + 1).is_some_and(|n| {
                                n.sender == msg.sender && n.sender_is_self == msg.sender_is_self
                            });
                            ui.add_space(if next_same_group { 3.0 } else { 12.0 });
                        }
                    });

                if just_opened {
                    ui.scroll_to_cursor(Some(Align::BOTTOM));
                }
                if highlight_active {
                    ui.ctx().request_repaint();
                }
            });

        if let Some(until) = fsm.data.highlight_until {
            if std::time::Instant::now() >= until {
                unsafe { &mut fsm.inner_mut().data }.highlight_message = None;
                unsafe { &mut fsm.inner_mut().data }.highlight_until = None;
            }
        }
        if fsm.data.search_target_message.is_some() {
            unsafe { &mut fsm.inner_mut().data }.search_target_message = None;
        }

        if back {
            fsm.handle(&TelegramEvent::BackToChatList);
            Self::send(bg, BgCommand::BackToChatList);
        }
        if let Some(id) = play_msg {
            fsm.handle(&TelegramEvent::VideoLoading(id));
            Self::send(bg, BgCommand::PlayVideo(id));
        }
        if load_older {
            Self::send(bg, BgCommand::LoadMoreMessages);
        }
    }

    // ------------------------------------------------------------- components

    /// A chat-list row: avatar with initials, name and preview line, with
    /// accent-tinted hover/selection fills (macOS Messages style).
    fn chat_row(ui: &mut egui::Ui, dialog: &DialogInfo, selected: bool) -> egui::Response {
        let (rect, response) = ui.allocate_exact_size(
            egui::vec2(ui.available_width(), CHAT_ROW_HEIGHT),
            Sense::click(),
        );

        if ui.is_rect_visible(rect) {
            let fill = if selected {
                C_RAISED2
            } else if response.hovered() {
                C_RAISED
            } else {
                Color32::TRANSPARENT
            };
            ui.painter().rect_filled(rect, CornerRadius::same(8), fill);

            let avatar = egui::Rect::from_center_size(
                egui::pos2(
                    rect.left() + ROW_PADDING + AVATAR_SIZE / 2.0,
                    rect.center().y,
                ),
                egui::Vec2::splat(AVATAR_SIZE),
            );
            paint_avatar(ui.painter(), avatar, &dialog.name);

            let text_rect = egui::Rect::from_min_max(
                egui::pos2(avatar.right() + ROW_PADDING, rect.top() + 11.0),
                egui::pos2(rect.right() - ROW_PADDING, rect.bottom() - 9.0),
            );
            // The text is laid out in a non-allocating child so that only the row's
            // own allocation advances the list cursor. An allocating `put`/`scope`
            // would rewind the cursor to the label's bottom and overlap the next row.
            let mut text = ui.new_child(
                UiBuilder::new()
                    .max_rect(text_rect)
                    .layout(Layout::top_down(Align::Min)),
            );
            text.spacing_mut().item_spacing.y = 2.0;
            text.add(
                egui::Label::new(RichText::new(&dialog.name).size(15.0).color(C_TEXT))
                    .selectable(false)
                    .truncate(),
            );
            if let Some(preview) = &dialog.last_message {
                text.add(
                    egui::Label::new(RichText::new(preview).size(12.5).color(C_MUTED))
                        .selectable(false)
                        .truncate(),
                );
            }
        }

        response.on_hover_cursor(egui::CursorIcon::PointingHand)
    }

    /// One message: a filled bubble, left for others and right for your own
    /// messages. Video attachments render as a media card (size/status, title,
    /// quality chips, original file name). Returns `true` when a play button was
    /// clicked.
    fn message_bubble(ui: &mut egui::Ui, msg: &MessageInfo, status: VideoStatus) -> bool {
        let is_me = msg.sender_is_self;
        let (fill, text_color) = if is_me {
            (Color32::TRANSPARENT, Color32::WHITE)
        } else {
            (C_RAISED, C_TEXT)
        };
        let layout = if is_me {
            Layout::right_to_left(Align::TOP)
        } else {
            Layout::left_to_right(Align::TOP)
        };
        let text = msg.text.trim();
        let has_text = !text.is_empty();
        let mut play_clicked = false;

        ui.with_layout(layout, |ui| {
            let max_width = ui.available_width() * BUBBLE_MAX_WIDTH;
            egui::Frame::new()
                .fill(fill)
                .stroke(egui::Stroke::new(
                    1.0,
                    if is_me { C_ACCENT } else { C_BORDER2 },
                ))
                .corner_radius(CornerRadius::same(12))
                .inner_margin(Margin::symmetric(12, 8))
                .show(ui, |ui| {
                    // The Frame's content ui inherits the bubble's horizontal layout, so
                    // force a top-down stack here — otherwise rows after the first are
                    // placed in a zero-width column against the edge and wrap per char.
                    ui.vertical(|ui| {
                        ui.set_max_width(max_width);
                        if has_text {
                            ui.add(
                                egui::Label::new(RichText::new(text).size(14.0).color(text_color))
                                    .wrap(),
                            );
                        }
                        if msg.has_video {
                            if has_text {
                                ui.add_space(8.0);
                            }
                            if Self::video_card(
                                ui,
                                msg.media.as_ref(),
                                is_me,
                                status,
                                !has_text,
                                max_width < 220.0,
                            ) {
                                play_clicked = true;
                            }
                        }
                    });
                });
        });

        play_clicked
    }

    /// The media card for a video attachment. `show_titles` is false when the
    /// message already has a caption (the caption plays that role; derived title and
    /// subtitle are skipped to avoid repeating the file name).
    fn video_card(
        ui: &mut egui::Ui,
        media: Option<&MediaMeta>,
        is_me: bool,
        status: VideoStatus,
        show_titles: bool,
        compact: bool,
    ) -> bool {
        let text_color = if is_me { Color32::WHITE } else { C_TEXT };
        let secondary = Self::secondary_on(is_me);
        let status_color = if is_me {
            Color32::from_white_alpha(0xDD)
        } else {
            match status {
                VideoStatus::Ready => egui::Color32::from_rgb(0x22, 0xc5, 0x5e),
                VideoStatus::Playing => C_ACCENT,
                VideoStatus::Loading => C_MUTED,
            }
        };
        let mut play_clicked = false;

        // Header: meta + status on the left, Play pill on the right — the same
        // horizontal + right-aligned-child pattern as the sender/time header above.
        ui.horizontal(|ui| {
            let mut meta = Vec::new();
            if let Some(m) = media {
                if let Some(size) = m.size_bytes {
                    meta.push(format_bytes(size));
                }
                if !compact && let Some(dur) = m.duration_secs {
                    meta.push(format_duration(dur));
                }
            }
            if !meta.is_empty() {
                ui.label(
                    RichText::new(meta.join(" · "))
                        .monospace()
                        .size(11.0)
                        .color(secondary),
                );
                ui.add_space(4.0);
            }
            if status == VideoStatus::Loading {
                ui.spinner();
            } else {
                let (rect, _) = ui.allocate_exact_size(egui::vec2(7.0, 7.0), Sense::hover());
                ui.painter().circle_filled(rect.center(), 3.5, status_color);
            }
            ui.label(RichText::new(status.label()).size(11.0).color(status_color));

            if status != VideoStatus::Loading {
                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                    if Self::play_pill(ui, false, compact) {
                        play_clicked = true;
                    }
                });
            }
        });

        let Some(meta) = media else {
            return play_clicked;
        };

        if show_titles {
            ui.add_space(6.0);
            ui.add(
                egui::Label::new(
                    RichText::new(&meta.title)
                        .strong()
                        .size(14.0)
                        .color(text_color),
                )
                .wrap(),
            );
            if let Some(subtitle) = &meta.subtitle {
                ui.add(
                    egui::Label::new(RichText::new(subtitle).size(12.5).color(secondary)).wrap(),
                );
            }
        }

        if !meta.chips.is_empty() {
            ui.add_space(6.0);
            ui.horizontal_wrapped(|ui| {
                ui.spacing_mut().item_spacing = egui::vec2(5.0, 5.0);
                for chip in meta.chips.iter().take(4) {
                    Self::chip(ui, chip);
                }
                if meta.chips.len() > 4 {
                    Self::chip(ui, &format!("+{}", meta.chips.len() - 4));
                }
            });
        }

        if !meta.file_name.is_empty() {
            ui.add_space(5.0);
            ui.add(
                egui::Label::new(
                    RichText::new(&meta.file_name)
                        .monospace()
                        .size(10.5)
                        .color(secondary),
                )
                .truncate(),
            )
            .on_hover_text(&meta.file_name);
        }

        play_clicked
    }

    /// Compact "Play" pill for a media card's header. `compact` drops the label and
    /// shows just the icon, for narrow sidebars.
    fn play_pill(ui: &mut egui::Ui, on_accent: bool, compact: bool) -> bool {
        let (idle, hovered, active) = if on_accent {
            (
                Color32::from_white_alpha(0x22),
                Color32::from_white_alpha(0x3A),
                Color32::from_white_alpha(0x48),
            )
        } else {
            let accent = egui::Color32::BLACK;
            if true {
                (
                    accent,
                    egui::Color32::from_gray(0x2a),
                    egui::Color32::from_gray(0x3a),
                )
            } else {
                (
                    accent,
                    accent.gamma_multiply(0.85),
                    accent.gamma_multiply(0.72),
                )
            }
        };
        let (text, min_size) = if compact {
            (icon_only(ui, ICON_PLAY_ARROW), egui::vec2(30.0, 26.0))
        } else {
            (
                icon_label_colored(ui, ICON_PLAY_ARROW, "Play", Color32::WHITE),
                egui::vec2(58.0, 26.0),
            )
        };

        ui.scope(|ui| {
            let widgets = &mut ui.style_mut().visuals.widgets;
            for (widget, fill) in [
                (&mut widgets.inactive, idle),
                (&mut widgets.hovered, hovered),
                (&mut widgets.active, active),
            ] {
                widget.weak_bg_fill = fill;
                widget.fg_stroke.color = Color32::WHITE;
            }
            ui.add(
                egui::Button::new(text)
                    .corner_radius(CornerRadius::same(6))
                    .min_size(min_size),
            )
        })
        .inner
        .on_hover_text("Play video")
        .clicked()
    }

    /// A rounded, bordered metadata chip (quality/encoding tag).
    fn chip(ui: &mut egui::Ui, label: &str) {
        egui::Frame::new()
            .fill(C_RAISED)
            .stroke(egui::Stroke::new(1.0, C_BORDER2))
            .corner_radius(CornerRadius::same(6))
            .inner_margin(Margin::symmetric(5, 2))
            .show(ui, |ui| {
                ui.style_mut().wrap_mode = Some(egui::TextWrapMode::Extend);
                ui.label(RichText::new(label).monospace().size(10.5).color(C_TEXT));
            });
    }

    /// Secondary text that stays readable on the accent (own-message) fill.
    fn secondary_on(on_accent: bool) -> Color32 {
        if on_accent {
            Color32::from_white_alpha(0xB8)
        } else {
            C_MUTED
        }
    }

    /// Filled accent button (white text) for a screen's primary action.
    fn primary_button(
        ui: &mut egui::Ui,
        icon: MaterialIcon,
        label: &str,
        enabled: bool,
    ) -> egui::Response {
        let fill = if enabled {
            C_ACCENT
        } else {
            C_ACCENT.gamma_multiply(0.35)
        };
        let width = ui.available_width();
        ui.add_enabled(
            enabled,
            egui::Button::new(icon_label_colored(ui, icon, label, Color32::WHITE))
                .fill(fill)
                .corner_radius(CornerRadius::same(6))
                .min_size(egui::vec2(width, 34.0)),
        )
    }

    /// Borderless, secondary-styled button for low-emphasis actions.
    fn ghost_button(ui: &mut egui::Ui, icon: MaterialIcon, label: &str) -> egui::Response {
        ui.add(egui::Button::new(icon_label(ui, icon, label)).frame(false))
    }

    /// Full-width borderless button used to page in more items.
    fn load_more_button(ui: &mut egui::Ui, label: &str) -> egui::Response {
        let width = ui.available_width();
        ui.add(
            egui::Button::new(icon_label(ui, ICON_EXPAND_MORE, label))
                .frame(false)
                .min_size(egui::vec2(width, 30.0)),
        )
    }

    /// Inline, soft-tinted error message.
    fn error_banner(ui: &mut egui::Ui, dark: bool, message: &str) {
        let (fill, text) = if dark {
            (
                Color32::from_rgb(58, 26, 26),
                Color32::from_rgb(255, 168, 168),
            )
        } else {
            (
                Color32::from_rgb(253, 233, 233),
                Color32::from_rgb(164, 34, 34),
            )
        };
        egui::Frame::new()
            .fill(fill)
            .corner_radius(CornerRadius::same(6))
            .inner_margin(Margin::symmetric(10, 8))
            .show(ui, |ui| {
                ui.set_width(ui.available_width());
                ui.label(RichText::new(message).size(12.5).color(text));
            });
    }

    /// Placeholder shown when a list has nothing in it (yet).
    fn empty_state(ui: &mut egui::Ui, message: &str, spinner: bool) {
        ui.vertical_centered(|ui| {
            ui.add_space(ui.available_height() * 0.3);
            if spinner {
                ui.spinner();
                ui.add_space(10.0);
            }
            ui.label(RichText::new(message).size(13.0).color(C_MUTED));
        });
    }

    /// Labelled single-line text field, full width.
    fn labeled_field(
        ui: &mut egui::Ui,
        label: &str,
        hint: &'static str,
        password: bool,
        value: &mut String,
    ) -> egui::Response {
        ui.label(RichText::new(label).size(12.0).color(C_MUTED));
        ui.add(
            egui::TextEdit::singleline(value)
                .hint_text(hint)
                .password(password)
                .desired_width(f32::INFINITY)
                .margin(Margin::symmetric(10, 8)),
        )
    }
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
