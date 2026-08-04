//! Verifies the custom font stack: the bundled Noto Emoji font must cover common and
//! modern emojis, and system script fallbacks must cover their scripts when the host OS
//! provides the fonts (checked conditionally so the test passes on any platform).

use std::collections::HashSet;

use eframe::egui;
use egui::epaint::text::{Fonts, TextOptions};
use min_mpv::fonts;

fn covered_chars() -> (egui::FontDefinitions, std::collections::BTreeMap<char, Vec<String>>) {
    let defs = fonts::font_definitions();
    let mut fonts = Fonts::new(TextOptions::default(), defs.clone());
    let chars = fonts
        .fonts
        .font(&egui::FontFamily::Proportional)
        .characters()
        .clone();
    (defs, chars)
}

#[test]
fn bundled_emoji_font_covers_common_and_modern_emojis() {
    let (defs, chars) = covered_chars();

    assert!(
        defs.font_data.contains_key("NotoEmoji"),
        "bundled NotoEmoji font missing from font definitions"
    );

    // Mix of classic and modern emojis (incl. Unicode 11/14 codepoints the egui
    // builtin fonts lack, and ZWJ-sequence members).
    for c in ['😀', '😂', '🚀', '🔥', '👍', '❤', '🥺', '🫠', '👨'] {
        let by = chars.get(&c).unwrap_or_else(|| panic!("no glyph for {c}"));
        assert!(
            by.iter().any(|f| f == "NotoEmoji"),
            "{c} not covered by NotoEmoji: {by:?}"
        );
    }
}

#[test]
fn script_fallbacks_cover_when_host_provides_them() {
    let (defs, chars) = covered_chars();
    let loaded: HashSet<&str> = defs.font_data.keys().map(String::as_str).collect();

    // Each entry is asserted only if the corresponding system font was found, so the
    // test stays green on machines without them.
    for (font, c) in [
        ("Devanagari", 'ह'),
        ("Arabic", 'ا'),
        ("Kannada", 'ಕ'),
        ("Thai", 'ก'),
        ("Tamil", 'த'),
        ("Telugu", 'త'),
        ("Bengali", 'ব'),
    ] {
        if loaded.contains(font) {
            assert!(
                chars.contains_key(&c),
                "{font} font is loaded but has no glyph for {c}"
            );
        }
    }
}
