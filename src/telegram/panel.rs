use eframe::egui;
use egui_material_icons::{MaterialIcon, icons::*};
use tokio::sync::mpsc::UnboundedSender;

use crate::fonts::icon_label;
use crate::style;
use crate::telegram::state_machine::{TelegramData, TelegramEvent, TelegramFsm, TelegramState};
use crate::telegram::{BgCommand, DialogInfo};
use statig::blocking::StateMachine;

/// Sidebar widget for the Telegram chat UI.
pub struct TelegramPanel {}

impl TelegramPanel {
    pub fn new() -> Self {
        Self {}
    }

    pub fn ui(
        &mut self,
        ui: &mut egui::Ui,
        fsm: &mut StateMachine<TelegramFsm>,
        bg: &Option<UnboundedSender<BgCommand>>,
    ) {
        ui.heading("Telegram");
        ui.separator();

        let state = fsm.state();
        match state {
            TelegramState::Connecting {} => self.connecting_ui(ui),
            TelegramState::Unauthenticated {} => self.auth_ui(ui, fsm, bg),
            TelegramState::Error {} => self.auth_ui(ui, fsm, bg),
            TelegramState::AwaitingCode {} => self.code_ui(ui, fsm, bg),
            TelegramState::AwaitingPassword {} => self.password_ui(ui, fsm, bg),
            TelegramState::ChatList {} => self.chat_list_ui(ui, fsm, bg),
            TelegramState::MessageList {} => self.message_list_ui(ui, fsm, bg),
        }
    }

    fn send(bg: &Option<UnboundedSender<BgCommand>>, cmd: BgCommand) {
        if let Some(bg) = bg {
            let _ = bg.send(cmd);
        }
    }

    /// Mutable access to the UI data stored inside the state machine.
    /// SAFETY: we only mutate UI fields, not the state machine's invariants.
    fn data_mut(fsm: &mut StateMachine<TelegramFsm>) -> &mut TelegramData {
        unsafe { &mut fsm.inner_mut().data }
    }

    /// Filled accent button (white text) for the primary action of a screen.
    fn primary_button(ui: &mut egui::Ui, icon: MaterialIcon, label: &str) -> egui::Response {
        ui.add(
            egui::Button::new(
                egui::RichText::new(icon_label(icon, label)).color(egui::Color32::WHITE),
            )
            .fill(style::ACCENT),
        )
    }

    /// A 60px chat card: avatar circle with initials, name and a preview line, with
    /// accent-tinted hover/selection fills (macOS Messages style).
    fn chat_row(ui: &mut egui::Ui, dialog: &DialogInfo, selected: bool) -> egui::Response {
        let desired = egui::vec2(ui.available_width(), 60.0);
        let (rect, response) = ui.allocate_exact_size(desired, egui::Sense::click());
        if ui.is_rect_visible(rect) {
            let dark = style::is_dark(ui.ctx());
            let bg = if selected {
                style::selected_fill()
            } else if response.hovered() {
                style::hover_fill()
            } else {
                egui::Color32::TRANSPARENT
            };
            ui.painter()
                .rect_filled(rect, egui::CornerRadius::same(style::CONTAINER_RADIUS), bg);

            let avatar_center = rect.left_center() + egui::vec2(28.0, 0.0);
            ui.painter()
                .circle_filled(avatar_center, 20.0, style::avatar_fill(&dialog.name));
            let initial = dialog
                .name
                .chars()
                .next()
                .unwrap_or('?')
                .to_uppercase()
                .to_string();
            ui.painter().text(
                avatar_center,
                egui::Align2::CENTER_CENTER,
                initial,
                egui::FontId::proportional(15.0),
                egui::Color32::WHITE,
            );

            let text_x = rect.left() + 56.0;
            ui.painter().text(
                egui::pos2(text_x, rect.top() + 12.0),
                egui::Align2::LEFT_TOP,
                &dialog.name,
                egui::FontId::proportional(15.0),
                style::text_primary(dark),
            );
            if let Some(preview) = &dialog.last_message {
                ui.painter().text(
                    egui::pos2(text_x, rect.top() + 34.0),
                    egui::Align2::LEFT_TOP,
                    truncate_preview(preview),
                    egui::FontId::proportional(12.5),
                    style::text_secondary(dark),
                );
            }
        }
        response
    }

    fn connecting_ui(&self, ui: &mut egui::Ui) {
        ui.horizontal(|ui| {
            ui.spinner();
            ui.label("Connecting to Telegram…");
        });
    }

    fn auth_ui(
        &mut self,
        ui: &mut egui::Ui,
        fsm: &mut StateMachine<TelegramFsm>,
        bg: &Option<UnboundedSender<BgCommand>>,
    ) {
        let is_error = matches!(fsm.state(), TelegramState::Error {});
        if is_error {
            ui.label("An error occurred.");
            if ui.button(icon_label(ICON_REFRESH, "Retry")).clicked() {
                fsm.handle(&TelegramEvent::ResetAuth);
            }
            ui.separator();
        }

        if let Some(err) = &fsm.data.error
            && !err.trim().is_empty()
        {
            ui.colored_label(egui::Color32::RED, err);
            ui.separator();
        }

        ui.label("Sign in with your Telegram account");
        let phone = {
            let data = Self::data_mut(fsm);
            ui.horizontal(|ui| {
                ui.label("Phone:");
                ui.text_edit_singleline(&mut data.phone);
            });
            data.phone.trim().to_string()
        };
        if Self::primary_button(ui, ICON_SEND, "Send Code").clicked() && !phone.is_empty() {
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
        ui.label("Enter the code sent to your Telegram app");
        if let Some(err) = &fsm.data.error
            && !err.trim().is_empty()
        {
            ui.colored_label(egui::Color32::RED, err);
        }
        let code = {
            let data = Self::data_mut(fsm);
            ui.horizontal(|ui| {
                ui.label("Code:");
                ui.text_edit_singleline(&mut data.code);
            });
            data.code.trim().to_string()
        };
        if Self::primary_button(ui, ICON_CHECK, "Verify").clicked() && !code.is_empty() {
            fsm.handle(&TelegramEvent::CodeSubmitted(code.clone()));
            Self::send(bg, BgCommand::SubmitCode(code));
        }
        if ui
            .button(icon_label(ICON_ARROW_BACK, "Change phone number"))
            .clicked()
        {
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
        ui.label("Two-factor authentication");
        if let Some(hint) = &fsm.data.password_hint
            && !hint.is_empty()
        {
            ui.label(format!("Hint: {hint}"));
        }
        if let Some(err) = &fsm.data.error
            && !err.trim().is_empty()
        {
            ui.colored_label(egui::Color32::RED, err);
        }
        let (password, display_pw) = {
            let data = Self::data_mut(fsm);
            ui.horizontal(|ui| {
                ui.label("Password:");
                ui.text_edit_singleline(&mut data.password);
            });
            let pw = data.password.clone().into_bytes();
            let display = data.password.clone();
            (pw, display)
        };
        if Self::primary_button(ui, ICON_LOGIN, "Sign In").clicked() && !password.is_empty() {
            fsm.handle(&TelegramEvent::PasswordSubmitted(display_pw));
            Self::send(bg, BgCommand::SubmitPassword(password));
        }
        if ui
            .button(icon_label(ICON_ARROW_BACK, "Change phone number"))
            .clicked()
        {
            Self::data_mut(fsm).password.clear();
            fsm.handle(&TelegramEvent::NeedsAuth);
        }
    }

    fn chat_list_ui(
        &mut self,
        ui: &mut egui::Ui,
        fsm: &mut StateMachine<TelegramFsm>,
        bg: &Option<UnboundedSender<BgCommand>>,
    ) {
        ui.label("Chats");
        ui.separator();

        let (clicked, has_more) = {
            let data = &fsm.data;
            let mut clicked = None;
            let mut has_more_clicked = false;
            egui::ScrollArea::vertical().show(ui, |ui| {
                for dialog in &data.dialogs {
                    let selected = data.selected_chat == Some(dialog.peer_ref);
                    if Self::chat_row(ui, dialog, selected).clicked() {
                        clicked = Some(dialog.peer_ref);
                    }
                }
                if data.has_more_dialogs
                    && ui
                        .button(icon_label(ICON_EXPAND_MORE, "Load more chats"))
                        .clicked()
                {
                    has_more_clicked = true;
                }
            });
            (clicked, has_more_clicked)
        };

        if let Some(peer) = clicked {
            fsm.handle(&TelegramEvent::ChatSelected(peer));
            Self::send(bg, BgCommand::SelectChat(peer));
        }
        if has_more {
            Self::send(bg, BgCommand::LoadMoreDialogs);
        }

        ui.separator();
        if ui
            .add(egui::Button::new(ICON_LOGOUT).frame(false))
            .on_hover_text("Sign out")
            .clicked()
        {
            Self::send(bg, BgCommand::SignOut);
        }
    }

    fn message_list_ui(
        &mut self,
        ui: &mut egui::Ui,
        fsm: &mut StateMachine<TelegramFsm>,
        bg: &Option<UnboundedSender<BgCommand>>,
    ) {
        ui.horizontal(|ui| {
            if ui
                .add(egui::Button::new(ICON_ARROW_BACK).frame(false))
                .on_hover_text("Back")
                .clicked()
            {
                fsm.handle(&TelegramEvent::BackToChatList);
                Self::send(bg, BgCommand::BackToChatList);
            }
            if let Some(name) = &fsm.data.selected_chat_name {
                ui.heading(name);
            } else {
                ui.heading("Chat");
            }
        });
        ui.separator();

        if let Some(err) = &fsm.data.error
            && !err.trim().is_empty()
        {
            ui.colored_label(egui::Color32::RED, err);
            ui.separator();
        }

        let mut clicked_msg_id: Option<i32> = None;
        let mut load_more_clicked = false;
        egui::ScrollArea::vertical().show(ui, |ui| {
            // Older messages load above the current ones, so the button lives at the top.
            if fsm.data.has_more_messages {
                if ui
                    .button(icon_label(ICON_EXPAND_MORE, "Load older messages"))
                    .clicked()
                {
                    load_more_clicked = true;
                }
                ui.separator();
            }
            for msg in &fsm.data.messages {
                ui.horizontal(|ui| {
                    if !msg.sender.is_empty() {
                        ui.label(egui::RichText::new(&msg.sender).strong());
                    }
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        ui.label(egui::RichText::new(&msg.time).weak().small());
                    });
                });
                if !msg.text.is_empty() {
                    ui.label(&msg.text);
                }
                if msg.has_video {
                    let is_loading =
                        fsm.data.loading_video && fsm.data.current_video == Some(msg.id);
                    if is_loading {
                        ui.horizontal(|ui| {
                            ui.spinner();
                            ui.label("Loading video…");
                        });
                    } else if ui.button(icon_label(ICON_PLAY_ARROW, "Play")).clicked() {
                        clicked_msg_id = Some(msg.id);
                    }
                }
                ui.separator();
            }
        });

        if let Some(msg_id) = clicked_msg_id {
            fsm.handle(&TelegramEvent::VideoLoading(msg_id));
            Self::send(bg, BgCommand::PlayVideo(msg_id));
        }
        if load_more_clicked {
            Self::send(bg, BgCommand::LoadMoreMessages);
        }
    }
}

impl Default for TelegramPanel {
    fn default() -> Self {
        Self::new()
    }
}

/// Painter text doesn't wrap; cut long previews at 43 chars with an ellipsis.
fn truncate_preview(text: &str) -> String {
    let mut chars: Vec<char> = text.chars().collect();
    if chars.len() > 46 {
        chars.truncate(43);
        format!("{}…", chars.into_iter().collect::<String>())
    } else {
        text.to_string()
    }
}
