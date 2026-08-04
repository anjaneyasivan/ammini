//! Custom font stack for the app: a modern monochrome emoji font (the egui defaults are
//! years out of date and miss most emojis found in real chats), a bundled DejaVu Sans for
//! symbols/dingbats/arrows the emoji font lacks, plus best-effort system fonts as
//! fallbacks for non-Latin scripts (Devanagari, Arabic, Thai, …).

use std::sync::Arc;

use eframe::egui;

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
    ("Devanagari", "/System/Library/Fonts/Supplemental/Devanagari Sangam MN.ttc", 0),
    ("Bengali", "/System/Library/Fonts/Supplemental/Bengali Sangam MN.ttc", 0),
    ("Gurmukhi", "/System/Library/Fonts/Supplemental/Gurmukhi Sangam MN.ttc", 0),
    ("Gujarati", "/System/Library/Fonts/Supplemental/Gujarati Sangam MN.ttc", 0),
    ("Tamil", "/System/Library/Fonts/Supplemental/Tamil Sangam MN.ttc", 0),
    ("Telugu", "/System/Library/Fonts/Supplemental/Telugu Sangam MN.ttc", 0),
    ("Malayalam", "/System/Library/Fonts/Supplemental/Malayalam Sangam MN.ttc", 0),
    ("Kannada", "/System/Library/Fonts/NotoSansKannada.ttc", 0),
    ("Oriya", "/System/Library/Fonts/NotoSansOriya.ttc", 0),
    ("Myanmar", "/System/Library/Fonts/NotoSansMyanmar.ttc", 0),
    ("Armenian", "/System/Library/Fonts/NotoSansArmenian.ttc", 0),
    ("Khmer", "/System/Library/Fonts/Supplemental/Khmer Sangam MN.ttf", 0),
    ("Lao", "/System/Library/Fonts/Supplemental/Lao MN.ttc", 0),
    ("Thai", "/System/Library/Fonts/Supplemental/Thonburi.ttc", 0),
    // Linux CJK (Noto Sans CJK). Paths vary by distro; only one will exist.
    ("NotoCJK", "/usr/share/fonts/opentype/noto/NotoSansCJK-Regular.ttc", 0),
    ("NotoCJKAlt", "/usr/share/fonts/truetype/noto/NotoSansCJK-Regular.ttc", 0),
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
    defs.font_data.insert("DejaVuSans".to_owned(), Arc::new(egui::FontData::from_static(DEJA_VU_SANS)));

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
        list.splice(1..1, fallbacks.iter().cloned());
    }

    defs
}

/// Install the custom font stack on the egui context. Call once at startup.
pub fn install(ctx: &egui::Context) {
    ctx.set_fonts(font_definitions());
    tracing::info!("fonts: custom font stack installed");
}
