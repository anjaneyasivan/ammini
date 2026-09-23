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
use crate::telegram::state_machine::{TelegramData, TelegramEvent, TelegramFsm, TelegramState};
use crate::telegram::{BgCommand, DialogInfo, MessageInfo};
use statig::blocking::StateMachine;

/// Height of a chat-list row (avatar + two lines of text).
const CHAT_ROW_HEIGHT: f32 = 64.0;
/// Avatar diameter in the chat list.
const AVATAR_SIZE: f32 = 40.0;
/// Horizontal padding inside a chat row.
const ROW_PADDING: f32 = 12.0;
/// Widest a message bubble may grow, as a fraction of the conversation width.
const BUBBLE_MAX_WIDTH: f32 = 0.72;

/// Sidebar widget for the Telegram chat UI.
pub struct TelegramPanel {
    /// Peer key of the conversation currently open. A change between frames
    /// means a chat was just opened, which triggers a one-shot scroll to the
    /// newest message.
    open_chat: Option<String>,
}

impl TelegramPanel {
    pub fn new() -> Self {
        Self { open_chat: None }
    }

    pub fn ui(
        &mut self,
        ui: &mut egui::Ui,
        fsm: &mut StateMachine<TelegramFsm>,
        bg: &Option<UnboundedSender<BgCommand>>,
    ) {
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
        let dark = style::is_dark(ui.ctx());
        ui.vertical_centered(|ui| {
            ui.add_space(ui.available_height() * 0.35);
            ui.spinner();
            ui.add_space(10.0);
            ui.label(
                RichText::new("Connecting to Telegram…")
                    .size(13.0)
                    .color(style::text_secondary(dark)),
            );
        });
    }

    fn auth_ui(
        &mut self,
        ui: &mut egui::Ui,
        fsm: &mut StateMachine<TelegramFsm>,
        bg: &Option<UnboundedSender<BgCommand>>,
    ) {
        let dark = style::is_dark(ui.ctx());
        ui.add_space(10.0);
        ui.label(RichText::new("Sign in").strong().size(17.0));
        ui.add_space(3.0);
        ui.label(
            RichText::new("Enter the phone number linked to your Telegram account.")
                .size(12.5)
                .color(style::text_secondary(dark)),
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
        let dark = style::is_dark(ui.ctx());
        ui.add_space(10.0);
        ui.label(RichText::new("Verification").strong().size(17.0));
        ui.add_space(3.0);
        ui.label(
            RichText::new("Enter the code Telegram sent to your app.")
                .size(12.5)
                .color(style::text_secondary(dark)),
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
        let dark = style::is_dark(ui.ctx());
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
                    .color(style::text_secondary(dark)),
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
        let dark = style::is_dark(ui.ctx());
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
        // Header ------------------------------------------------------------
        ui.add_space(10.0);
        ui.horizontal(|ui| {
            ui.add_space(2.0);
            ui.label(RichText::new("Chats").strong().size(18.0));
        });
        ui.add_space(8.0);
        ui.separator();

        // Footer (pinned) ----------------------------------------------------
        let mut sign_out = false;
        egui::Panel::bottom("tg_chats_footer")
            .frame(egui::Frame::NONE)
            .show(ui, |ui| {
                ui.add_space(6.0);
                ui.separator();
                ui.add_space(2.0);
                ui.horizontal(|ui| {
                    ui.add_space(2.0);
                    if Self::ghost_button(ui, ICON_LOGOUT, "Sign out").clicked() {
                        sign_out = true;
                    }
                });
                ui.add_space(4.0);
            });

        // Body (scrolls) -----------------------------------------------------
        let mut selected = None;
        let mut load_more = false;
        egui::CentralPanel::no_frame().show(ui, |ui| {
            if fsm.data.dialogs.is_empty() {
                Self::empty_state(ui, "No chats yet", true);
                return;
            }
            egui::ScrollArea::vertical()
                .id_salt("tg_chat_list")
                .auto_shrink([false, false])
                .show(ui, |ui| {
                    for dialog in &fsm.data.dialogs {
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
        let dark = style::is_dark(ui.ctx());

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
        egui::CentralPanel::no_frame().show(ui, |ui| {
            if fsm.data.messages.is_empty() {
                Self::empty_state(ui, "No messages", true);
                return;
            }
            egui::ScrollArea::vertical()
                .id_salt(("tg_messages", &chat_key))
                .auto_shrink([false, false])
                .stick_to_bottom(true)
                .show(ui, |ui| {
                    if fsm.data.has_more_messages {
                        if Self::load_more_button(ui, "Load older messages").clicked() {
                            load_older = true;
                        }
                        ui.add_space(8.0);
                    }

                    let messages = &fsm.data.messages;
                    for (i, msg) in messages.iter().enumerate() {
                        let is_me = msg.sender_is_self;
                        // Group consecutive messages from one sender: name and time
                        // appear only at the top of a group.
                        let starts_group = i == 0
                            || messages[i - 1].sender != msg.sender
                            || messages[i - 1].sender_is_self != is_me;
                        if starts_group && !msg.sender.is_empty() {
                            ui.horizontal(|ui| {
                                ui.label(
                                    RichText::new(&msg.sender)
                                        .size(12.0)
                                        .color(style::text_secondary(dark)),
                                );
                                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                                    ui.label(
                                        RichText::new(&msg.time)
                                            .size(11.0)
                                            .color(style::text_secondary(dark)),
                                    );
                                });
                            });
                            ui.add_space(3.0);
                        }

                        let loading =
                            fsm.data.loading_video && fsm.data.current_video == Some(msg.id);
                        if Self::message_bubble(ui, msg, dark, loading) {
                            play_msg = Some(msg.id);
                        }

                        // Tight spacing inside a group, breathing room between groups.
                        let next_same_group = messages.get(i + 1).is_some_and(|n| {
                            n.sender == msg.sender && n.sender_is_self == msg.sender_is_self
                        });
                        ui.add_space(if next_same_group { 3.0 } else { 12.0 });
                    }

                    if just_opened {
                        // Land the newest message at the viewport bottom; clamps to
                        // the real content bounds, so it can't leave bad offsets.
                        ui.scroll_to_cursor(Some(Align::BOTTOM));
                    }
                });
        });

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
            let dark = style::is_dark(ui.ctx());
            let fill = if selected {
                style::selected_fill()
            } else if response.hovered() {
                style::hover_fill()
            } else {
                Color32::TRANSPARENT
            };
            ui.painter()
                .rect_filled(rect, CornerRadius::same(style::CONTAINER_RADIUS), fill);

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
                egui::Label::new(
                    RichText::new(&dialog.name)
                        .size(15.0)
                        .color(style::text_primary(dark)),
                )
                .truncate(),
            );
            if let Some(preview) = &dialog.last_message {
                text.add(
                    egui::Label::new(
                        RichText::new(preview)
                            .size(12.5)
                            .color(style::text_secondary(dark)),
                    )
                    .truncate(),
                );
            }
        }

        response.on_hover_cursor(egui::CursorIcon::PointingHand)
    }

    /// One message: a filled bubble, left for others and right for your own
    /// messages. Returns `true` when its play button was clicked.
    fn message_bubble(ui: &mut egui::Ui, msg: &MessageInfo, dark: bool, loading: bool) -> bool {
        let is_me = msg.sender_is_self;
        let (fill, text_color) = if is_me {
            (style::ACCENT, Color32::WHITE)
        } else {
            (style::bubble_other(dark), style::text_primary(dark))
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
                .corner_radius(CornerRadius::same(style::BUBBLE_RADIUS))
                .inner_margin(Margin::symmetric(12, 8))
                .show(ui, |ui| {
                    ui.set_max_width(max_width);
                    if has_text {
                        ui.add(
                            egui::Label::new(RichText::new(text).size(14.0).color(text_color))
                                .wrap(),
                        );
                    }
                    if msg.has_video {
                        if has_text {
                            ui.add_space(6.0);
                        }
                        if loading {
                            ui.horizontal(|ui| {
                                ui.spinner();
                                ui.label(
                                    RichText::new("Loading video…").size(13.0).color(text_color),
                                );
                            });
                        } else if Self::play_button(ui, is_me) {
                            play_clicked = true;
                        }
                    }
                });
        });

        play_clicked
    }

    /// Full-width play button for a video bubble. The fill is set per interact
    /// state in a scoped style, because a plain `Button` repaints a static
    /// `.fill()` on hover.
    fn play_button(ui: &mut egui::Ui, on_accent: bool) -> bool {
        let (idle, hovered, active) = if on_accent {
            (
                Color32::from_white_alpha(0x22),
                Color32::from_white_alpha(0x3A),
                Color32::from_white_alpha(0x48),
            )
        } else {
            let accent = style::ACCENT;
            if style::is_dark(ui.ctx()) {
                (
                    accent,
                    accent.gamma_multiply(1.15),
                    accent.gamma_multiply(1.25),
                )
            } else {
                (
                    accent,
                    accent.gamma_multiply(0.85),
                    accent.gamma_multiply(0.72),
                )
            }
        };
        let width = ui.available_width();

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
                egui::Button::new(icon_label_colored(
                    ui,
                    ICON_PLAY_ARROW,
                    "Play",
                    Color32::WHITE,
                ))
                .corner_radius(CornerRadius::same(style::WIDGET_RADIUS))
                .min_size(egui::vec2(width, 32.0)),
            )
        })
        .inner
        .on_hover_text("Play video")
        .clicked()
    }

    /// Filled accent button (white text) for a screen's primary action.
    fn primary_button(
        ui: &mut egui::Ui,
        icon: MaterialIcon,
        label: &str,
        enabled: bool,
    ) -> egui::Response {
        let fill = if enabled {
            style::ACCENT
        } else {
            style::ACCENT.gamma_multiply(0.35)
        };
        let width = ui.available_width();
        ui.add_enabled(
            enabled,
            egui::Button::new(icon_label_colored(ui, icon, label, Color32::WHITE))
                .fill(fill)
                .corner_radius(CornerRadius::same(style::WIDGET_RADIUS))
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
            .corner_radius(CornerRadius::same(style::WIDGET_RADIUS))
            .inner_margin(Margin::symmetric(10, 8))
            .show(ui, |ui| {
                ui.set_width(ui.available_width());
                ui.label(RichText::new(message).size(12.5).color(text));
            });
    }

    /// Placeholder shown when a list has nothing in it (yet).
    fn empty_state(ui: &mut egui::Ui, message: &str, spinner: bool) {
        let dark = style::is_dark(ui.ctx());
        ui.vertical_centered(|ui| {
            ui.add_space(ui.available_height() * 0.3);
            if spinner {
                ui.spinner();
                ui.add_space(10.0);
            }
            ui.label(
                RichText::new(message)
                    .size(13.0)
                    .color(style::text_secondary(dark)),
            );
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
        let dark = style::is_dark(ui.ctx());
        ui.label(
            RichText::new(label)
                .size(12.0)
                .color(style::text_secondary(dark)),
        );
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
