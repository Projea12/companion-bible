//! Detects GHS hymn number references in transcribed speech.
//!
//! Handles patterns like:
//!   - "GHS 234"
//!   - "open GHS 234"
//!   - "GHS two hundred and sixty four"
//!   - "Gospel Hymns and Sound number 234"
//!   - "Gospel Hymns and Songs number two hundred and sixty"

use std::sync::OnceLock;

use regex::Regex;

use crate::NumberNormalizer;

// ─── Regex ────────────────────────────────────────────────────────────────────

static HYMN_RE: OnceLock<Regex> = OnceLock::new();

fn hymn_re() -> &'static Regex {
    HYMN_RE.get_or_init(|| {
        Regex::new(
            r"(?i)\b(?:open\s+)?(?:ghs|gospel\s+hymns?\s+and\s+s(?:ound|ongs?|ong))\s+(?:number\s+)?(\d+)\b",
        )
        .expect("hymn_re")
    })
}

// ─── Public API ───────────────────────────────────────────────────────────────

/// Attempt to extract a hymn number from a transcription segment.
///
/// Runs the `NumberNormalizer` first so spoken numbers ("two hundred and sixty
/// four") are converted to digits before pattern matching.
///
/// Returns the hymn number (1–260) if found, otherwise `None`.
pub fn detect_hymn_number(text: &str) -> Option<u16> {
    let nn = NumberNormalizer::new();
    let normalised = nn.normalize(text);

    hymn_re()
        .captures(&normalised)
        .and_then(|caps| caps.get(1))
        .and_then(|m| m.as_str().parse::<u16>().ok())
        .filter(|n| (1..=260).contains(n))
}

// ─── Tests ────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn detects_ghs_digits() {
        assert_eq!(detect_hymn_number("let us open GHS 234"), Some(234));
        assert_eq!(detect_hymn_number("GHS 1"), Some(1));
        assert_eq!(detect_hymn_number("GHS 260"), Some(260));
    }

    #[test]
    fn detects_ghs_spoken_number() {
        assert_eq!(
            detect_hymn_number("open GHS two hundred and thirty four"),
            Some(234)
        );
        assert_eq!(detect_hymn_number("GHS two hundred"), Some(200));
        assert_eq!(detect_hymn_number("ghs thirty four"), Some(34));
    }

    #[test]
    fn detects_full_name_digits() {
        assert_eq!(
            detect_hymn_number("Gospel Hymns and Sound number 234"),
            Some(234)
        );
        assert_eq!(
            detect_hymn_number("Gospel Hymns and Songs number 10"),
            Some(10)
        );
    }

    #[test]
    fn detects_full_name_spoken() {
        assert_eq!(
            detect_hymn_number("Gospel Hymns and Sound number two hundred and sixty"),
            Some(260)
        );
    }

    #[test]
    fn rejects_out_of_range() {
        assert_eq!(detect_hymn_number("GHS 0"), None);
        assert_eq!(detect_hymn_number("GHS 261"), None);
        assert_eq!(detect_hymn_number("GHS 999"), None);
    }

    #[test]
    fn returns_none_for_unrelated_text() {
        assert_eq!(detect_hymn_number("John 3:16"), None);
        assert_eq!(detect_hymn_number("let us pray"), None);
    }

    #[test]
    fn case_insensitive() {
        assert_eq!(detect_hymn_number("ghs 50"), Some(50));
        assert_eq!(
            detect_hymn_number("GOSPEL HYMNS AND SOUND NUMBER 100"),
            Some(100)
        );
    }
}
