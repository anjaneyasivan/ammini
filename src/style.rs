//! macOS-style global styling: generous spacing, soft radii, borderless widgets, a
//! single accent color, and both light/dark variants resolved from the system theme.

use eframe::egui::{self, Color32, CornerRadius, Stroke};

/// macOS system blue — the app's single accent color, used in both light and dark mode.
pub const ACCENT: Color32 = Color32::from_rgb(0, 122, 255);

/// Radius for buttons, inputs and list rows.
pub const WIDGET_RADIUS: u8 = 8;
/// Radius for larger containers (bubbles, cards).
pub const CONTAINER_RADIUS: u8 = 10;
/// Radius for chat/message bubbles.
pub const BUBBLE_RADIUS: u8 = 14;

/// Apply the global style to both themes and follow the system light/dark appearance.
/// Call once at startup, after the font stack is installed.
pub fn install(ctx: &egui::Context) {
    for theme in [egui::Theme::Light, egui::Theme::Dark] {
        ctx.style_mut_of(theme, |style| {
            // Generous spacing reads as native; egui's defaults are cramped.
            style.spacing.item_spacing = egui::vec2(10.0, 8.0);
            style.spacing.button_padding = egui::vec2(12.0, 6.0);
            style.spacing.window_margin = egui::Margin::same(16);

            // Soft radii everywhere, no hard 1px widget borders.
            for widget in [
                &mut style.visuals.widgets.inactive,
                &mut style.visuals.widgets.hovered,
                &mut style.visuals.widgets.active,
            ] {
                widget.corner_radius = CornerRadius::same(WIDGET_RADIUS);
                widget.bg_stroke = Stroke::NONE;
            }
            style.visuals.widgets.hovered.bg_fill = hover_fill();
            style.visuals.window_corner_radius = CornerRadius::same(12);
            // Selections and list highlights use the accent at low opacity, not a border.
            style.visuals.selection.bg_fill = selected_fill();
            style.visuals.selection.stroke = Stroke::NONE;
        });
    }
    ctx.options_mut(|o| o.theme_preference = egui::ThemePreference::System);
    tracing::info!("style: macOS-style theme installed");
}

/// Whether the system is currently in dark mode (for resolving custom colors).
pub fn is_dark(ctx: &egui::Context) -> bool {
    ctx.theme() == egui::Theme::Dark
}

/// Hover fill: accent at 10%.
pub fn hover_fill() -> Color32 {
    ACCENT.linear_multiply(0.10)
}

/// Selected fill: accent at 20%.
pub fn selected_fill() -> Color32 {
    ACCENT.linear_multiply(0.20)
}

/// Seekbar "cached" indicator fill: a lighter shade of the current rail background,
/// so cached spans read as "available" without clashing with the played fill. Derived
/// from the rail instead of the mode so both light and dark themes stay correct.
pub fn cache_bar_fill(rail_bg: Color32) -> Color32 {
    rail_bg.lerp_to_gamma(Color32::WHITE, 0.35)
}

/// Primary text color for both modes (near-black/near-white, never pure).
pub fn text_primary(dark: bool) -> Color32 {
    if dark {
        Color32::from_rgb(240, 240, 245)
    } else {
        Color32::from_rgb(28, 28, 30)
    }
}

/// Secondary/metadata text color (~60% gray in both modes).
pub fn text_secondary(dark: bool) -> Color32 {
    if dark {
        Color32::from_rgb(165, 165, 175)
    } else {
        Color32::from_rgb(110, 110, 118)
    }
}

/// Neutral bubble fill for other people's messages.
pub fn bubble_other(dark: bool) -> Color32 {
    if dark {
        Color32::from_rgb(44, 44, 46)
    } else {
        Color32::from_rgb(233, 233, 235)
    }
}

/// Outline for a bubble/card border. On an accent bubble a white hairline reads;
/// otherwise it is a faint edge derived from the theme.
pub fn bubble_stroke(on_accent: bool, dark: bool) -> Color32 {
    if on_accent {
        Color32::from_white_alpha(0x33)
    } else if dark {
        Color32::from_white_alpha(0x1A)
    } else {
        Color32::from_black_alpha(0x14)
    }
}

/// Fill for a metadata chip (quality/encoding tag) inside a bubble.
pub fn chip_fill(on_accent: bool, dark: bool) -> Color32 {
    if on_accent {
        Color32::from_white_alpha(0x1E)
    } else if dark {
        Color32::from_white_alpha(0x0F)
    } else {
        Color32::from_black_alpha(0x08)
    }
}

/// Outline for a metadata chip.
pub fn chip_outline(on_accent: bool, dark: bool) -> Color32 {
    if on_accent {
        Color32::from_white_alpha(0x3D)
    } else if dark {
        Color32::from_white_alpha(0x24)
    } else {
        Color32::from_black_alpha(0x1A)
    }
}

/// Text color for a metadata chip.
pub fn chip_text(on_accent: bool, dark: bool) -> Color32 {
    if on_accent {
        Color32::from_white_alpha(0xE0)
    } else {
        text_primary(dark)
    }
}

/// "Ready" status (emerald) for a video card — theme-aware.
pub fn status_ready(dark: bool) -> Color32 {
    if dark {
        Color32::from_rgb(52, 199, 120)
    } else {
        Color32::from_rgb(16, 140, 90)
    }
}

/// Muted avatar background derived from the contact name, so each chat gets its own
/// soft hue while staying desaturated (no saturated colors outside the accent).
pub fn avatar_fill(name: &str) -> Color32 {
    let mut hash: u32 = 2166136261;
    for b in name.bytes() {
        hash ^= u32::from(b);
        hash = hash.wrapping_mul(16777619);
    }
    let hue = (hash % 24) as f32 * 15.0; // 24 discreet hues
    let (r, g, b) = hsv_to_rgb(hue, 0.35, 0.85);
    Color32::from_rgb(r, g, b)
}

/// Minimal HSV → RGB conversion (hue in degrees, s/v in [0, 1]).
fn hsv_to_rgb(hue: f32, s: f32, v: f32) -> (u8, u8, u8) {
    let c = v * s;
    let x = c * (1.0 - ((hue / 60.0) % 2.0 - 1.0).abs());
    let m = v - c;
    let (r, g, b) = match hue as u32 / 60 {
        0 => (c, x, 0.0),
        1 => (x, c, 0.0),
        2 => (0.0, c, x),
        3 => (0.0, x, c),
        4 => (x, 0.0, c),
        _ => (c, 0.0, x),
    };
    (
        ((r + m) * 255.0) as u8,
        ((g + m) * 255.0) as u8,
        ((b + m) * 255.0) as u8,
    )
}
