//! Helpers for local subtitle files.

use std::path::Path;

/// Video file extensions recognised by the player.
pub const VIDEO_EXTS: &[&str] = &["mp4", "mkv", "avi", "mov", "webm", "ogv", "flv"];

/// Subtitle file extensions we can feed to mpv via `sub-add`.
pub const SUBTITLE_EXTS: &[&str] = &["srt", "vtt", "ass", "ssa", "sub", "sup"];

pub fn is_video_ext(ext: &str) -> bool {
    VIDEO_EXTS.contains(&ext.to_ascii_lowercase().as_str())
}

pub fn is_subtitle_ext(ext: &str) -> bool {
    SUBTITLE_EXTS.contains(&ext.to_ascii_lowercase().as_str())
}

pub fn is_video_path(path: &Path) -> bool {
    path.extension()
        .and_then(|e| e.to_str())
        .is_some_and(is_video_ext)
}

pub fn is_subtitle_path(path: &Path) -> bool {
    path.extension()
        .and_then(|e| e.to_str())
        .is_some_and(is_subtitle_ext)
}

/// Title derived from a subtitle file: the file stem (no extension).
pub fn title_from_path(path: &Path) -> String {
    path.file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or("subtitle")
        .to_string()
}

/// Guess a language code from a filename like `movie.en.srt` or `movie_fr.ass`.
/// Returns an empty string when no recognisable code is found.
use libmpv2::Mpv;

/// Add one or more local subtitle files to the running mpv instance.
/// The first file is selected immediately (`select`), the rest are added as
/// `auto`. Returns an error string listing any failures.
pub fn add_subtitle_files(mpv: &Mpv, paths: &[std::path::PathBuf]) -> Result<(), String> {
    let mut errors = Vec::new();
    for (i, path) in paths.iter().enumerate() {
        let flag = if i == 0 { "select" } else { "auto" };
        let title = title_from_path(path);
        let lang = guess_lang_from_filename(path);
        let path_str = path.to_string_lossy();
        let args = [path_str.as_ref(), flag, title.as_str(), lang.as_str()];
        if let Err(e) = mpv.command("sub-add", &args) {
            errors.push(format!("{}: {e}", path.display()));
        }
    }
    if errors.is_empty() {
        Ok(())
    } else {
        Err(errors.join("; "))
    }
}

pub fn guess_lang_from_filename(path: &Path) -> String {
    let stem = match path.file_stem().and_then(|s| s.to_str()) {
        Some(s) => s,
        None => return String::new(),
    };
    // The language code is usually the last dot/underscore/hyphen separated token.
    let token = stem.rsplit(['.', '_', '-']).next().unwrap_or("").trim();
    if (token.len() == 2 || token.len() == 3) && token.chars().all(|c| c.is_ascii_alphabetic()) {
        return token.to_ascii_lowercase();
    }
    String::new()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;

    #[test]
    fn subtitle_exts_recognised() {
        for ext in ["srt", "vtt", "ass", "ssa", "sub", "sup", "SRT"] {
            assert!(is_subtitle_ext(ext), "{ext}");
        }
        assert!(!is_subtitle_ext("mp4"));
        assert!(!is_subtitle_ext("txt"));
    }

    #[test]
    fn video_exts_recognised() {
        for ext in ["mp4", "mkv", "MKV"] {
            assert!(is_video_ext(ext));
        }
        assert!(!is_video_ext("srt"));
    }

    #[test]
    fn paths_classified_by_extension() {
        assert!(is_subtitle_path(Path::new("/tmp/movie.en.srt")));
        assert!(is_video_path(Path::new("/tmp/movie.mkv")));
        assert!(!is_subtitle_path(Path::new("/tmp/movie.mkv")));
        assert!(!is_video_path(Path::new("/tmp/movie.srt")));
    }

    #[test]
    fn title_from_path_strips_extension() {
        assert_eq!(title_from_path(Path::new("/tmp/movie.en.srt")), "movie.en");
        // Note: title keeps the language token; caller may trim if desired.
    }

    #[test]
    fn guess_lang_from_filename_handles_common_patterns() {
        assert_eq!(guess_lang_from_filename(Path::new("movie.en.srt")), "en");
        assert_eq!(guess_lang_from_filename(Path::new("movie_fr.ass")), "fr");
        assert_eq!(guess_lang_from_filename(Path::new("movie-es.vtt")), "es");
        assert_eq!(guess_lang_from_filename(Path::new("movie.srt")), "");
        assert_eq!(guess_lang_from_filename(Path::new("my movie.mkv")), "");
    }
}
