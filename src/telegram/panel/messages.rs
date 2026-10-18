use eframe::egui::{
    self, Align, Align2, Color32, CornerRadius, FontId, Layout, Margin, RichText, Sense,
};
use tokio::sync::mpsc::UnboundedSender;

use crate::telegram::BgCommand;
use crate::telegram::state_machine::{TelegramEvent, TelegramFsm};
use statig::blocking::StateMachine;

use super::*;

impl TelegramPanel {
    pub(crate) fn message_list_ui(
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
                .add(
                    egui::Button::image(
                        egui::Image::from_bytes(
                            "back.svg",
                            crate::icons::ICON_KL_ARROW_LEFT_FILL.1,
                        )
                        .fit_to_exact_size(egui::vec2(16.0, 16.0)),
                    )
                    .frame(false),
                )
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

        if fsm.data.message_filter.is_empty() {
            unsafe { &mut fsm.inner_mut().data }.message_filter = "All".to_string();
        }

        // Message filter pills (All / Videos)
        ui.add_space(4.0);
        ui.horizontal(|ui| {
            ui.add_space(2.0);
            for f in ["All", "Videos"] {
                let active = fsm.data.message_filter == f;
                let font = FontId::proportional(10.0);
                let text_width = ui
                    .painter()
                    .layout_no_wrap(f.to_string(), font.clone(), C_TEXT)
                    .size()
                    .x;
                let (rect, resp) =
                    ui.allocate_exact_size(egui::vec2(text_width + 16.0, 20.0), Sense::click());
                let hovered = resp.hovered()
                    || rect.contains(ui.ctx().pointer_hover_pos().unwrap_or(egui::Pos2::ZERO));
                let fill = if active {
                    C_RAISED2
                } else if hovered {
                    C_ACCENT.linear_multiply(0.15)
                } else {
                    Color32::TRANSPARENT
                };
                ui.painter().rect_filled(rect, CornerRadius::same(4), fill);
                ui.painter().text(
                    rect.center(),
                    Align2::CENTER_CENTER,
                    f,
                    font.clone(),
                    if active { C_TEXT } else { C_MUTED },
                );
                if resp.clicked() {
                    unsafe { &mut fsm.inner_mut().data }.message_filter = f.to_string();
                }
                ui.add_space(4.0);
            }
        });
        ui.add_space(4.0);

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
                            if fsm.data.message_filter == "Videos" && !msg.has_video {
                                continue;
                            }
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

        if let Some(until) = fsm.data.highlight_until
            && std::time::Instant::now() >= until
        {
            unsafe { &mut fsm.inner_mut().data }.highlight_message = None;
            unsafe { &mut fsm.inner_mut().data }.highlight_until = None;
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
}
