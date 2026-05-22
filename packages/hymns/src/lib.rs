mod generated {
    include!(concat!(env!("OUT_DIR"), "/hymns_data.rs"));
}

use std::sync::OnceLock;

// ─── Section ──────────────────────────────────────────────────────────────────

#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum HymnSection {
    Stanza { lines: Vec<String> },
    Chorus { lines: Vec<String> },
}

impl HymnSection {
    pub fn lines(&self) -> &[String] {
        match self {
            Self::Stanza { lines } | Self::Chorus { lines } => lines,
        }
    }

    pub fn last_line(&self) -> Option<&str> {
        self.lines()
            .iter()
            .rev()
            .find(|l| !l.trim().is_empty())
            .map(|s| s.as_str())
    }

    pub fn is_chorus(&self) -> bool {
        matches!(self, Self::Chorus { .. })
    }
}

// ─── Hymn ─────────────────────────────────────────────────────────────────────

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct Hymn {
    pub number: u16,
    pub title: String,
    /// Raw sections in file order (Stanza / Chorus alternating).
    sections: Vec<HymnSection>,
}

impl Hymn {
    /// The full playback sequence interleaving chorus after every stanza.
    ///
    /// For hymns with a chorus: S1 → C → S2 → C → … → SN → C
    /// For hymns without: S1 → S2 → … → SN
    pub fn playback_sequence(&self) -> Vec<&HymnSection> {
        let chorus = self.sections.iter().find(|s| s.is_chorus());
        let stanzas: Vec<&HymnSection> = self.sections.iter().filter(|s| !s.is_chorus()).collect();

        let mut seq = Vec::new();
        for stanza in stanzas {
            seq.push(stanza);
            if let Some(c) = chorus {
                seq.push(c);
            }
        }
        seq
    }

    pub fn has_chorus(&self) -> bool {
        self.sections.iter().any(|s| s.is_chorus())
    }
}

// ─── Parser ───────────────────────────────────────────────────────────────────

fn parse_hymn(number: u16, title: &str, content: &str) -> Hymn {
    let mut sections: Vec<HymnSection> = Vec::new();

    for block in content.split("\n\n") {
        let lines: Vec<String> = block.lines().map(|l| l.trim_end().to_string()).collect();

        let non_empty: Vec<&String> = lines.iter().filter(|l| !l.trim().is_empty()).collect();
        if non_empty.is_empty() {
            continue;
        }

        // A block is a Chorus if ALL its non-empty lines start with whitespace.
        let is_chorus = non_empty
            .iter()
            .all(|l| l.starts_with(' ') || l.starts_with('\t'));

        let cleaned: Vec<String> = non_empty.iter().map(|l| l.trim().to_string()).collect();

        if is_chorus {
            sections.push(HymnSection::Chorus { lines: cleaned });
        } else {
            sections.push(HymnSection::Stanza { lines: cleaned });
        }
    }

    Hymn {
        number,
        title: title.to_string(),
        sections,
    }
}

// ─── HymnBook ─────────────────────────────────────────────────────────────────

pub struct HymnBook {
    hymns: Vec<Hymn>,
}

static BOOK: OnceLock<HymnBook> = OnceLock::new();

impl HymnBook {
    /// Return the shared global instance (parsed once, reused forever).
    pub fn global() -> &'static HymnBook {
        BOOK.get_or_init(|| {
            let hymns = generated::HYMNS_RAW
                .iter()
                .map(|(number, title, content)| parse_hymn(*number, title, content))
                .collect();
            HymnBook { hymns }
        })
    }

    pub fn get(&self, number: u16) -> Option<&Hymn> {
        self.hymns.iter().find(|h| h.number == number)
    }

    pub fn len(&self) -> usize {
        self.hymns.len()
    }

    pub fn is_empty(&self) -> bool {
        self.hymns.is_empty()
    }
}

// ─── Fuzzy last-line matcher ──────────────────────────────────────────────────

/// Returns `true` when at least 70 % of the words in `last_line` appear
/// (case-insensitively) in the transcribed `text`.
pub fn last_line_matches(last_line: &str, text: &str) -> bool {
    let needle_words: Vec<String> = tokenize(last_line);
    if needle_words.is_empty() {
        return false;
    }
    let haystack_words: Vec<String> = tokenize(text);
    let matched = needle_words
        .iter()
        .filter(|w| haystack_words.contains(w))
        .count();
    let ratio = matched as f32 / needle_words.len() as f32;
    ratio >= 0.70
}

fn tokenize(s: &str) -> Vec<String> {
    s.split(|c: char| !c.is_alphanumeric())
        .filter(|t| t.len() >= 2)
        .map(|t| t.to_lowercase())
        .collect()
}

// ─── Tests ────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn book_has_260_hymns() {
        assert_eq!(HymnBook::global().len(), 260);
    }

    #[test]
    fn hymn_1_has_chorus() {
        let h = HymnBook::global().get(1).unwrap();
        assert!(h.has_chorus(), "hymn 1 should have a chorus");
    }

    #[test]
    fn playback_sequence_interleaves_chorus() {
        let h = HymnBook::global().get(1).unwrap();
        let seq = h.playback_sequence();
        // Sequence must alternate stanza/chorus.
        for (i, section) in seq.iter().enumerate() {
            if i % 2 == 0 {
                assert!(!section.is_chorus(), "even index should be stanza");
            } else {
                assert!(section.is_chorus(), "odd index should be chorus");
            }
        }
    }

    #[test]
    fn last_line_matches_70_percent() {
        assert!(last_line_matches(
            "Never a friend like Jesus",
            "never a friend like jesus christ"
        ));
        assert!(last_line_matches(
            "Great is Thy faithfulness Lord unto me",
            "great is thy faithfulness lord"
        ));
        assert!(!last_line_matches(
            "Never a friend like Jesus",
            "hello world"
        ));
    }

    #[test]
    fn lookup_by_number() {
        let book = HymnBook::global();
        assert!(book.get(1).is_some());
        assert!(book.get(260).is_some());
        assert!(book.get(999).is_none());
    }
}
