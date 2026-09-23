//! Custom font stack for the app: a modern monochrome emoji font (the egui defaults are
//! years out of date and miss most emojis found in real chats), a bundled DejaVu Sans for
//! symbols/dingbats/arrows the emoji font lacks, plus best-effort system fonts as
//! fallbacks for non-Latin scripts (Devanagari, Arabic, Thai, …).

use std::sync::Arc;

use eframe::egui;
use egui_material_icons::MaterialIcon;

/// Inter (variable), the primary text face — a free San Francisco substitute, per the
/// macOS style guide.
///
/// Source: rsms/inter v4.1, SIL Open Font License 1.1 (see `assets/fonts/OFL-Inter.txt`).
const INTER: &[u8] = include_bytes!("../assets/fonts/InterVariable.ttf");

/// Modern monochrome Noto Emoji (variable weight), covering emojis through Unicode 15+
/// including ZWJ sequences, skin tones and flags.
///
/// Source: google/fonts `ofl/notoemoji`, SIL Open Font License 1.1
/// (see `assets/fonts/OFL-NotoEmoji.txt`).
const NOTO_EMOJI: &[u8] = include_bytes!("../assets/fonts/NotoEmoji-Variable.ttf");

/// DejaVu Sans, bundled for symbols that the emoji font doesn't cover (e.g. Dingbats like
/// `➠`, arrows, check marks). Also acts as a broad Latin/Cyrillic/Greek fallback on every
/// platform, not just Linux.
///
/// Source: dejavu-fonts 2.37, `ttf/DejaVuSans.ttf` (see `assets/fonts/LICENSE-DejaVu.txt`).
const DEJA_VU_SANS: &[u8] = include_bytes!("../assets/fonts/DejaVuSans.ttf");

/// System fonts tried as script fallbacks, one entry per script. macOS entries are kept
/// small (< 5 MB): the big ones (PingFang.ttc ~78 MB, Arial Unicode ~23 MB) are skipped
/// on purpose to keep startup and memory usage light — CJK on macOS is therefore not
/// covered. Each font is loaded only when it exists, so this degrades gracefully on
/// other platforms.
///
/// NOTE: egui panics at startup if any registered font fails to parse, so only add fonts
/// that are known to load; `tests/emoji_support.rs` (which builds a `Fonts` from these
/// definitions) is the safety net for that.
const SYSTEM_FALLBACKS: &[(&str, &str, u32)] = &[
    ("Arabic", "/System/Library/Fonts/GeezaPro.ttc", 0),
    (
        "Devanagari",
        "/System/Library/Fonts/Supplemental/Devanagari Sangam MN.ttc",
        0,
    ),
    (
        "Bengali",
        "/System/Library/Fonts/Supplemental/Bengali Sangam MN.ttc",
        0,
    ),
    (
        "Gurmukhi",
        "/System/Library/Fonts/Supplemental/Gurmukhi Sangam MN.ttc",
        0,
    ),
    (
        "Gujarati",
        "/System/Library/Fonts/Supplemental/Gujarati Sangam MN.ttc",
        0,
    ),
    (
        "Tamil",
        "/System/Library/Fonts/Supplemental/Tamil Sangam MN.ttc",
        0,
    ),
    (
        "Telugu",
        "/System/Library/Fonts/Supplemental/Telugu Sangam MN.ttc",
        0,
    ),
    (
        "Malayalam",
        "/System/Library/Fonts/Supplemental/Malayalam Sangam MN.ttc",
        0,
    ),
    ("Kannada", "/System/Library/Fonts/NotoSansKannada.ttc", 0),
    ("Oriya", "/System/Library/Fonts/NotoSansOriya.ttc", 0),
    ("Myanmar", "/System/Library/Fonts/NotoSansMyanmar.ttc", 0),
    ("Armenian", "/System/Library/Fonts/NotoSansArmenian.ttc", 0),
    (
        "Khmer",
        "/System/Library/Fonts/Supplemental/Khmer Sangam MN.ttf",
        0,
    ),
    ("Lao", "/System/Library/Fonts/Supplemental/Lao MN.ttc", 0),
    ("Thai", "/System/Library/Fonts/Supplemental/Thonburi.ttc", 0),
    // Linux CJK (Noto Sans CJK). Paths vary by distro; only one will exist.
    (
        "NotoCJK",
        "/usr/share/fonts/opentype/noto/NotoSansCJK-Regular.ttc",
        0,
    ),
    (
        "NotoCJKAlt",
        "/usr/share/fonts/truetype/noto/NotoSansCJK-Regular.ttc",
        0,
    ),
];

/// Build the app's font stack on top of the egui defaults. Our emoji font and any system
/// fallbacks that exist on this machine are inserted right after the primary fonts, ahead
/// of egui's (outdated) builtin emoji fonts.
pub fn font_definitions() -> egui::FontDefinitions {
    let mut defs = egui::FontDefinitions::default();

    defs.font_data.insert(
        "NotoEmoji".to_owned(),
        Arc::new(
            egui::FontData::from_static(NOTO_EMOJI).tweak(egui::FontTweak {
                scale: 0.9, // emoji glyphs are drawn to fill the em box; nudge them to text size
                ..Default::default()
            }),
        ),
    );
    defs.font_data.insert(
        "DejaVuSans".to_owned(),
        Arc::new(egui::FontData::from_static(DEJA_VU_SANS)),
    );
    defs.font_data.insert(
        "Inter".to_owned(),
        Arc::new(egui::FontData::from_static(INTER)),
    );

    // DejaVu Sans sits right after the emoji font: it picks up symbols/arrows/dingbats
    // the emoji font doesn't cover, before egui's (outdated) builtins or script fallbacks.
    let mut fallbacks: Vec<String> = vec!["NotoEmoji".to_owned(), "DejaVuSans".to_owned()];
    for (name, path, index) in SYSTEM_FALLBACKS {
        match std::fs::read(path) {
            Ok(bytes) => {
                tracing::debug!("fonts: loaded {name} from {path}");
                defs.font_data.insert(
                    (*name).to_owned(),
                    Arc::new(egui::FontData {
                        index: *index,
                        tweak: Default::default(),
                        font: bytes.into(),
                    }),
                );
                fallbacks.push((*name).to_owned());
            }
            Err(e) => tracing::debug!("fonts: skipping {name} ({path}): {e}"),
        }
    }

    for family in [egui::FontFamily::Proportional, egui::FontFamily::Monospace] {
        let list = defs.families.get_mut(&family).unwrap();
        // Inter is the primary face; our emoji/symbol/script fallbacks sit right after
        // it, ahead of egui's builtin fonts.
        list.insert(0, "Inter".to_owned());
        list.splice(1..1, fallbacks.iter().cloned());
    }

    defs
}

/// Install the custom font stack on the egui context. Call once at startup.
/// The material-icons font is registered afterwards via `egui_material_icons::initialize`
/// (see `AmminiApp::new`), which must run AFTER this function: `install` uses
/// `set_fonts` (replaces all fonts), `initialize` uses `add_font` (merges).
pub fn install(ctx: &egui::Context) {
    ctx.set_fonts(font_definitions());
    tracing::info!("fonts: custom font stack installed");
}

/// Prefix a Material icon glyph to a button label, e.g. `icon_label(ui, ICON_PLAY, "Play")`.
///
/// The glyph is pinned to the dedicated `material-icons` family (see
/// [`egui_material_icons::FONT_FAMILY`]); it must NOT flow through the Proportional
/// fallback chain as a plain string. The primary Proportional face, Inter, claims several
/// Material PUA codepoints (e.g. `play_arrow` U+E037) with its own blank glyphs, and egui
/// resolves each character to the *first* face in the chain that covers it — so a plain
/// string would render Inter's glyph (nothing visible) instead of the icon.
///
/// The glyph section is sized from the button text style so icon and label line up at the
/// widget's text size; `Color32::PLACEHOLDER` leaves the color to the widget (buttons
/// paint it with their own text color).
pub fn icon_label(ui: &egui::Ui, icon: MaterialIcon, text: &str) -> egui::WidgetText {
    icon_label_colored(ui, icon, text, egui::Color32::PLACEHOLDER)
}

/// Like [`icon_label`], but with an explicit color applied to both glyph and label
/// (used by filled accent buttons, which paint white text on the accent fill).
pub fn icon_label_colored(
    ui: &egui::Ui,
    icon: MaterialIcon,
    text: &str,
    color: egui::Color32,
) -> egui::WidgetText {
    use egui::text::LayoutJob;

    let size = icon_glyph_size(ui);

    let mut job = LayoutJob::default();
    job.append(
        icon.codepoint,
        0.0,
        egui::text::TextFormat {
            font_id: egui::FontId::new(
                size,
                egui::FontFamily::Name(egui_material_icons::FONT_FAMILY.into()),
            ),
            color,
            ..Default::default()
        },
    );
    job.append(
        text,
        3.0,
        egui::text::TextFormat {
            color,
            ..Default::default()
        },
    );
    egui::WidgetText::from(job)
}

/// How much larger icon glyphs are drawn than the surrounding widget text, so an
/// icon reads at a comfortable weight next to a label instead of looking like a
/// stray character.
pub const ICON_SCALE: f32 = 1.3;

/// Text size this `Ui`'s buttons resolve to (egui's `TextStyle::Button`).
fn button_text_size(ui: &egui::Ui) -> f32 {
    ui.style()
        .text_styles
        .get(&egui::TextStyle::Button)
        .cloned()
        .unwrap_or_default()
        .size
}

/// Size icon glyphs are drawn at.
fn icon_glyph_size(ui: &egui::Ui) -> f32 {
    button_text_size(ui) * ICON_SCALE
}

/// A standalone icon glyph (no label), sized like the icons in [`icon_label`].
/// Use it for icon-only buttons so they match their icon+label neighbours.
pub fn icon_only(ui: &egui::Ui, icon: MaterialIcon) -> egui::WidgetText {
    let mut job = egui::text::LayoutJob::default();
    job.append(
        icon.codepoint,
        0.0,
        egui::text::TextFormat {
            font_id: egui::FontId::new(
                icon_glyph_size(ui),
                egui::FontFamily::Name(egui_material_icons::FONT_FAMILY.into()),
            ),
            color: egui::Color32::PLACEHOLDER,
            ..Default::default()
        },
    );
    egui::WidgetText::from(job)
}
