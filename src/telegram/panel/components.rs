use eframe::egui::{
    self, Align, Color32, CornerRadius, Layout, Margin, RichText, Sense, UiBuilder,
};

use crate::telegram::media_meta::{format_bytes, format_duration};
use crate::telegram::{DialogInfo, MediaMeta, MessageInfo};

use super::*;

impl TelegramPanel {
    pub(crate) fn chat_row(
        ui: &mut egui::Ui,
        dialog: &DialogInfo,
        selected: bool,
    ) -> egui::Response {
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
    pub(crate) fn message_bubble(
        ui: &mut egui::Ui,
        msg: &MessageInfo,
        status: VideoStatus,
    ) -> bool {
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
    pub(crate) fn video_card(
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
    pub(crate) fn play_pill(ui: &mut egui::Ui, on_accent: bool, compact: bool) -> bool {
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
        let play_icon = egui::Image::from_bytes("play.svg", crate::icons::ICON_KL_PLAY_FILL.1)
            .fit_to_exact_size(egui::vec2(16.0, 16.0));
        let min_size = if compact {
            egui::vec2(30.0, 26.0)
        } else {
            egui::vec2(58.0, 26.0)
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
            if compact {
                ui.add(
                    egui::Button::image(play_icon)
                        .corner_radius(CornerRadius::same(6))
                        .min_size(min_size),
                )
            } else {
                ui.add(
                    egui::Button::image_and_text(
                        play_icon,
                        egui::RichText::new("Play").color(Color32::WHITE),
                    )
                    .corner_radius(CornerRadius::same(6))
                    .min_size(min_size),
                )
            }
        })
        .inner
        .on_hover_text("Play video")
        .clicked()
    }

    /// A rounded, bordered metadata chip (quality/encoding tag).
    pub(crate) fn chip(ui: &mut egui::Ui, label: &str) {
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
    pub(crate) fn secondary_on(on_accent: bool) -> Color32 {
        if on_accent {
            Color32::from_white_alpha(0xB8)
        } else {
            C_MUTED
        }
    }

    /// Filled accent button (white text) for a screen's primary action.
    pub(crate) fn primary_button(
        ui: &mut egui::Ui,
        icon: &'static [u8],
        label: &str,
        enabled: bool,
    ) -> egui::Response {
        let fill = if enabled {
            C_ACCENT
        } else {
            C_ACCENT.gamma_multiply(0.35)
        };
        let width = ui.available_width();
        let image = egui::Image::from_bytes(format!("{:p}.svg", icon), icon)
            .fit_to_exact_size(egui::vec2(16.0, 16.0));
        ui.add_enabled(
            enabled,
            egui::Button::image_and_text(image, egui::RichText::new(label).color(Color32::WHITE))
                .fill(fill)
                .corner_radius(CornerRadius::same(6))
                .min_size(egui::vec2(width, 34.0)),
        )
    }

    /// Borderless, secondary-styled button for low-emphasis actions.
    pub(crate) fn ghost_button(
        ui: &mut egui::Ui,
        icon: &'static [u8],
        label: &str,
    ) -> egui::Response {
        let image = egui::Image::from_bytes(format!("{:p}.svg", icon), icon)
            .fit_to_exact_size(egui::vec2(16.0, 16.0));
        ui.add(egui::Button::image_and_text(image, label).frame(false))
    }

    /// Full-width borderless button used to page in more items.
    pub(crate) fn load_more_button(ui: &mut egui::Ui, label: &str) -> egui::Response {
        let width = ui.available_width();
        let image =
            egui::Image::from_bytes("expand.svg", crate::icons::ICON_KL_CHEVRON_DOWN_FILL.1)
                .fit_to_exact_size(egui::vec2(16.0, 16.0));
        ui.add(
            egui::Button::image_and_text(image, label)
                .frame(false)
                .min_size(egui::vec2(width, 30.0)),
        )
    }

    /// Inline, soft-tinted error message.
    pub(crate) fn error_banner(ui: &mut egui::Ui, dark: bool, message: &str) {
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
    pub(crate) fn empty_state(ui: &mut egui::Ui, message: &str, spinner: bool) {
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
    pub(crate) fn labeled_field(
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
