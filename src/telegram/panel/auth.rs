use eframe::egui::{self, Key, RichText};
use tokio::sync::mpsc::UnboundedSender;

use crate::telegram::BgCommand;
use crate::telegram::state_machine::{TelegramEvent, TelegramFsm};
use statig::blocking::StateMachine;

use super::*;

impl TelegramPanel {
    pub(crate) fn connecting_ui(&self, ui: &mut egui::Ui) {
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

    pub(crate) fn auth_ui(
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
        if Self::primary_button(
            ui,
            crate::icons::ICON_KL_SEND_FILL.1,
            "Send code",
            !phone.is_empty(),
        )
        .clicked()
        {
            submit = true;
        }
        if submit && !phone.is_empty() {
            fsm.handle(&TelegramEvent::PhoneSubmitted(phone.clone()));
            Self::send(bg, BgCommand::SubmitPhone(phone));
        }
    }

    pub(crate) fn code_ui(
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
        if Self::primary_button(
            ui,
            crate::icons::ICON_KL_CHECK_FILL.1,
            "Verify",
            !code.is_empty(),
        )
        .clicked()
        {
            submit = true;
        }
        if submit && !code.is_empty() {
            fsm.handle(&TelegramEvent::CodeSubmitted(code.clone()));
            Self::send(bg, BgCommand::SubmitCode(code));
        }

        ui.add_space(6.0);
        if Self::ghost_button(
            ui,
            crate::icons::ICON_KL_ARROW_LEFT_FILL.1,
            "Change phone number",
        )
        .clicked()
        {
            Self::data_mut(fsm).code.clear();
            fsm.handle(&TelegramEvent::NeedsAuth);
        }
    }

    pub(crate) fn password_ui(
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
        if Self::primary_button(
            ui,
            crate::icons::ICON_KL_LOG_IN_FILL.1,
            "Sign in",
            !password.is_empty(),
        )
        .clicked()
        {
            submit = true;
        }
        if submit && !password.is_empty() {
            fsm.handle(&TelegramEvent::PasswordSubmitted(display));
            Self::send(bg, BgCommand::SubmitPassword(password));
        }

        ui.add_space(6.0);
        if Self::ghost_button(
            ui,
            crate::icons::ICON_KL_ARROW_LEFT_FILL.1,
            "Change phone number",
        )
        .clicked()
        {
            Self::data_mut(fsm).password.clear();
            fsm.handle(&TelegramEvent::NeedsAuth);
        }
    }

    pub(crate) fn error_ui(&self, ui: &mut egui::Ui, fsm: &mut StateMachine<TelegramFsm>) {
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

        if Self::primary_button(
            ui,
            crate::icons::ICON_KL_REFRESH_CW_FILL.1,
            "Try again",
            true,
        )
        .clicked()
        {
            fsm.handle(&TelegramEvent::ResetAuth);
        }
    }
}
