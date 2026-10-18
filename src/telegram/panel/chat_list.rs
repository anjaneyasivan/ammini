use eframe::egui::{self, Align2, Color32, CornerRadius, FontId, Margin, RichText, Sense};
use tokio::sync::mpsc::UnboundedSender;

use crate::telegram::BgCommand;
use crate::telegram::state_machine::{TelegramEvent, TelegramFsm};
use statig::blocking::StateMachine;

use super::*;

impl TelegramPanel {
    pub(crate) fn chat_list_ui(
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
        if let Some(dl) = self.search_deadline
            && std::time::Instant::now() >= dl
        {
            self.search_deadline = None;
            let q = fsm.data.search_query.clone();
            if !q.trim().is_empty() {
                Self::send(bg, BgCommand::SearchChats(q));
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
                        for f in ["All", "Media", "Channels", "Groups", "Directs", "Unread"] {
                            let active = fsm.data.dialog_filter == f;
                            let font = FontId::proportional(10.0);
                            let text_width = ui
                                .painter()
                                .layout_no_wrap(f.to_string(), font.clone(), C_TEXT)
                                .size()
                                .x;
                            let (rect, resp) = ui.allocate_exact_size(
                                egui::vec2(text_width + 16.0, 20.0),
                                Sense::click(),
                            );
                            let hovered = resp.hovered()
                                || rect.contains(
                                    ui.ctx().pointer_hover_pos().unwrap_or(egui::Pos2::ZERO),
                                );
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
                                unsafe { &mut fsm.inner_mut().data }.dialog_filter = f.to_string();
                            }
                            ui.add_space(4.0);
                        }
                    });
                });
            ui.add_space(6.0);
        }

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
    }
}
