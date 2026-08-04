use eframe::egui;
use tokio::sync::mpsc::UnboundedSender;

use crate::telegram::BgCommand;
use crate::telegram::state_machine::{TelegramData, TelegramEvent, TelegramFsm, TelegramState};
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
            if ui.button("Retry").clicked() {
                fsm.handle(&TelegramEvent::ResetAuth);
            }
            ui.separator();
        }

        if let Some(err) = &fsm.data.error {
            if !err.trim().is_empty() {
                ui.colored_label(egui::Color32::RED, err);
                ui.separator();
            }
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
        if ui.button("Send Code").clicked() && !phone.is_empty() {
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
        if let Some(err) = &fsm.data.error {
            if !err.trim().is_empty() {
                ui.colored_label(egui::Color32::RED, err);
            }
        }
        let code = {
            let data = Self::data_mut(fsm);
            ui.horizontal(|ui| {
                ui.label("Code:");
                ui.text_edit_singleline(&mut data.code);
            });
            data.code.trim().to_string()
        };
        if ui.button("Verify").clicked() && !code.is_empty() {
            fsm.handle(&TelegramEvent::CodeSubmitted(code.clone()));
            Self::send(bg, BgCommand::SubmitCode(code));
        }
        if ui.button("← Change phone number").clicked() {
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
        if let Some(hint) = &fsm.data.password_hint {
            if !hint.is_empty() {
                ui.label(format!("Hint: {hint}"));
            }
        }
        if let Some(err) = &fsm.data.error {
            if !err.trim().is_empty() {
                ui.colored_label(egui::Color32::RED, err);
            }
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
        if ui.button("Sign In").clicked() && !password.is_empty() {
            fsm.handle(&TelegramEvent::PasswordSubmitted(display_pw));
            Self::send(bg, BgCommand::SubmitPassword(password));
        }
        if ui.button("← Change phone number").clicked() {
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
                    ui.horizontal(|ui| {
                        let response = ui.selectable_label(false, &dialog.name);
                        if response.clicked() {
                            clicked = Some(dialog.peer_ref);
                        }
                    });
                    if let Some(preview) = &dialog.last_message {
                        ui.label(egui::RichText::new(preview).weak().small());
                    }
                    ui.separator();
                }
                if data.has_more_dialogs && ui.button("Load more chats").clicked() {
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
        if ui.button("Sign out").clicked() {
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
            if ui.button("← Back").clicked() {
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

        if let Some(err) = &fsm.data.error {
            if !err.trim().is_empty() {
                ui.colored_label(egui::Color32::RED, err);
                ui.separator();
            }
        }

        let mut clicked_msg_id: Option<i32> = None;
        let mut load_more_clicked = false;
        egui::ScrollArea::vertical().show(ui, |ui| {
            // Older messages load above the current ones, so the button lives at the top.
            if fsm.data.has_more_messages {
                if ui.button("Load older messages").clicked() {
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
                    } else if ui.button("▶ Play").clicked() {
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
