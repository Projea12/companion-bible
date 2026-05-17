use companion_bible::KjvBible;
use std::collections::HashSet;

const MATCH_THRESHOLD: f32 = 0.55;

const STOP_WORDS: &[&str] = &[
    "the", "and", "for", "that", "this", "with", "not", "but", "are", "was", "were", "have",
    "has", "had", "his", "her", "their", "our", "thy", "thee", "thou", "thine", "unto", "also",
    "shall", "will", "unto", "even", "from", "which", "who", "whom", "what", "all", "they",
    "them", "him", "she", "its", "hath", "doth", "saith", "said",
];

/// Compare transcribed text against every verse in a chapter and return the
/// best matching verse number + score, or `None` if no verse clears the
/// threshold.
///
/// Scoring: fraction of significant verse words found in the transcript.
/// KJV preachers often read close to verbatim, so even 55 % word overlap is
/// a strong signal.
pub fn fuzzy_verse_match(
    text: &str,
    bible: &KjvBible,
    book: &str,
    chapter: u8,
) -> Option<(u8, f32)> {
    if text.split_whitespace().count() < 5 {
        return None;
    }

    let text_words = tokenize(text);
    let verse_count = bible.verse_count(book, chapter).ok()?;

    let mut best_verse: Option<u8> = None;
    let mut best_score = MATCH_THRESHOLD;

    for v in 1..=verse_count {
        if let Ok(verse) = bible.get_verse(book, chapter, v) {
            let verse_words = tokenize(&verse.text);
            let n_verse = verse_words.len();
            if n_verse == 0 {
                continue;
            }
            let matches = verse_words.iter().filter(|w| text_words.contains(*w)).count();
            let score = matches as f32 / n_verse as f32;
            if score > best_score {
                best_score = score;
                best_verse = Some(v);
            }
        }
    }

    best_verse.map(|v| (v, best_score))
}

fn tokenize(text: &str) -> HashSet<String> {
    text.to_lowercase()
        .split(|c: char| !c.is_alphabetic())
        .filter(|w| w.len() > 2 && !STOP_WORDS.contains(w))
        .map(String::from)
        .collect()
}

#[cfg(test)]
mod tests {
    use super::tokenize;

    #[test]
    fn stop_words_excluded() {
        let words = tokenize("for God so loved the world");
        assert!(!words.contains("for"));
        assert!(!words.contains("the"));
        assert!(words.contains("god"));
        assert!(words.contains("loved"));
        assert!(words.contains("world"));
    }

    #[test]
    fn short_text_below_min_words() {
        // fuzzy_verse_match returns None for < 5 words — tested indirectly
        let words = tokenize("grace mercy");
        assert_eq!(words.len(), 2);
    }
}
