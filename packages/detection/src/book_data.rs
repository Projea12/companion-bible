//! Bible book names and their spoken/written aliases.
//!
//! Each entry is `(canonical_name, &[aliases])` where the canonical name
//! matches the key used in the Bible JSON database (e.g. `"1 Corinthians"`).
//! Aliases include:
//! - Standard three-letter abbreviations
//! - Common short forms (e.g. "Psalm" for "Psalms")
//! - Spoken variants (e.g. "Song of Songs" for "Song of Solomon")
//!
//! After [`NumberNormalizer`][crate::NumberNormalizer] runs, ordinal prefixes
//! ("First", "Second", "Third") are already converted to digits ("1", "2", "3"),
//! so numbered books are represented as "1 Samuel", "2 Kings", etc.

use std::collections::HashMap;
use std::sync::OnceLock;

// ─── Book data ────────────────────────────────────────────────────────────────

/// All 66 canonical Bible book names with their common aliases.
///
/// `(canonical_name, &[aliases including canonical])` — aliases are matched
/// case-insensitively in the pattern engine.
pub const BOOK_DATA: &[(&str, &[&str])] = &[
    // ── Old Testament ─────────────────────────────────────────────────────────
    ("Genesis", &["Genesis", "Gen"]),
    ("Exodus", &["Exodus", "Exod", "Ex"]),
    ("Leviticus", &["Leviticus", "Lev"]),
    ("Numbers", &["Numbers", "Num"]),
    ("Deuteronomy", &["Deuteronomy", "Deut", "Dt"]),
    ("Joshua", &["Joshua", "Josh"]),
    ("Judges", &["Judges", "Judg"]),
    ("Ruth", &["Ruth"]),
    ("1 Samuel", &["1 Samuel", "1 Sam"]),
    ("2 Samuel", &["2 Samuel", "2 Sam"]),
    ("1 Kings", &["1 Kings", "1 Kgs"]),
    ("2 Kings", &["2 Kings", "2 Kgs"]),
    ("1 Chronicles", &["1 Chronicles", "1 Chron", "1 Chr"]),
    ("2 Chronicles", &["2 Chronicles", "2 Chron", "2 Chr"]),
    ("Ezra", &["Ezra"]),
    ("Nehemiah", &["Nehemiah", "Neh"]),
    ("Esther", &["Esther", "Esth"]),
    ("Job", &["Job"]),
    ("Psalms", &["Psalms", "Psalm", "Psa", "Ps"]),
    ("Proverbs", &["Proverbs", "Prov"]),
    ("Ecclesiastes", &["Ecclesiastes", "Eccl", "Ecc"]),
    (
        "Song of Solomon",
        &["Song of Solomon", "Song of Songs", "Song", "Sos", "Cant"],
    ),
    ("Isaiah", &["Isaiah", "Isa"]),
    ("Jeremiah", &["Jeremiah", "Jer"]),
    ("Lamentations", &["Lamentations", "Lam"]),
    ("Ezekiel", &["Ezekiel", "Ezek"]),
    ("Daniel", &["Daniel", "Dan"]),
    ("Hosea", &["Hosea", "Hos"]),
    ("Joel", &["Joel"]),
    ("Amos", &["Amos"]),
    ("Obadiah", &["Obadiah", "Obad"]),
    ("Jonah", &["Jonah", "Jon"]),
    ("Micah", &["Micah", "Mic"]),
    ("Nahum", &["Nahum", "Nah"]),
    ("Habakkuk", &["Habakkuk", "Hab"]),
    ("Zephaniah", &["Zephaniah", "Zeph"]),
    ("Haggai", &["Haggai", "Hag"]),
    ("Zechariah", &["Zechariah", "Zech"]),
    ("Malachi", &["Malachi", "Mal"]),
    // ── New Testament ─────────────────────────────────────────────────────────
    ("Matthew", &["Matthew", "Matt", "Mt"]),
    ("Mark", &["Mark", "Mk"]),
    ("Luke", &["Luke", "Lk"]),
    ("John", &["John", "Jn"]),
    ("Acts", &["Acts"]),
    ("Romans", &["Romans", "Rom"]),
    ("1 Corinthians", &["1 Corinthians", "1 Cor"]),
    ("2 Corinthians", &["2 Corinthians", "2 Cor"]),
    ("Galatians", &["Galatians", "Gal"]),
    ("Ephesians", &["Ephesians", "Eph"]),
    ("Philippians", &["Philippians", "Phil"]),
    ("Colossians", &["Colossians", "Col"]),
    ("1 Thessalonians", &["1 Thessalonians", "1 Thess", "1 Thes"]),
    ("2 Thessalonians", &["2 Thessalonians", "2 Thess", "2 Thes"]),
    ("1 Timothy", &["1 Timothy", "1 Tim"]),
    ("2 Timothy", &["2 Timothy", "2 Tim"]),
    ("Titus", &["Titus", "Tit"]),
    ("Philemon", &["Philemon", "Phlm", "Phm"]),
    ("Hebrews", &["Hebrews", "Heb"]),
    ("James", &["James", "Jas"]),
    ("1 Peter", &["1 Peter", "1 Pet"]),
    ("2 Peter", &["2 Peter", "2 Pet"]),
    ("1 John", &["1 John", "1 Jn"]),
    ("2 John", &["2 John", "2 Jn"]),
    ("3 John", &["3 John", "3 Jn"]),
    ("Jude", &["Jude"]),
    ("Revelation", &["Revelation", "Rev"]),
];

// ─── Alias lookup ─────────────────────────────────────────────────────────────

static ALIAS_MAP: OnceLock<HashMap<String, &'static str>> = OnceLock::new();

fn alias_map() -> &'static HashMap<String, &'static str> {
    ALIAS_MAP.get_or_init(|| {
        let mut map = HashMap::new();
        for (canonical, aliases) in BOOK_DATA {
            for alias in *aliases {
                let key = normalise_alias(alias);
                map.insert(key, *canonical);
            }
        }
        map
    })
}

/// Normalise an alias for lookup: lowercase, collapse whitespace.
pub(crate) fn normalise_alias(s: &str) -> String {
    s.split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
        .to_lowercase()
}

/// Return the canonical book name for any alias (case-insensitive).
///
/// Whitespace is collapsed before matching, so "1  John" maps to "1 John".
///
/// ```
/// use companion_detection::book_data::canonical_name;
/// assert_eq!(canonical_name("rom"),              Some("Romans"));
/// assert_eq!(canonical_name("1 cor"),            Some("1 Corinthians"));
/// assert_eq!(canonical_name("Song of Songs"),    Some("Song of Solomon"));
/// assert_eq!(canonical_name("Hezekiah"),         None);
/// ```
pub fn canonical_name(alias: &str) -> Option<&'static str> {
    alias_map().get(&normalise_alias(alias)).copied()
}

// ─── Regex pattern helpers ────────────────────────────────────────────────────

/// Build a regex alternation string of every book name and alias.
///
/// The alternation is ordered longest-alias-first so that the regex engine
/// prefers "1 Corinthians" over "Corinthians" and "Song of Solomon" over
/// "Song of Songs" and both over "Song".  Spaces within aliases are replaced
/// with `\s+` for flexible whitespace matching.
pub fn build_book_alternation() -> String {
    let mut aliases: Vec<&str> = BOOK_DATA
        .iter()
        .flat_map(|(_, aliases)| aliases.iter().copied())
        .collect();

    // Longest raw string first — ensures greediness favours full names.
    aliases.sort_by_key(|b| std::cmp::Reverse(b.len()));

    aliases
        .iter()
        .map(|alias| alias_to_regex_part(alias))
        .collect::<Vec<_>>()
        .join("|")
}

/// Convert a single alias to its regex fragment: spaces → `\s+`.
/// No other escaping is needed because all aliases are alphanumeric + spaces.
fn alias_to_regex_part(alias: &str) -> String {
    alias.split_whitespace().collect::<Vec<_>>().join(r"\s+")
}

// ─── Tests ────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn canonical_name_exact_canonical() {
        assert_eq!(canonical_name("Genesis"), Some("Genesis"));
        assert_eq!(canonical_name("Revelation"), Some("Revelation"));
        assert_eq!(canonical_name("Psalms"), Some("Psalms"));
        assert_eq!(canonical_name("1 Corinthians"), Some("1 Corinthians"));
        assert_eq!(canonical_name("Song of Solomon"), Some("Song of Solomon"));
    }

    #[test]
    fn canonical_name_abbreviation() {
        assert_eq!(canonical_name("Gen"), Some("Genesis"));
        assert_eq!(canonical_name("Rom"), Some("Romans"));
        assert_eq!(canonical_name("Rev"), Some("Revelation"));
        assert_eq!(canonical_name("Ps"), Some("Psalms"));
        assert_eq!(canonical_name("Hab"), Some("Habakkuk"));
        assert_eq!(canonical_name("Zeph"), Some("Zephaniah"));
    }

    #[test]
    fn canonical_name_numbered_books() {
        assert_eq!(canonical_name("1 Sam"), Some("1 Samuel"));
        assert_eq!(canonical_name("2 Sam"), Some("2 Samuel"));
        assert_eq!(canonical_name("1 Cor"), Some("1 Corinthians"));
        assert_eq!(canonical_name("2 Cor"), Some("2 Corinthians"));
        assert_eq!(canonical_name("1 Thess"), Some("1 Thessalonians"));
        assert_eq!(canonical_name("3 Jn"), Some("3 John"));
    }

    #[test]
    fn canonical_name_alternate_forms() {
        assert_eq!(canonical_name("Psalm"), Some("Psalms"));
        assert_eq!(canonical_name("Song of Songs"), Some("Song of Solomon"));
    }

    #[test]
    fn canonical_name_case_insensitive() {
        assert_eq!(canonical_name("GENESIS"), Some("Genesis"));
        assert_eq!(canonical_name("revelation"), Some("Revelation"));
        assert_eq!(canonical_name("jOhN"), Some("John"));
        assert_eq!(canonical_name("1 COR"), Some("1 Corinthians"));
    }

    #[test]
    fn canonical_name_whitespace_normalised() {
        assert_eq!(canonical_name("1  John"), Some("1 John"));
        assert_eq!(canonical_name(" Romans "), Some("Romans"));
    }

    #[test]
    fn canonical_name_unknown_returns_none() {
        assert_eq!(canonical_name("Hezekiah"), None);
        assert_eq!(canonical_name(""), None);
        assert_eq!(canonical_name("Book"), None);
    }

    #[test]
    fn book_data_has_66_entries() {
        assert_eq!(BOOK_DATA.len(), 66, "expected exactly 66 books");
    }

    #[test]
    fn all_canonical_names_resolve() {
        for (canonical, _) in BOOK_DATA {
            assert_eq!(
                canonical_name(canonical),
                Some(*canonical),
                "canonical '{canonical}' does not resolve to itself"
            );
        }
    }

    #[test]
    fn all_aliases_resolve_to_their_canonical() {
        for (canonical, aliases) in BOOK_DATA {
            for alias in *aliases {
                assert_eq!(
                    canonical_name(alias),
                    Some(*canonical),
                    "alias '{alias}' did not resolve to '{canonical}'"
                );
            }
        }
    }

    #[test]
    fn build_book_alternation_contains_key_names() {
        let alt = build_book_alternation();
        assert!(alt.contains("Genesis"), "missing Genesis");
        assert!(alt.contains("Revelation"), "missing Revelation");
        assert!(alt.contains("Corinthians"), "missing Corinthians");
        assert!(alt.contains("Thessalonians"), "missing Thessalonians");
    }
}
