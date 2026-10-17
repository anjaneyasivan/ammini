//! Bundled icon assets for the newplayer UI.
#![allow(dead_code)]

pub const ICON_KL_ARROW_LEFT_FILL: (&'static str, &'static [u8]) = (
    "kl-arrow-left-fill.svg",
    include_bytes!("../assets/icons/newplayer/kl-arrow-left-fill.svg"),
);
pub const ICON_KL_CHECK_FILL: (&'static str, &'static [u8]) = (
    "kl-check-fill.svg",
    include_bytes!("../assets/icons/newplayer/kl-check-fill.svg"),
);
pub const ICON_KL_CHEVRON_DOWN_FILL: (&'static str, &'static [u8]) = (
    "kl-chevron-down-fill.svg",
    include_bytes!("../assets/icons/newplayer/kl-chevron-down-fill.svg"),
);
pub const ICON_KL_CIRCLE_MENU_FILL: (&'static str, &'static [u8]) = (
    "kl-circle-menu-fill.svg",
    include_bytes!("../assets/icons/newplayer/kl-circle-menu-fill.svg"),
);
pub const ICON_KL_FAST_FORWARD_FILL: (&'static str, &'static [u8]) = (
    "kl-fast-forward-fill.svg",
    include_bytes!("../assets/icons/newplayer/kl-fast-forward-fill.svg"),
);
pub const ICON_KL_FULLSCREEN_EXIT_FILL: (&'static str, &'static [u8]) = (
    "kl-fullscreen-exit-fill.svg",
    include_bytes!("../assets/icons/newplayer/kl-fullscreen-exit-fill.svg"),
);
pub const ICON_KL_FULLSCREEN_FILL: (&'static str, &'static [u8]) = (
    "kl-fullscreen-fill.svg",
    include_bytes!("../assets/icons/newplayer/kl-fullscreen-fill.svg"),
);
pub const ICON_KL_HISTORY_FILL: (&'static str, &'static [u8]) = (
    "kl-history-fill.svg",
    include_bytes!("../assets/icons/newplayer/kl-history-fill.svg"),
);
pub const ICON_KL_INFO_FILL: (&'static str, &'static [u8]) = (
    "kl-info-fill.svg",
    include_bytes!("../assets/icons/newplayer/kl-info-fill.svg"),
);
pub const ICON_KL_INFO: (&'static str, &'static [u8]) = (
    "kl-info.svg",
    include_bytes!("../assets/icons/newplayer/kl-info.svg"),
);
pub const ICON_KL_LAYERS_FILL: (&'static str, &'static [u8]) = (
    "kl-layers-fill.svg",
    include_bytes!("../assets/icons/newplayer/kl-layers-fill.svg"),
);
pub const ICON_KL_LOG_IN_FILL: (&'static str, &'static [u8]) = (
    "kl-log-in-fill.svg",
    include_bytes!("../assets/icons/newplayer/kl-log-in-fill.svg"),
);
pub const ICON_KL_LOG_OUT_FILL: (&'static str, &'static [u8]) = (
    "kl-log-out-fill.svg",
    include_bytes!("../assets/icons/newplayer/kl-log-out-fill.svg"),
);
pub const ICON_KL_LOGOUT: (&'static str, &'static [u8]) = (
    "kl-logout.svg",
    include_bytes!("../assets/icons/newplayer/kl-logout.svg"),
);
pub const ICON_KL_MESSAGE_FILL: (&'static str, &'static [u8]) = (
    "kl-message-fill.svg",
    include_bytes!("../assets/icons/newplayer/kl-message-fill.svg"),
);
pub const ICON_KL_MUSIC_NOTE_FILL: (&'static str, &'static [u8]) = (
    "kl-music-note-fill.svg",
    include_bytes!("../assets/icons/newplayer/kl-music-note-fill.svg"),
);
pub const ICON_KL_PAUSE_FILL: (&'static str, &'static [u8]) = (
    "kl-pause-fill.svg",
    include_bytes!("../assets/icons/newplayer/kl-pause-fill.svg"),
);
pub const ICON_KL_PLAY_FILL: (&'static str, &'static [u8]) = (
    "kl-play-fill.svg",
    include_bytes!("../assets/icons/newplayer/kl-play-fill.svg"),
);
pub const ICON_KL_REFRESH_CW_FILL: (&'static str, &'static [u8]) = (
    "kl-refresh-cw-fill.svg",
    include_bytes!("../assets/icons/newplayer/kl-refresh-cw-fill.svg"),
);
pub const ICON_KL_REWIND_FILL: (&'static str, &'static [u8]) = (
    "kl-rewind-fill.svg",
    include_bytes!("../assets/icons/newplayer/kl-rewind-fill.svg"),
);
pub const ICON_KL_SEND_FILL: (&'static str, &'static [u8]) = (
    "kl-send-fill.svg",
    include_bytes!("../assets/icons/newplayer/kl-send-fill.svg"),
);
pub const ICON_KL_SETTINGS_FILL: (&'static str, &'static [u8]) = (
    "kl-settings-fill.svg",
    include_bytes!("../assets/icons/newplayer/kl-settings-fill.svg"),
);
pub const ICON_KL_SETTINGS: (&'static str, &'static [u8]) = (
    "kl-settings.svg",
    include_bytes!("../assets/icons/newplayer/kl-settings.svg"),
);
pub const ICON_KL_SUBTITLES_FILL: (&'static str, &'static [u8]) = (
    "kl-subtitles-fill.svg",
    include_bytes!("../assets/icons/newplayer/kl-subtitles-fill.svg"),
);
pub const ICON_KL_VOLUME_FILL: (&'static str, &'static [u8]) = (
    "kl-volume-fill.svg",
    include_bytes!("../assets/icons/newplayer/kl-volume-fill.svg"),
);
pub const ICON_KL_VOLUME_OFF_FILL: (&'static str, &'static [u8]) = (
    "kl-volume-off-fill.svg",
    include_bytes!("../assets/icons/newplayer/kl-volume-off-fill.svg"),
);

/// Helper to build an egui Image from a bundled icon.
pub fn image(icon: (&'static str, &'static [u8]), size: f32) -> eframe::egui::Image<'static> {
    eframe::egui::Image::from_bytes(icon.0, icon.1)
        .fit_to_exact_size(eframe::egui::vec2(size, size))
}
