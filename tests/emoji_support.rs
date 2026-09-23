//! Verifies the custom font stack: the bundled Noto Emoji font must cover common and
//! modern emojis, and system script fallbacks must cover their scripts when the host OS
//! provides the fonts (checked conditionally so the test passes on any platform).

use std::collections::HashSet;
use std::sync::Arc;

use eframe::egui;
use egui::epaint::text::{Fonts, TextOptions};
use min_mpv::fonts;

fn covered_chars() -> (
    egui::FontDefinitions,
    std::collections::BTreeMap<char, Vec<String>>,
) {
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
fn bundled_dejavu_sans_covers_symbols_and_arrows() {
    let (defs, chars) = covered_chars();

    assert!(
        defs.font_data.contains_key("DejaVuSans"),
        "bundled DejaVu Sans missing from font definitions"
    );

    // Dingbats/arrows the emoji font doesn't cover (e.g. `➠` seen in real chats).
    for c in ['➠', '➢', '➥', '✔', '✖', '➔', '⟶'] {
        let by = chars.get(&c).unwrap_or_else(|| panic!("no glyph for {c}"));
        assert!(
            by.iter().any(|f| f == "DejaVuSans"),
            "{c} not covered by DejaVuSans: {by:?}"
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

#[test]
fn material_icon_font_registers_and_covers_icons() {
    // Mirrors `MinMpvApp::new`: the custom stack is installed with `set_fonts`, then the
    // material-icons font is merged in with `Context::add_font`. Icons must resolve
    // through the Proportional fallback chain, or every button label shows a
    // missing-glyph box. The merge below replicates `add_font` (Highest priority ->
    // front of the family list, Lowest -> appended), which the app only applies during
    // `Context::run()` — not available in a headless test.
    let insert = egui_material_icons::font_insert();
    let mut defs = fonts::font_definitions();
    for family in insert.families {
        let list = defs.families.entry(family.family).or_default();
        match family.priority {
            egui::epaint::text::FontPriority::Highest => list.insert(0, insert.name.clone()),
            egui::epaint::text::FontPriority::Lowest => list.push(insert.name.clone()),
        }
    }
    defs.font_data
        .insert(insert.name.clone(), Arc::new(insert.data));

    let mut fonts = Fonts::new(TextOptions::default(), defs);
    let chars = fonts
        .fonts
        .font(&egui::FontFamily::Proportional)
        .characters()
        .clone();

    // One icon per UI area: player transport, file/url toolbar, Telegram auth + chat.
    for icon in [
        egui_material_icons::icons::ICON_PLAY_ARROW,
        egui_material_icons::icons::ICON_FOLDER_OPEN,
        egui_material_icons::icons::ICON_SEND,
        egui_material_icons::icons::ICON_LOGOUT,
    ] {
        let c = icon.codepoint.chars().next().unwrap();
        let by = chars.get(&c).unwrap_or_else(|| panic!("no glyph for {c}"));
        assert!(
            by.iter().any(|f| f == egui_material_icons::FONT_FAMILY),
            "{c} not covered by the material-icons font: {by:?}"
        );
    }
}
