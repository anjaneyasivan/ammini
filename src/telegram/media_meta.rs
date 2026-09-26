//! Filename keyword extraction for Telegram video cards.
//!
//! Telegram video documents usually arrive with a scene-style file name (e.g.
//! `Yellowstone (2018) - S02E02 - New Beginnings (1080p BluRay x265).mkv`). This
//! module turns that raw name into presentation metadata — a show title, an episode
//! subtitle, and a handful of quality/encoding chips — using a dependency-free token
//! scan. Anything it cannot recognise degrades gracefully to the file name, so a card
//! is never blank.

/// Presentation metadata for a video attachment, derived from its file name (plus the
/// size/duration the caller fills in from the Telegram document).
#[derive(Clone, Debug, PartialEq)]
pub struct MediaMeta {
    /// The original file name, extension included — always shown on the card.
    pub file_name: String,
    /// File size in bytes, when the document reports it.
    pub size_bytes: Option<u64>,
    /// Duration in seconds, when the document reports it.
    pub duration_secs: Option<f64>,
    /// Show/movie title, including the `SxxExx` marker when one is present.
    pub title: String,
    /// Episode title between the episode marker and the quality tags, if any.
    pub subtitle: Option<String>,
    pub season: Option<u32>,
    pub episode: Option<u32>,
    pub year: Option<u32>,
    /// Quality/encoding tags in a stable order (resolution, source, codec, …).
    pub chips: Vec<String>,
}

impl MediaMeta {
    /// Parse `file_name` into a [`MediaMeta`] (size/duration stay `None`).
    #[must_use]
    pub fn parse(file_name: &str) -> Self {
        let stem = strip_extension(file_name);
        let normalized = normalize(&stem);
        let tokens: Vec<&str> = normalized.split_whitespace().collect();

        let meta_start = tokens
            .iter()
            .position(|t| is_tech_token(t))
            .unwrap_or(tokens.len());
        let year = find_year(&tokens[..meta_start]);
        let (season, episode, marker_end) = find_episode(&tokens[..meta_start]);

        let title_end = marker_end.unwrap_or(meta_start);
        let title_tokens = clean_edges(&tokens[..title_end]);
        let title = if title_tokens.is_empty() {
            let all = clean_edges(&tokens).join(" ");
            if all.is_empty() {
                "video".to_string()
            } else {
                all
            }
        } else {
            title_tokens.join(" ")
        };

        let subtitle = marker_end.and_then(|end| {
            let rest = clean_edges(&tokens[end..meta_start]);
            (!rest.is_empty()).then(|| rest.join(" "))
        });

        // Season/episode form one chip and lead the row, so they still show when a
        // caption hides the derived title (the title is where they'd otherwise appear).
        let mut chips = Vec::new();
        match (season, episode) {
            (Some(s), Some(e)) => chips.push(format!("S{s:02}E{e:02}")),
            (Some(s), None) => chips.push(format!("S{s:02}")),
            (None, Some(e)) => chips.push(format!("E{e:02}")),
            (None, None) => {}
        }
        for chip in extract_chips(&stem) {
            if chips.len() >= MAX_CHIPS {
                break;
            }
            chips.push(chip);
        }

        Self {
            file_name: file_name.to_owned(),
            size_bytes: None,
            duration_secs: None,
            title,
            subtitle,
            season,
            episode,
            year,
            chips,
        }
    }
}

/// Format a byte count the way a media card does: `1.2 GB`, `512 MB`, `940 KB`.
#[must_use]
pub fn format_bytes(bytes: u64) -> String {
    const UNITS: [&str; 5] = ["B", "KB", "MB", "GB", "TB"];
    let mut value = bytes as f64;
    let mut unit = 0;
    while value >= 1024.0 && unit < UNITS.len() - 1 {
        value /= 1024.0;
        unit += 1;
    }
    if unit == 0 {
        format!("{bytes} B")
    } else if value < 10.0 {
        format!("{value:.1} {}", UNITS[unit])
    } else {
        format!("{value:.0} {}", UNITS[unit])
    }
}

/// Format a duration compactly: `1h 24m`, `42m`, `42m 10s`, `18s`.
#[must_use]
pub fn format_duration(secs: f64) -> String {
    let total = secs.max(0.0).round() as u64;
    let h = total / 3600;
    let m = (total % 3600) / 60;
    let s = total % 60;
    if h > 0 {
        if m > 0 {
            format!("{h}h {m}m")
        } else {
            format!("{h}h")
        }
    } else if m > 0 {
        if s > 0 {
            format!("{m}m {s}s")
        } else {
            format!("{m}m")
        }
    } else {
        format!("{s}s")
    }
}

/// Drop a trailing video extension (`.mkv`, `.mp4`, …), leaving dots that are part of
/// the name intact (`Show.Name.mkv` → `Show.Name`).
fn strip_extension(name: &str) -> String {
    const VIDEO_EXTS: &[&str] = &[
        "mp4", "mkv", "avi", "mov", "webm", "ogv", "flv", "m4v", "mpg", "mpeg", "ts", "m2ts",
        "wmv", "3gp",
    ];
    match name.rsplit_once('.') {
        Some((stem, ext))
            if !stem.is_empty() && VIDEO_EXTS.contains(&ext.to_ascii_lowercase().as_str()) =>
        {
            stem.to_owned()
        }
        _ => name.to_owned(),
    }
}

/// Collapse the scene-style separators (`.` and `_`) into spaces so the rest of the
/// scan can work on words. Hyphens are kept because they often join title words.
fn normalize(s: &str) -> String {
    s.chars()
        .map(|c| if c == '.' || c == '_' { ' ' } else { c })
        .collect()
}

/// Lowercase, alphanumeric-only form of a token — for pattern matching.
fn compact(s: &str) -> String {
    s.chars()
        .filter(char::is_ascii_alphanumeric)
        .map(|c| c.to_ascii_lowercase())
        .collect()
}

/// `s02e02`, `s02`, `e02`, or `1x02` → season/episode, plus the index just past the
/// marker so callers can slice title/subtitle around it.
fn find_episode(tokens: &[&str]) -> (Option<u32>, Option<u32>, Option<usize>) {
    let mut season = None;
    let mut episode = None;
    let mut end = None;

    let mut i = 0;
    while i < tokens.len() {
        let c = compact(tokens[i]);
        if let Some((s, e)) = parse_sxxexx(&c) {
            season = season.or(s);
            episode = episode.or(e);
            end = Some(i + 1);
        } else if (c == "season" || c == "episode")
            && let Some(n) = tokens.get(i + 1).and_then(|t| parse_all_u32(t))
        {
            if c == "season" {
                season = Some(n);
            } else {
                episode = Some(n);
            }
            end = Some(i + 2);
            i += 1;
        }
        i += 1;
    }

    (season, episode, end)
}

/// `s02e02` → (2, 2); `s02` → (2, None); `e02` → (None, 2); `1x02` → (1, 2).
fn parse_sxxexx(c: &str) -> Option<(Option<u32>, Option<u32>)> {
    if let Some(rest) = c.strip_prefix('s') {
        if let Some((se, ep)) = rest.split_once('e') {
            return Some((Some(parse_all_u32(se)?), Some(parse_all_u32(ep)?)));
        }
        if let Some(s) = parse_all_u32(rest) {
            return Some((Some(s), None));
        }
    }
    if let Some(rest) = c.strip_prefix('e')
        && let Some(e) = parse_all_u32(rest)
    {
        return Some((None, Some(e)));
    }
    if let Some((a, b)) = c.split_once('x')
        && let (Some(s), Some(e)) = (parse_all_u32(a), parse_all_u32(b))
    {
        return Some((Some(s), Some(e)));
    }
    None
}

/// Parse a token that is entirely digits (allowing leading zeros).
fn parse_all_u32(s: &str) -> Option<u32> {
    if !s.is_empty() && s.bytes().all(|b| b.is_ascii_digit()) {
        s.parse().ok()
    } else {
        None
    }
}

/// First 4-digit year in the title region.
fn find_year(tokens: &[&str]) -> Option<u32> {
    tokens.iter().find_map(|t| {
        let c = compact(t);
        if c.len() != 4 {
            return None;
        }
        c.parse::<u32>().ok().filter(|y| (1900..=2099).contains(y))
    })
}

/// Tokens that mark the start of the quality/encoding block. Everything before the
/// first one is title/subtitle material.
fn is_tech_token(tok: &str) -> bool {
    const RESOLUTION: &[&str] = &["2160p", "1440p", "1080p", "720p", "480p", "4k", "uhd"];
    const SOURCE: &[&str] = &[
        "bluray", "bdrip", "brrip", "webrip", "webdl", "web", "hdtv", "hdtvrip", "dvdrip", "remux",
        "hd", "bd",
    ];
    const CODEC: &[&str] = &["x265", "h265", "x264", "h264", "avc", "av1", "vp9"];
    const DEPTH: &[&str] = &[
        "10bit",
        "8bit",
        "hdr",
        "hdr10",
        "dovi",
        "dolbyvision",
        "multi",
    ];
    // Audio tags share a prefix and are often glued to channel counts (`ddp5`, `dd5`).
    const AUDIO_PREFIXES: &[&str] = &[
        "ddp", "dd", "aac", "ac3", "dts", "atmos", "truehd", "flac", "opus",
    ];

    let c = compact(tok);
    if c.is_empty() {
        return false;
    }
    RESOLUTION.contains(&c.as_str())
        || SOURCE.contains(&c.as_str())
        || CODEC.contains(&c.as_str())
        || DEPTH.contains(&c.as_str())
        || AUDIO_PREFIXES.iter().any(|p| c.starts_with(p))
}

/// Strip leading/trailing standalone separator tokens (and blank ones) so a title or
/// subtitle never starts or ends with a dangling `-`, while keeping interior separators
/// (e.g. the `-` in `Yellowstone (2018) - S02E02`).
fn clean_edges(tokens: &[&str]) -> Vec<String> {
    let is_sep = |t: &str| {
        t.is_empty()
            || t.chars()
                .all(|c| matches!(c, '-' | '–' | '—' | '|' | ':' | '•' | '.' | '_'))
    };
    let mut start = 0;
    let mut end = tokens.len();
    while start < end && is_sep(tokens[start]) {
        start += 1;
    }
    while end > start && is_sep(tokens[end - 1]) {
        end -= 1;
    }
    tokens[start..end]
        .iter()
        .map(|t| {
            t.trim_matches(|c: char| matches!(c, '–' | '—' | '|' | ':' | '•'))
                .to_string()
        })
        .filter(|s| !s.is_empty())
        .collect()
}

/// Ordered chip groups: within a group the first matching needle wins, and groups are
/// emitted top-to-bottom. This keeps resolution before source before codec, and stops
/// `web` from also matching inside `webdl`.
const CHIP_GROUPS: &[&[(&str, &str)]] = &[
    &[("2160p", "2160p"), ("4k", "4K"), ("uhd", "UHD")],
    &[("1440p", "1440p")],
    &[("1080p", "1080p")],
    &[("720p", "720p")],
    &[("480p", "480p")],
    &[("bluray", "BluRay"), ("bdrip", "BDRip"), ("brrip", "BRRip")],
    &[("webdl", "WEB-DL"), ("webrip", "WEBRip"), ("web", "WEB")],
    &[("hdtvrip", "HDTVRip"), ("hdtv", "HDTV")],
    &[("dvdrip", "DVDRip")],
    &[("remux", "REMUX")],
    &[
        ("x265", "x265"),
        ("h265", "x265"),
        ("hevc", "HEVC"),
        ("x264", "x264"),
        ("h264", "x264"),
        ("avc", "AVC"),
        ("av1", "AV1"),
        ("vp9", "VP9"),
    ],
    &[("10bit", "10bit"), ("8bit", "8bit")],
    &[
        ("hdr10", "HDR10"),
        ("dovi", "DV"),
        ("dolbyvision", "DV"),
        ("hdr", "HDR"),
    ],
    &[
        ("truehd", "TrueHD"),
        ("ddp51", "DDP5.1"),
        ("ddp5", "DDP5.1"),
        ("ddp", "DDP"),
    ],
    &[("dd51", "DD5.1"), ("dd5", "DD5.1")],
    &[("aac", "AAC"), ("ac3", "AC3")],
    &[("dts", "DTS"), ("atmos", "Atmos")],
    &[("multi", "MULTi")],
];

/// How many chips a card shows at most (season/episode included).
const MAX_CHIPS: usize = 7;

/// Quality/encoding chips for the whole file name.
fn extract_chips(stem: &str) -> Vec<String> {
    let hay = compact(stem);
    let mut chips: Vec<String> = Vec::new();
    for group in CHIP_GROUPS {
        if let Some((_, label)) = group.iter().find(|(needle, _)| hay.contains(needle)) {
            let label = (*label).to_string();
            if !chips.contains(&label) {
                chips.push(label);
            }
        }
        if chips.len() >= MAX_CHIPS {
            break;
        }
    }
    chips
}

#[cfg(test)]
mod tests {
    use super::{MediaMeta, format_bytes, format_duration};

    fn parse(name: &str) -> MediaMeta {
        MediaMeta::parse(name)
    }

    #[test]
    fn parses_show_episode_with_subtitle() {
        let m = parse("Yellowstone (2018) - S02E02 - New Beginnings (1080p BluRay x265).mkv");
        assert_eq!(m.title, "Yellowstone (2018) - S02E02");
        assert_eq!(m.subtitle.as_deref(), Some("New Beginnings"));
        assert_eq!(m.chips, vec!["S02E02", "1080p", "BluRay", "x265"]);
        assert_eq!(
            (m.season, m.episode, m.year),
            (Some(2), Some(2), Some(2018))
        );
    }

    #[test]
    fn parses_underscored_movie_name() {
        let m =
            parse("The_Whisper_Man_2026_1080p_10bit_WEBRip_TAMiL+ENG_DDP5_1_x265_PHINEASPSA.mkv");
        assert_eq!(m.title, "The Whisper Man 2026");
        assert_eq!(m.subtitle, None);
        assert_eq!(m.year, Some(2026));
        assert!(m.chips.contains(&"1080p".to_string()));
        assert!(m.chips.contains(&"WEBRip".to_string()));
        assert!(m.chips.contains(&"10bit".to_string()));
        assert!(m.chips.contains(&"x265".to_string()));
    }

    #[test]
    fn parses_dotted_scene_name() {
        let m = parse("@WMR_Ente.Mezhuthiri.Athazhangal.2018.720p.HD.x265.HEVC.mkv");
        assert_eq!(m.title, "@WMR Ente Mezhuthiri Athazhangal 2018");
        assert_eq!(m.year, Some(2018));
        assert!(m.chips.contains(&"720p".to_string()));
        assert!(m.chips.contains(&"x265".to_string()));
    }

    #[test]
    fn parses_bracketed_year() {
        let m = parse("Resident Evil [2002] (1080p x265 10bit Joy).mkv");
        assert_eq!(m.title, "Resident Evil [2002]");
        assert_eq!(m.year, Some(2002));
        assert!(m.chips.contains(&"1080p".to_string()));
        assert!(m.chips.contains(&"x265".to_string()));
        assert!(m.chips.contains(&"10bit".to_string()));
    }

    #[test]
    fn parses_alternate_episode_formats() {
        let m = parse("Show.Name.1x02.Pilot.1080p.WEB-DL.mkv");
        assert_eq!((m.season, m.episode), (Some(1), Some(2)));
        assert_eq!(m.title, "Show Name 1x02");
        assert_eq!(m.subtitle.as_deref(), Some("Pilot"));
        assert_eq!(m.chips[0], "S01E02");
        assert!(m.chips.contains(&"WEB-DL".to_string()));
    }

    #[test]
    fn falls_back_to_filename_when_nothing_matches() {
        let m = parse("family-video.mp4");
        assert_eq!(m.title, "family-video");
        assert_eq!(m.subtitle, None);
        assert!(m.chips.is_empty());
        assert_eq!(m.file_name, "family-video.mp4");
    }

    #[test]
    fn codec_tokens_are_not_episodes() {
        let m = parse("Movie.2024.x265.HEVC.1080p.mkv");
        assert_eq!(m.season, None);
        assert_eq!(m.episode, None);
        assert_eq!(m.title, "Movie 2024");
    }

    #[test]
    fn formats_bytes() {
        assert_eq!(format_bytes(900), "900 B");
        assert_eq!(format_bytes(940 * 1024), "940 KB");
        assert_eq!(format_bytes(9 * 1024 * 1024), "9.0 MB");
        assert_eq!(format_bytes(512 * 1024 * 1024), "512 MB");
        assert_eq!(format_bytes(1_288_490_189), "1.2 GB");
    }

    #[test]
    fn formats_durations() {
        assert_eq!(format_duration(18.0), "18s");
        assert_eq!(format_duration(42.0 * 60.0), "42m");
        assert_eq!(format_duration(42.0 * 60.0 + 10.0), "42m 10s");
        assert_eq!(format_duration(3600.0 + 24.0 * 60.0), "1h 24m");
    }

    #[test]
    fn media_meta_is_constructible_via_field_defaults() {
        // The struct is filled in by the client with document size/duration.
        let mut m = MediaMeta::parse("Movie.2020.1080p.mkv");
        m.size_bytes = Some(1000);
        m.duration_secs = Some(60.0);
        assert!(m.chips.contains(&"1080p".to_string()));
    }
}
