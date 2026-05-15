use crate::types::VerseText;

// ─── SearchResult ─────────────────────────────────────────────────────────────

/// A single verse match returned by `KjvBible::search`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SearchResult {
    pub verse: VerseText,
    /// Higher is more relevant. Computed from word-boundary matches and
    /// the number of query terms found.
    pub score: u32,
}

// ─── Scoring (pub(crate)) ─────────────────────────────────────────────────────

/// Score a pre-lowercased verse against a set of pre-lowercased query terms.
///
/// Returns 0 if no term matches at all, so the caller can skip the verse.
pub(crate) fn score_verse(lower_verse: &str, terms: &[String]) -> u32 {
    let mut score: u32 = 0;
    for term in terms {
        if lower_verse.contains(term.as_str()) {
            score += 1; // substring hit
            // Whole-word bonus: chars before and after the match must be
            // non-alphabetic (or the match is at the string boundary).
            if is_whole_word_match(lower_verse, term) {
                score += 10;
            }
        }
    }
    score
}

/// Returns `true` if `term` appears in `text` with word boundaries on both
/// sides (non-alphabetic character or string edge).
fn is_whole_word_match(text: &str, term: &str) -> bool {
    let bytes = text.as_bytes();
    let tlen = term.len();
    let tlen_text = bytes.len();

    if tlen == 0 || tlen > tlen_text {
        return false;
    }

    let mut start = 0;
    while let Some(pos) = text[start..].find(term.as_ref() as &str) {
        let abs = start + pos;
        let before_ok = abs == 0 || !bytes[abs - 1].is_ascii_alphabetic();
        let after_ok = abs + tlen >= tlen_text || !bytes[abs + tlen].is_ascii_alphabetic();
        if before_ok && after_ok {
            return true;
        }
        start = abs + 1;
        if start >= tlen_text {
            break;
        }
    }
    false
}
