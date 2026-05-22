//! Number-word normalizer for spoken scripture references.
//!
//! Whisper transcribes numbers as words ("chapter three verse sixteen").
//! This module converts those words to digits ("chapter 3 verse 16") so the
//! downstream scripture-detection parser can match numeric patterns.
//!
//! ## Supported range
//!
//! | Input form                     | Example                              |
//! |-------------------------------|--------------------------------------|
//! | Ordinals (book prefixes)       | "First Corinthians" → "1 Corinthians"|
//! | Cardinals 1–19                 | "three" → "3"                        |
//! | Cardinals 20, 30, … 90        | "twenty" → "20"                      |
//! | Compound 21–99 (spaced)        | "twenty one" → "21"                  |
//! | Compound 21–99 (hyphenated)    | "twenty-one" → "21"                  |
//! | Hundreds 100–199               | "one hundred" → "100"                |
//! | Hundreds + tens 110–190        | "one hundred and thirty" → "130"     |
//! | Hundreds + compound 101–199    | "one hundred and forty nine" → "149" |
//!
//! The upper bound of 199 covers every chapter and verse number in the Bible
//! (max chapter: Psalms 150; max verse: Psalm 119:176).
//!
//! ## Algorithm
//!
//! Text is tokenized on whitespace.  Hyphens between alphabetic characters are
//! converted to spaces first (handling "twenty-one" as two tokens).  The token
//! stream is then scanned left-to-right; at each position the parser tries to
//! greedily consume the longest valid number phrase and replace it with its
//! decimal representation.  Non-number tokens pass through unchanged.

// ─── Word tables ──────────────────────────────────────────────────────────────

/// Words for 1–19 (ones and teens).
const ONES: &[(&str, u32)] = &[
    ("one", 1),
    ("two", 2),
    ("three", 3),
    ("four", 4),
    ("five", 5),
    ("six", 6),
    ("seven", 7),
    ("eight", 8),
    ("nine", 9),
    ("ten", 10),
    ("eleven", 11),
    ("twelve", 12),
    ("thirteen", 13),
    ("fourteen", 14),
    ("fifteen", 15),
    ("sixteen", 16),
    ("seventeen", 17),
    ("eighteen", 18),
    ("nineteen", 19),
];

/// Words for 20, 30, … 90 (tens).
const TENS: &[(&str, u32)] = &[
    ("twenty", 20),
    ("thirty", 30),
    ("forty", 40),
    ("fifty", 50),
    ("sixty", 60),
    ("seventy", 70),
    ("eighty", 80),
    ("ninety", 90),
];

/// Ordinal words and their cardinal equivalents.
///
/// Ordinals are used as book-number prefixes in spoken references:
/// "First Corinthians", "Second Timothy", "Third John".
const ORDINALS: &[(&str, u32)] = &[
    ("first", 1),
    ("second", 2),
    ("third", 3),
    ("fourth", 4),
    ("fifth", 5),
    ("sixth", 6),
    ("seventh", 7),
    ("eighth", 8),
    ("ninth", 9),
    ("tenth", 10),
    ("eleventh", 11),
    ("twelfth", 12),
    ("thirteenth", 13),
    ("fourteenth", 14),
    ("fifteenth", 15),
    ("sixteenth", 16),
    ("seventeenth", 17),
    ("eighteenth", 18),
    ("nineteenth", 19),
    ("twentieth", 20),
];

// ─── Lookup helpers ───────────────────────────────────────────────────────────

fn lookup_ones(word: &str) -> Option<u32> {
    ONES.iter().find(|(w, _)| *w == word).map(|(_, v)| *v)
}

fn lookup_tens(word: &str) -> Option<u32> {
    TENS.iter().find(|(w, _)| *w == word).map(|(_, v)| *v)
}

fn lookup_ordinal(word: &str) -> Option<u32> {
    ORDINALS.iter().find(|(w, _)| *w == word).map(|(_, v)| *v)
}

// ─── Greedy cardinal parser ───────────────────────────────────────────────────

/// Try to parse a cardinal number phrase starting at `pos` in `tokens`.
///
/// Returns `(value, tokens_consumed)` or `None` if no number phrase starts at
/// `pos`.  The parse is greedy: "twenty one" always beats just "twenty".
///
/// Handles:
/// - ones/teens (1 token)
/// - tens alone or tens+ones (1–2 tokens)
/// - "N hundred [and] [tens [ones]]" for N in 1–9 (up to 999)
fn try_parse_cardinal(tokens: &[&str], pos: usize) -> Option<(u32, usize)> {
    let tok = tokens.get(pos)?.to_lowercase();
    let tok = tok.as_str();

    // ── Hundreds: "N hundred [and] [tens [ones]]" ─────────────────────────────
    if let Some(hundreds_val) = lookup_ones(tok) {
        let next_is_hundred = tokens
            .get(pos + 1)
            .map(|t| t.to_lowercase() == "hundred")
            .unwrap_or(false);

        if next_is_hundred {
            let mut val = hundreds_val * 100;
            let mut consumed = 2usize; // "N" + "hundred"

            // Optional "and" connector
            let and_pos = pos + consumed;
            let has_and = tokens
                .get(and_pos)
                .map(|t| t.to_lowercase() == "and")
                .unwrap_or(false);
            let rem_pos = if has_and { and_pos + 1 } else { and_pos };

            if let Some(rem_tok) = tokens.get(rem_pos) {
                let rem = rem_tok.to_lowercase();
                let rem = rem.as_str();

                if let Some(tens_val) = lookup_tens(rem) {
                    // "N hundred [and] tens [ones]"
                    consumed = rem_pos + 1 - pos;
                    val += tens_val;
                    // Optional ones after tens
                    if let Some(ones_tok) = tokens.get(pos + consumed) {
                        if let Some(ones_val) = lookup_ones(&ones_tok.to_lowercase()) {
                            val += ones_val;
                            consumed += 1;
                        }
                    }
                    return Some((val, consumed));
                }

                if let Some(ones_val) = lookup_ones(rem) {
                    // "N hundred [and] ones"
                    consumed = rem_pos + 1 - pos;
                    val += ones_val;
                    return Some((val, consumed));
                }
            }

            // Just "N hundred"
            return Some((hundreds_val * 100, 2));
        }
        // Not followed by "hundred" → fall through to ones/teens check.
    }

    // ── Tens + optional ones: "twenty [one]" ─────────────────────────────────
    if let Some(tens_val) = lookup_tens(tok) {
        let mut val = tens_val;
        let mut consumed = 1usize;
        if let Some(ones_tok) = tokens.get(pos + 1) {
            if let Some(ones_val) = lookup_ones(&ones_tok.to_lowercase()) {
                val += ones_val;
                consumed = 2;
            }
        }
        return Some((val, consumed));
    }

    // ── Ones / teens ──────────────────────────────────────────────────────────
    if let Some(ones_val) = lookup_ones(tok) {
        return Some((ones_val, 1));
    }

    None
}

// ─── Hyphen pre-processor ─────────────────────────────────────────────────────

/// Replace hyphens that sit between two alphabetic characters with a space.
///
/// This normalises "twenty-one" → "twenty one" before tokenisation.
/// Hyphens adjacent to digits or punctuation are left intact.
fn split_alpha_hyphens(input: &str) -> String {
    let bytes = input.as_bytes();
    let mut out = String::with_capacity(input.len());
    for (i, b) in bytes.iter().enumerate() {
        if *b == b'-' {
            let before_alpha = i > 0 && bytes[i - 1].is_ascii_alphabetic();
            let after_alpha = i + 1 < bytes.len() && bytes[i + 1].is_ascii_alphabetic();
            if before_alpha && after_alpha {
                out.push(' ');
                continue;
            }
        }
        out.push(*b as char);
    }
    out
}

// ─── NumberNormalizer ─────────────────────────────────────────────────────────

/// Converts spoken number words in transcription text to decimal digits.
///
/// ## Usage
/// ```rust
/// use companion_detection::NumberNormalizer;
///
/// let nn = NumberNormalizer::new();
///
/// // Full pipeline
/// assert_eq!(nn.normalize("First Corinthians chapter thirteen verse thirteen"),
///            "1 Corinthians chapter 13 verse 13");
///
/// // Ordinals only
/// assert_eq!(nn.ordinals_to_digits("Second Timothy chapter two"),
///            "2 Timothy chapter two");
///
/// // Cardinals (including compounds)
/// assert_eq!(nn.cardinals_to_digits("Psalms chapter one hundred and nineteen"),
///            "Psalms chapter 119");
///
/// // Compounds only (single-word cardinals left as words)
/// assert_eq!(nn.compounds_to_digits("chapter twenty one verse three"),
///            "chapter 21 verse three");
/// ```
pub struct NumberNormalizer;

impl NumberNormalizer {
    pub fn new() -> Self {
        Self
    }

    // ── Public API ────────────────────────────────────────────────────────────

    /// Convert ordinal words to digits; leave all other words unchanged.
    ///
    /// Handles the 20 ordinals in [`ORDINALS`] ("first" through "twentieth").
    /// Matching is case-insensitive; canonical output is the bare digit string.
    ///
    /// ```rust
    /// # use companion_detection::NumberNormalizer;
    /// let nn = NumberNormalizer::new();
    /// assert_eq!(nn.ordinals_to_digits("First Corinthians"), "1 Corinthians");
    /// assert_eq!(nn.ordinals_to_digits("second"), "2");
    /// assert_eq!(nn.ordinals_to_digits("three"),  "three"); // cardinal unchanged
    /// ```
    pub fn ordinals_to_digits(&self, input: &str) -> String {
        input
            .split_whitespace()
            .map(|tok| {
                let lower = tok.to_lowercase();
                // Strip trailing punctuation from the key so "first," matches.
                let key = lower.trim_end_matches(|c: char| !c.is_alphabetic());
                if let Some(val) = lookup_ordinal(key) {
                    let suffix = &tok[key.len()..]; // preserve punctuation after key
                    format!("{val}{suffix}")
                } else {
                    tok.to_string()
                }
            })
            .collect::<Vec<_>>()
            .join(" ")
    }

    /// Convert all cardinal number words (1–199) to digits.
    ///
    /// Includes single-word cardinals ("three" → "3"), tens ("twenty" → "20"),
    /// spaced compounds ("twenty one" → "21"), hyphenated compounds
    /// ("twenty-one" → "21"), and hundreds ("one hundred and fifty" → "150").
    ///
    /// ```rust
    /// # use companion_detection::NumberNormalizer;
    /// let nn = NumberNormalizer::new();
    /// assert_eq!(nn.cardinals_to_digits("verse three"),          "verse 3");
    /// assert_eq!(nn.cardinals_to_digits("twenty one"),           "21");
    /// assert_eq!(nn.cardinals_to_digits("one hundred and fifty"),"150");
    /// ```
    pub fn cardinals_to_digits(&self, input: &str) -> String {
        let normalized = split_alpha_hyphens(input);
        let tokens: Vec<&str> = normalized.split_whitespace().collect();
        self.run_scanner(&tokens, false)
    }

    /// Convert only multi-word compound number phrases to digits.
    ///
    /// Single-word cardinals ("three", "twenty") are **not** replaced.
    /// Multi-token phrases ("twenty one", "one hundred and fifty") are replaced.
    ///
    /// This is useful as an intermediate step when you want to preserve
    /// standalone single-word cardinals for other processing.
    ///
    /// ```rust
    /// # use companion_detection::NumberNormalizer;
    /// let nn = NumberNormalizer::new();
    /// assert_eq!(nn.compounds_to_digits("chapter twenty one verse three"),
    ///            "chapter 21 verse three");
    /// assert_eq!(nn.compounds_to_digits("one hundred and nineteen"),
    ///            "119");
    /// assert_eq!(nn.compounds_to_digits("one"),    "one");    // not compound
    /// assert_eq!(nn.compounds_to_digits("twenty"), "twenty"); // not compound
    /// ```
    pub fn compounds_to_digits(&self, input: &str) -> String {
        let normalized = split_alpha_hyphens(input);
        let tokens: Vec<&str> = normalized.split_whitespace().collect();
        self.run_scanner(&tokens, true)
    }

    /// Apply all conversions: ordinals first, then cardinals (including compounds).
    ///
    /// ```rust
    /// # use companion_detection::NumberNormalizer;
    /// let nn = NumberNormalizer::new();
    /// assert_eq!(
    ///     nn.normalize("First Corinthians chapter thirteen verse thirteen"),
    ///     "1 Corinthians chapter 13 verse 13"
    /// );
    /// assert_eq!(
    ///     nn.normalize("Psalms chapter one hundred and nineteen verse one"),
    ///     "Psalms chapter 119 verse 1"
    /// );
    /// ```
    pub fn normalize(&self, input: &str) -> String {
        let s = self.ordinals_to_digits(input);
        self.cardinals_to_digits(&s)
    }

    // ── Internal ──────────────────────────────────────────────────────────────

    /// Core token-scanning loop used by both `cardinals_to_digits` and
    /// `compounds_to_digits`.
    ///
    /// When `compounds_only` is `true`, single-token matches (consumed == 1)
    /// are emitted as-is; only multi-token matches trigger substitution.
    fn run_scanner(&self, tokens: &[&str], compounds_only: bool) -> String {
        let mut out: Vec<String> = Vec::with_capacity(tokens.len());
        let mut i = 0;
        while i < tokens.len() {
            match try_parse_cardinal(tokens, i) {
                Some((val, consumed)) if !compounds_only || consumed > 1 => {
                    out.push(val.to_string());
                    i += consumed;
                }
                _ => {
                    out.push(tokens[i].to_string());
                    i += 1;
                }
            }
        }
        out.join(" ")
    }
}

impl Default for NumberNormalizer {
    fn default() -> Self {
        Self::new()
    }
}

// ─── Tests ────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    // ── Test helpers ──────────────────────────────────────────────────────────

    fn nn() -> NumberNormalizer {
        NumberNormalizer::new()
    }

    /// Convert a number 1–199 to its canonical word form.
    fn to_words(n: u32) -> String {
        match n {
            1 => "one".into(),
            2 => "two".into(),
            3 => "three".into(),
            4 => "four".into(),
            5 => "five".into(),
            6 => "six".into(),
            7 => "seven".into(),
            8 => "eight".into(),
            9 => "nine".into(),
            10 => "ten".into(),
            11 => "eleven".into(),
            12 => "twelve".into(),
            13 => "thirteen".into(),
            14 => "fourteen".into(),
            15 => "fifteen".into(),
            16 => "sixteen".into(),
            17 => "seventeen".into(),
            18 => "eighteen".into(),
            19 => "nineteen".into(),
            20 => "twenty".into(),
            30 => "thirty".into(),
            40 => "forty".into(),
            50 => "fifty".into(),
            60 => "sixty".into(),
            70 => "seventy".into(),
            80 => "eighty".into(),
            90 => "ninety".into(),
            21..=99 => {
                let tens_word = match n / 10 {
                    2 => "twenty",
                    3 => "thirty",
                    4 => "forty",
                    5 => "fifty",
                    6 => "sixty",
                    7 => "seventy",
                    8 => "eighty",
                    9 => "ninety",
                    _ => unreachable!(),
                };
                format!("{tens_word} {}", to_words(n % 10))
            }
            100 => "one hundred".into(),
            101..=199 => format!("one hundred and {}", to_words(n - 100)),
            _ => panic!("to_words: unsupported n={n}"),
        }
    }

    /// Convert n to its ordinal word (first, second, … twentieth).
    fn to_ordinal(n: u32) -> &'static str {
        match n {
            1 => "first",
            2 => "second",
            3 => "third",
            4 => "fourth",
            5 => "fifth",
            6 => "sixth",
            7 => "seventh",
            8 => "eighth",
            9 => "ninth",
            10 => "tenth",
            11 => "eleventh",
            12 => "twelfth",
            13 => "thirteenth",
            14 => "fourteenth",
            15 => "fifteenth",
            16 => "sixteenth",
            17 => "seventeenth",
            18 => "eighteenth",
            19 => "nineteenth",
            20 => "twentieth",
            _ => panic!("to_ordinal: unsupported n={n}"),
        }
    }

    // ── ordinals_to_digits: every ordinal first through twentieth ─────────────

    #[test]
    fn ordinals_first_through_twentieth() {
        let nn = nn();
        for n in 1u32..=20 {
            let word = to_ordinal(n);
            let result = nn.ordinals_to_digits(word);
            assert_eq!(
                result,
                n.to_string(),
                "ordinal '{word}' → '{result}' (expected {n})"
            );
        }
    }

    #[test]
    fn ordinals_case_insensitive() {
        let nn = nn();
        assert_eq!(nn.ordinals_to_digits("FIRST"), "1");
        assert_eq!(nn.ordinals_to_digits("Second"), "2");
        assert_eq!(nn.ordinals_to_digits("tHiRd"), "3");
    }

    #[test]
    fn ordinals_does_not_touch_cardinals() {
        let nn = nn();
        assert_eq!(nn.ordinals_to_digits("one"), "one");
        assert_eq!(nn.ordinals_to_digits("twenty"), "twenty");
        assert_eq!(nn.ordinals_to_digits("nineteen"), "nineteen");
    }

    #[test]
    fn ordinals_in_sentence_first_corinthians() {
        assert_eq!(
            nn().ordinals_to_digits("First Corinthians chapter thirteen verse thirteen"),
            "1 Corinthians chapter thirteen verse thirteen"
        );
    }

    #[test]
    fn ordinals_in_sentence_second_timothy() {
        assert_eq!(
            nn().ordinals_to_digits("Second Timothy chapter two verse fifteen"),
            "2 Timothy chapter two verse fifteen"
        );
    }

    #[test]
    fn ordinals_in_sentence_third_john() {
        assert_eq!(
            nn().ordinals_to_digits("Third John verse fourteen"),
            "3 John verse fourteen"
        );
    }

    #[test]
    fn ordinals_preserves_trailing_comma() {
        assert_eq!(nn().ordinals_to_digits("first,"), "1,");
    }

    #[test]
    fn ordinals_preserves_trailing_period() {
        assert_eq!(nn().ordinals_to_digits("second."), "2.");
    }

    // ── cardinals_to_digits: every number 1–150 ───────────────────────────────

    #[test]
    fn cardinals_all_1_through_150() {
        let nn = nn();
        for n in 1u32..=150 {
            let words = to_words(n);
            let result = nn.cardinals_to_digits(&words);
            assert_eq!(result, n.to_string(), "n={n}: words='{words}' → '{result}'");
        }
    }

    // ── cardinals_to_digits: per-category explicit tests ─────────────────────

    #[test]
    fn cardinal_one() {
        assert_eq!(nn().cardinals_to_digits("one"), "1");
    }
    #[test]
    fn cardinal_nine() {
        assert_eq!(nn().cardinals_to_digits("nine"), "9");
    }
    #[test]
    fn cardinal_ten() {
        assert_eq!(nn().cardinals_to_digits("ten"), "10");
    }
    #[test]
    fn cardinal_eleven() {
        assert_eq!(nn().cardinals_to_digits("eleven"), "11");
    }
    #[test]
    fn cardinal_nineteen() {
        assert_eq!(nn().cardinals_to_digits("nineteen"), "19");
    }
    #[test]
    fn cardinal_twenty() {
        assert_eq!(nn().cardinals_to_digits("twenty"), "20");
    }
    #[test]
    fn cardinal_twenty_one() {
        assert_eq!(nn().cardinals_to_digits("twenty one"), "21");
    }
    #[test]
    fn cardinal_thirty() {
        assert_eq!(nn().cardinals_to_digits("thirty"), "30");
    }
    #[test]
    fn cardinal_thirty_one() {
        assert_eq!(nn().cardinals_to_digits("thirty one"), "31");
    }
    #[test]
    fn cardinal_ninety_nine() {
        assert_eq!(nn().cardinals_to_digits("ninety nine"), "99");
    }
    #[test]
    fn cardinal_one_hundred() {
        assert_eq!(nn().cardinals_to_digits("one hundred"), "100");
    }
    #[test]
    fn cardinal_119() {
        assert_eq!(nn().cardinals_to_digits("one hundred and nineteen"), "119");
    }
    #[test]
    fn cardinal_150() {
        assert_eq!(nn().cardinals_to_digits("one hundred and fifty"), "150");
    }

    // ── cardinals_to_digits: hyphenated compounds ─────────────────────────────

    #[test]
    fn cardinal_hyphenated_twenty_one() {
        assert_eq!(nn().cardinals_to_digits("twenty-one"), "21");
    }

    #[test]
    fn cardinal_hyphenated_thirty_two() {
        assert_eq!(nn().cardinals_to_digits("thirty-two"), "32");
    }

    #[test]
    fn cardinal_hyphenated_forty_five() {
        assert_eq!(nn().cardinals_to_digits("forty-five"), "45");
    }

    #[test]
    fn cardinal_hyphenated_ninety_nine() {
        assert_eq!(nn().cardinals_to_digits("ninety-nine"), "99");
    }

    #[test]
    fn cardinal_hyphenated_in_hundreds() {
        // "one hundred and forty-five"
        assert_eq!(
            nn().cardinals_to_digits("one hundred and forty-five"),
            "145"
        );
    }

    // ── cardinals_to_digits: spaced vs hyphenated equivalence ─────────────────

    #[test]
    fn cardinal_spaced_and_hyphenated_produce_same_result() {
        let nn = nn();
        for n in [21u32, 28, 32, 45, 50, 63, 76, 89, 99] {
            let spaced = to_words(n);
            let hyphenated = spaced.replace(' ', "-");
            assert_eq!(
                nn.cardinals_to_digits(&spaced),
                nn.cardinals_to_digits(&hyphenated),
                "n={n}: spaced='{spaced}' hyphenated='{hyphenated}'"
            );
        }
    }

    // ── cardinals_to_digits: sentence context ─────────────────────────────────

    #[test]
    fn cardinal_in_sentence_chapter_verse() {
        assert_eq!(
            nn().cardinals_to_digits("chapter three verse sixteen"),
            "chapter 3 verse 16"
        );
    }

    #[test]
    fn cardinal_in_sentence_compound_chapter() {
        assert_eq!(
            nn().cardinals_to_digits("Psalms chapter twenty three verse one"),
            "Psalms chapter 23 verse 1"
        );
    }

    #[test]
    fn cardinal_in_sentence_hundred_chapter() {
        assert_eq!(
            nn().cardinals_to_digits("Psalms chapter one hundred and nineteen verse one"),
            "Psalms chapter 119 verse 1"
        );
    }

    #[test]
    fn cardinal_in_sentence_hundred_fifty() {
        assert_eq!(
            nn().cardinals_to_digits("The book of Psalms has one hundred and fifty chapters"),
            "The book of Psalms has 150 chapters"
        );
    }

    // ── cardinals_to_digits: hundred without "and" ────────────────────────────

    #[test]
    fn cardinal_one_hundred_twenty_without_and() {
        // "one hundred twenty" (no "and") must still parse as 120.
        assert_eq!(nn().cardinals_to_digits("one hundred twenty"), "120");
    }

    #[test]
    fn cardinal_one_hundred_twenty_one_without_and() {
        assert_eq!(nn().cardinals_to_digits("one hundred twenty one"), "121");
    }

    // ── cardinals_to_digits: edge cases ───────────────────────────────────────

    #[test]
    fn cardinal_empty_string() {
        assert_eq!(nn().cardinals_to_digits(""), "");
    }

    #[test]
    fn cardinal_only_non_number_words_unchanged() {
        let s = "John chapter verse";
        assert_eq!(nn().cardinals_to_digits(s), s);
    }

    #[test]
    fn cardinal_digits_already_unchanged() {
        assert_eq!(
            nn().cardinals_to_digits("chapter 3 verse 16"),
            "chapter 3 verse 16"
        );
    }

    #[test]
    fn cardinal_mixed_words_and_digits() {
        assert_eq!(
            nn().cardinals_to_digits("chapter three verse 16"),
            "chapter 3 verse 16"
        );
    }

    #[test]
    fn cardinal_one_hundred_alone_no_trailing_and_consumed() {
        // "one hundred and" with nothing after — only "one hundred" consumed.
        assert_eq!(nn().cardinals_to_digits("one hundred and"), "100 and");
    }

    #[test]
    fn cardinal_one_not_followed_by_hundred() {
        assert_eq!(
            nn().cardinals_to_digits("chapter one verse one"),
            "chapter 1 verse 1"
        );
    }

    // ── compounds_to_digits ───────────────────────────────────────────────────

    #[test]
    fn compounds_twenty_one() {
        assert_eq!(nn().compounds_to_digits("twenty one"), "21");
    }

    #[test]
    fn compounds_twenty_eight() {
        assert_eq!(nn().compounds_to_digits("twenty eight"), "28");
    }

    #[test]
    fn compounds_one_hundred_and_nineteen() {
        assert_eq!(nn().compounds_to_digits("one hundred and nineteen"), "119");
    }

    #[test]
    fn compounds_does_not_replace_single_word_cardinal() {
        assert_eq!(nn().compounds_to_digits("three"), "three");
        assert_eq!(nn().compounds_to_digits("twenty"), "twenty");
        assert_eq!(nn().compounds_to_digits("one"), "one");
    }

    #[test]
    fn compounds_in_sentence_leaves_singles() {
        // "twenty one" → "21"; standalone "three" stays.
        assert_eq!(
            nn().compounds_to_digits("chapter twenty one verse three"),
            "chapter 21 verse three"
        );
    }

    #[test]
    fn compounds_one_hundred_is_replaced() {
        // "one hundred" consumes 2 tokens → replaced.
        assert_eq!(nn().compounds_to_digits("one hundred"), "100");
    }

    #[test]
    fn compounds_hyphenated_twenty_one() {
        assert_eq!(nn().compounds_to_digits("twenty-one"), "21");
    }

    // ── normalize: full pipeline ──────────────────────────────────────────────

    #[test]
    fn normalize_first_corinthians_thirteen() {
        assert_eq!(
            nn().normalize("First Corinthians chapter thirteen verse thirteen"),
            "1 Corinthians chapter 13 verse 13"
        );
    }

    #[test]
    fn normalize_second_thessalonians_three_sixteen() {
        assert_eq!(
            nn().normalize("Second Thessalonians chapter three verse sixteen"),
            "2 Thessalonians chapter 3 verse 16"
        );
    }

    #[test]
    fn normalize_third_john() {
        assert_eq!(
            nn().normalize("Third John verse fourteen"),
            "3 John verse 14"
        );
    }

    #[test]
    fn normalize_psalms_hundred_nineteen() {
        assert_eq!(
            nn().normalize("Psalms chapter one hundred and nineteen verse one"),
            "Psalms chapter 119 verse 1"
        );
    }

    #[test]
    fn normalize_psalms_twenty_three() {
        assert_eq!(
            nn().normalize("Psalms chapter twenty three verse one"),
            "Psalms chapter 23 verse 1"
        );
    }

    #[test]
    fn normalize_john_three_sixteen() {
        assert_eq!(
            nn().normalize("John chapter three verse sixteen"),
            "John chapter 3 verse 16"
        );
    }

    #[test]
    fn normalize_ordinal_and_compound_in_same_string() {
        assert_eq!(
            nn().normalize("Second Kings chapter twenty two verse one"),
            "2 Kings chapter 22 verse 1"
        );
    }

    #[test]
    fn normalize_all_digits_already_unchanged() {
        assert_eq!(nn().normalize("John 3:16"), "John 3:16");
    }

    #[test]
    fn normalize_empty_string() {
        assert_eq!(nn().normalize(""), "");
    }

    // ── normalize: ordinals for all Bible book prefixes ───────────────────────

    #[test]
    fn normalize_ordinal_book_prefixes_1_through_3() {
        let nn = nn();
        // Books in the Bible go up to "Third" (3 John).
        let cases = [
            ("First Samuel", "1 Samuel"),
            ("Second Samuel", "2 Samuel"),
            ("First Kings", "1 Kings"),
            ("Second Kings", "2 Kings"),
            ("First Chronicles", "1 Chronicles"),
            ("Second Chronicles", "2 Chronicles"),
            ("First Corinthians", "1 Corinthians"),
            ("Second Corinthians", "2 Corinthians"),
            ("First Thessalonians", "1 Thessalonians"),
            ("Second Thessalonians", "2 Thessalonians"),
            ("First Timothy", "1 Timothy"),
            ("Second Timothy", "2 Timothy"),
            ("First Peter", "1 Peter"),
            ("Second Peter", "2 Peter"),
            ("First John", "1 John"),
            ("Second John", "2 John"),
            ("Third John", "3 John"),
        ];
        for (input, expected) in cases {
            assert_eq!(nn.normalize(input), expected, "input: '{input}'");
        }
    }

    // ── Edge cases: "and" in hundred phrase ───────────────────────────────────

    #[test]
    fn cardinal_one_hundred_and_one() {
        assert_eq!(nn().cardinals_to_digits("one hundred and one"), "101");
    }

    #[test]
    fn cardinal_one_hundred_and_ten() {
        assert_eq!(nn().cardinals_to_digits("one hundred and ten"), "110");
    }

    #[test]
    fn cardinal_one_hundred_and_forty() {
        assert_eq!(nn().cardinals_to_digits("one hundred and forty"), "140");
    }

    #[test]
    fn cardinal_one_hundred_and_forty_nine() {
        assert_eq!(
            nn().cardinals_to_digits("one hundred and forty nine"),
            "149"
        );
    }

    // ── Psalms: complete chapter range 1–150 via normalize ────────────────────

    #[test]
    fn normalize_psalms_chapters_1_through_150() {
        let nn = nn();
        for ch in 1u32..=150 {
            let ch_words = to_words(ch);
            let input = format!("Psalms chapter {ch_words} verse one");
            let result = nn.normalize(&input);
            assert_eq!(
                result,
                format!("Psalms chapter {ch} verse 1"),
                "ch={ch}: input='{input}'"
            );
        }
    }
}
