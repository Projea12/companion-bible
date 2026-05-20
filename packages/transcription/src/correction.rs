//! Post-processing correction layer for Nigerian-English Whisper errors.
//!
//! Whisper is trained primarily on General-American and British English.  When
//! a Nigerian speaker reads from the Bible, several phonological features of
//! Nigerian English cause consistent transcription errors:
//!
//! | Feature                        | Example                               |
//! |-------------------------------|---------------------------------------|
//! | Final consonant deletion       | "Ephesians" → "Ephesian"              |
//! | /θ/ realised as /t/            | "Corinthians" → "Corentin"            |
//! | Vowel centralisation           | "Corinthians" → "Corenten"            |
//! | Final /h/ elision              | "Nehemiah" → "Nehemia"                |
//! | /ph/ → /f/ substitution        | "Philippians" → "Filipians"           |
//! | Syllable-timed stress          | "Phi·le·mon" as three equal beats     |
//! | Spurious pluralisation         | "Revelation" → "Revelations"          |
//! | Double-consonant simplification| "Habakkuk" → "Habakuk"                |
//!
//! ## Design
//!
//! The correction table maps lowercase, punctuation-stripped word forms to
//! their canonical Bible spelling.  [`correct_text`] processes each
//! whitespace-delimited token independently: it strips leading/trailing
//! punctuation, looks up the lowercase core, replaces the alphabetic span
//! if a match is found, and re-attaches the surrounding punctuation.
//!
//! This design keeps the correction strictly local — it never needs sentence
//! context — and fast: one linear scan over the small table per token.
//!
//! ## Integration point
//!
//! Call [`correct_batch`] immediately after `WhisperModel::transcribe` returns,
//! before the deduplication layer.  The transcription loop in `transcriber.rs`
//! already calls this.

use crate::transcript::TranscriptionSegment;

// ─── Correction table ─────────────────────────────────────────────────────────

/// `(wrong_form_lowercase, canonical_form)`.
///
/// Every entry is lowercase on the left — matching is always case-insensitive.
/// The right-hand side is the canonical Title Case Bible spelling.
///
/// ## Categories
///
/// - **Unambiguous phonetic errors**: the wrong form is not a valid English
///   word in any context (e.g. "corentin", "habakuk").  Always safe to correct.
///
/// - **Dropped suffix**: the form is a valid English adjective/noun but in a
///   sermon context almost certainly refers to the book (e.g. "ephesian" for
///   "Ephesians").  Annotated with `[suffix]`.
///
/// - **Spurious plural**: the canonical book name is singular but Nigerian
///   English speakers add a final *-s* (e.g. "Revelations").  Annotated with
///   `[plural]`.
pub const CORRECTIONS: &[(&str, &str)] = &[
    // ── Corinthians ───────────────────────────────────────────────────────────
    // /θ/ → /t/ collapses "inth" → "ent"; vowel shift turns the final syllable.
    ("corentin",   "Corinthians"),
    ("corentins",  "Corinthians"),
    ("corenten",   "Corinthians"),
    ("corentan",   "Corinthians"),
    ("corintin",   "Corinthians"),
    ("corinten",   "Corinthians"),
    ("corinthen",  "Corinthians"),
    ("corintian",  "Corinthians"), // 'h' dropped from 'th'
    ("corintians", "Corinthians"), // 'h' dropped, plural retained
    ("corinthian", "Corinthians"), // [suffix] dropped trailing 's'

    // ── Ephesians ─────────────────────────────────────────────────────────────
    // Final consonant deletion: /z/ is dropped leaving the adjective form.
    ("ephesian", "Ephesians"), // [suffix]

    // ── Philippians ───────────────────────────────────────────────────────────
    // /ph/ → /f/ is very common in Nigerian English.
    ("philippian",  "Philippians"), // [suffix] dropped 's'
    ("philippins",  "Philippians"), // 'ian' compressed to 'in'
    ("filipian",    "Philippians"), // ph→f
    ("filipians",   "Philippians"),
    ("philipian",   "Philippians"), // single 'l'
    ("philipians",  "Philippians"),
    ("phillipian",  "Philippians"), // double 'l', dropped 's'
    ("phillipians", "Philippians"),

    // ── Philemon ──────────────────────────────────────────────────────────────
    // Syllable-timed speech produces equal-stress syllables; Whisper sometimes
    // inserts hyphens between them or shifts the medial vowel.
    ("phi-le-mon", "Philemon"), // hyphenated slow speech
    ("phi-leman",  "Philemon"),
    ("phileman",   "Philemon"), // o→a vowel shift
    ("philoemon",  "Philemon"), // epenthetic 'o'

    // ── Galatians ─────────────────────────────────────────────────────────────
    // "-tion" endings are often realised as "-shun"; Whisper may also drop 'i'.
    ("galatian",  "Galatians"), // [suffix]
    ("galation",  "Galatians"), // 'i' in 'tians' dropped
    ("galations", "Galatians"),
    ("galasians", "Galatians"), // t→s substitution

    // ── Colossians ────────────────────────────────────────────────────────────
    ("colossian", "Colossians"), // [suffix]
    ("colosian",  "Colossians"), // single 's'
    ("colosians", "Colossians"),

    // ── Thessalonians ─────────────────────────────────────────────────────────
    // Initial /θ/ elided; complex four-syllable word invites multiple errors.
    ("thessalonian",  "Thessalonians"), // [suffix] dropped 's'
    ("thessalonia",   "Thessalonians"), // truncated
    ("tessalonian",   "Thessalonians"), // Th→T
    ("tessalonians",  "Thessalonians"),
    ("tesalonian",    "Thessalonians"), // Th→T, single 's'
    ("tesalonians",   "Thessalonians"),
    ("tessa-lonians", "Thessalonians"), // hyphenated

    // ── Habakkuk ──────────────────────────────────────────────────────────────
    // Double-k is rare in English; single-k and c-substitutions are common.
    ("habakuk",  "Habakkuk"),
    ("habacuc",  "Habakkuk"), // k→c (Latin Septuagint spelling leaks in)
    ("habakku",  "Habakkuk"), // transposed double-k
    ("habaku",   "Habakkuk"), // truncated final syllable

    // ── Ecclesiastes ──────────────────────────────────────────────────────────
    // Longest book name; often truncated or phonetically respelled.
    ("ecclesiast",   "Ecclesiastes"), // truncated
    ("ecclesiaste",  "Ecclesiastes"), // dropped final 's'
    ("eklesiastes",  "Ecclesiastes"), // phonetic Nigerian spelling: ecc→ek
    ("eklesiast",    "Ecclesiastes"), // truncated phonetic form

    // ── Deuteronomy ───────────────────────────────────────────────────────────
    // Middle /ə/ vowel in "ter" is often lost in rapid speech.
    ("deutronomy",  "Deuteronomy"), // dropped medial 'e'
    ("duteronomy",  "Deuteronomy"), // /dju/→/du/
    ("deuteronomi", "Deuteronomy"), // final y→i

    // ── Nehemiah ──────────────────────────────────────────────────────────────
    // Final /h/ regularly elided in Nigerian English.
    ("nehemia", "Nehemiah"),

    // ── Zephaniah ─────────────────────────────────────────────────────────────
    // Final /h/ elision; /e/→/a/ vowel shift.
    ("zephania", "Zephaniah"),
    ("zaphania", "Zephaniah"), // Ze→Za

    // ── Zechariah ─────────────────────────────────────────────────────────────
    // Initial Ze→Za substitution; final /h/ sometimes elided.
    ("zacharia",  "Zechariah"), // Ze→Za, dropped 'h'
    ("zachariah", "Zechariah"), // Ze→Za (Zachariah is a distinct name but
                                // Whisper almost always means the OT prophet here)

    // ── Malachi ───────────────────────────────────────────────────────────────
    // /tʃ/→/k/ consonant substitution; spelling uncertainty.
    ("malaki",  "Malachi"),
    ("malacci", "Malachi"), // double c variant
    ("malacy",  "Malachi"), // 'chi'→'cy'

    // ── Obadiah ───────────────────────────────────────────────────────────────
    // Final /h/ elision.
    ("obadia", "Obadiah"),

    // ── Nahum ─────────────────────────────────────────────────────────────────
    // /h/ in coda position frequently dropped; "Naum" is not a common English word.
    ("naum", "Nahum"),

    // ── Lamentations ──────────────────────────────────────────────────────────
    // Final /z/ dropped from "-tions" cluster.
    ("lamentation", "Lamentations"), // [suffix]

    // ── Revelation ────────────────────────────────────────────────────────────
    // [plural] "Revelations" is the most frequent book-name error across all
    // Nigerian English speakers.  The canonical book is singular.
    ("revelations", "Revelation"),

    // ── Chronicles ────────────────────────────────────────────────────────────
    // /h/ dropped from "ch" cluster; trailing 's' sometimes lost.
    ("cronicles", "Chronicles"), // 'h' dropped from 'ch'
    ("cronicle",  "Chronicles"),

    // ── Hebrews ───────────────────────────────────────────────────────────────
    // [suffix] Final /z/ dropped.  "Hebrew" as an adjective is valid, but in a
    // sermon context "Hebrew chapter 11" unambiguously means the epistle.
    ("hebrew", "Hebrews"),
];

// ─── Fuzzy matching ───────────────────────────────────────────────────────────

/// Canonical Bible book names used for fuzzy matching.
///
/// Only single-word names with ≥ 5 characters are included.  Short names
/// (Ruth, Joel, Amos, Jude…) are excluded — their edit-distance neighbourhood
/// overlaps with common English words, producing too many false positives.
const CANONICAL_BOOK_NAMES: &[&str] = &[
    "Genesis", "Exodus", "Leviticus", "Numbers", "Deuteronomy",
    "Joshua", "Judges", "Esther", "Psalms", "Proverbs",
    "Ecclesiastes", "Isaiah", "Jeremiah", "Lamentations", "Ezekiel",
    "Daniel", "Hosea", "Obadiah", "Micah", "Nahum", "Habakkuk",
    "Zephaniah", "Haggai", "Zechariah", "Malachi",
    "Matthew", "Romans", "Corinthians", "Galatians", "Ephesians",
    "Philippians", "Colossians", "Thessalonians", "Timothy", "Titus",
    "Philemon", "Hebrews", "James", "Peter", "Revelation",
    "Samuel", "Kings", "Chronicles", "Nehemiah",
];

/// Classic dynamic-programming Levenshtein edit distance.
fn levenshtein(a: &str, b: &str) -> usize {
    let a: Vec<char> = a.chars().collect();
    let b: Vec<char> = b.chars().collect();
    let m = a.len();
    let n = b.len();
    let mut prev: Vec<usize> = (0..=n).collect();
    let mut curr = vec![0usize; n + 1];
    for i in 1..=m {
        curr[0] = i;
        for j in 1..=n {
            curr[j] = if a[i - 1] == b[j - 1] {
                prev[j - 1]
            } else {
                1 + prev[j - 1].min(prev[j]).min(curr[j - 1])
            };
        }
        std::mem::swap(&mut prev, &mut curr);
    }
    prev[n]
}

// ─── Public API ───────────────────────────────────────────────────────────────

/// Apply all known Nigerian-English corrections to every segment in `batch`.
///
/// This is the primary integration point — call it immediately after
/// `WhisperModel::transcribe` returns, before the deduplication pass.
pub fn correct_batch(batch: &mut Vec<TranscriptionSegment>) {
    for seg in batch.iter_mut() {
        seg.text = correct_text(&seg.text);
    }
}

/// Apply corrections to a single segment (modifies `seg.text` in place).
pub fn correct_segment(seg: &mut TranscriptionSegment) {
    seg.text = correct_text(&seg.text);
}

/// Apply corrections to a free-form text string.
///
/// Each whitespace-delimited token is corrected independently: leading/trailing
/// punctuation is preserved, only the alphabetic core is matched and replaced.
///
/// ```rust
/// use companion_transcription::correction::correct_text;
///
/// assert_eq!(correct_text("Corentin chapter 3"),   "Corinthians chapter 3");
/// assert_eq!(correct_text("Ephesian 1:1"),          "Ephesians 1:1");
/// assert_eq!(correct_text("Habakuk 2:4."),          "Habakkuk 2:4.");
/// assert_eq!(correct_text("The book of Revelations"), "The book of Revelation");
/// ```
pub fn correct_text(input: &str) -> String {
    let tokens: Vec<&str> = input.split_whitespace().collect();
    if tokens.is_empty() {
        return input.to_string();
    }
    tokens
        .iter()
        .map(|t| correct_token(t))
        .collect::<Vec<_>>()
        .join(" ")
}

// ─── Internal ─────────────────────────────────────────────────────────────────

/// Correct a single whitespace-delimited token.
///
/// The alphabetic span of the token (skipping leading/trailing punctuation) is
/// extracted, lowercased, and looked up in [`CORRECTIONS`].  If a match is
/// found the span is replaced with the canonical form while the surrounding
/// punctuation characters are kept intact.
fn correct_token(token: &str) -> String {
    // Locate the first alphabetic byte.
    let alpha_start = match token.find(|c: char| c.is_alphabetic()) {
        Some(i) => i,
        None => return token.to_string(), // purely punctuation / numeric token
    };

    // Locate the last alphabetic byte and advance past it.
    let alpha_end = {
        let last_idx = token
            .rfind(|c: char| c.is_alphabetic())
            .expect("alpha_start succeeded so rfind must too");
        // Advance past the last character (multi-byte safe).
        last_idx + token[last_idx..].chars().next().map(|c| c.len_utf8()).unwrap_or(1)
    };

    let word = &token[alpha_start..alpha_end];
    let lower = word.to_lowercase();

    for (wrong, right) in CORRECTIONS {
        if lower == *wrong {
            let prefix = &token[..alpha_start];
            let suffix = &token[alpha_end..];
            return format!("{prefix}{right}{suffix}");
        }
    }

    // Fuzzy fallback: catch novel transcription errors not yet in CORRECTIONS.
    // Only fire for tokens of 5+ chars that are NOT already a canonical book name.
    // Edit distance 1 for 5–7 char words, distance 2 for 8+ char words.
    if word.len() >= 5 {
        // Guard: token already matches a canonical name → nothing to correct.
        let already_canonical = CANONICAL_BOOK_NAMES
            .iter()
            .any(|c| c.to_lowercase() == lower);
        if !already_canonical {
            let max_dist = if word.len() >= 8 { 2 } else { 1 };
            let mut best: Option<(&str, usize)> = None;
            for &canonical in CANONICAL_BOOK_NAMES {
                let target_lower = canonical.to_lowercase();
                // Skip targets whose length differs too much — saves compute and
                // prevents "Genesis" (7) matching "Ezra" (4) at distance 5.
                if lower.len().abs_diff(target_lower.len()) > max_dist + 1 {
                    continue;
                }
                let d = levenshtein(&lower, &target_lower);
                if d > 0 && d <= max_dist {
                    match best {
                        None => best = Some((canonical, d)),
                        Some((_, bd)) if d < bd => best = Some((canonical, d)),
                        _ => {}
                    }
                }
            }
            if let Some((canonical, _)) = best {
                let prefix = &token[..alpha_start];
                let suffix = &token[alpha_end..];
                return format!("{prefix}{canonical}{suffix}");
            }
        }
    }

    token.to_string()
}

// ─── Tests ────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use crate::transcript::TranscriptionSegment;

    // ── Helper ────────────────────────────────────────────────────────────────

    fn seg(text: &str) -> TranscriptionSegment {
        TranscriptionSegment {
            text: text.to_string(),
            audio_start_ms: 0,
            audio_end_ms: 500,
            whisper_confidence: 0.9,
            is_duplicate: false,
            context_window: String::new(),
        }
    }

    // ── One test per CORRECTIONS entry ────────────────────────────────────────

    // Corinthians
    #[test] fn corentin()    { assert_eq!(correct_text("corentin"),   "Corinthians"); }
    #[test] fn corentins()   { assert_eq!(correct_text("corentins"),  "Corinthians"); }
    #[test] fn corenten()    { assert_eq!(correct_text("corenten"),   "Corinthians"); }
    #[test] fn corentan()    { assert_eq!(correct_text("corentan"),   "Corinthians"); }
    #[test] fn corintin()    { assert_eq!(correct_text("corintin"),   "Corinthians"); }
    #[test] fn corinten()    { assert_eq!(correct_text("corinten"),   "Corinthians"); }
    #[test] fn corinthen()   { assert_eq!(correct_text("corinthen"),  "Corinthians"); }
    #[test] fn corintian()   { assert_eq!(correct_text("corintian"),  "Corinthians"); }
    #[test] fn corintians()  { assert_eq!(correct_text("corintians"), "Corinthians"); }
    #[test] fn corinthian()  { assert_eq!(correct_text("corinthian"), "Corinthians"); }

    // Ephesians
    #[test] fn ephesian()    { assert_eq!(correct_text("ephesian"),   "Ephesians"); }

    // Philippians
    #[test] fn philippian()  { assert_eq!(correct_text("philippian"),  "Philippians"); }
    #[test] fn philippins()  { assert_eq!(correct_text("philippins"),  "Philippians"); }
    #[test] fn filipian()    { assert_eq!(correct_text("filipian"),    "Philippians"); }
    #[test] fn filipians()   { assert_eq!(correct_text("filipians"),   "Philippians"); }
    #[test] fn philipian()   { assert_eq!(correct_text("philipian"),   "Philippians"); }
    #[test] fn philipians()  { assert_eq!(correct_text("philipians"),  "Philippians"); }
    #[test] fn phillipian()  { assert_eq!(correct_text("phillipian"),  "Philippians"); }
    #[test] fn phillipians() { assert_eq!(correct_text("phillipians"), "Philippians"); }

    // Philemon
    #[test] fn phi_le_mon()  { assert_eq!(correct_text("Phi-le-mon"), "Philemon"); }
    #[test] fn phi_leman()   { assert_eq!(correct_text("Phi-leman"),  "Philemon"); }
    #[test] fn phileman()    { assert_eq!(correct_text("phileman"),   "Philemon"); }
    #[test] fn philoemon()   { assert_eq!(correct_text("philoemon"),  "Philemon"); }

    // Galatians
    #[test] fn galatian()    { assert_eq!(correct_text("galatian"),  "Galatians"); }
    #[test] fn galation()    { assert_eq!(correct_text("galation"),  "Galatians"); }
    #[test] fn galations()   { assert_eq!(correct_text("galations"), "Galatians"); }
    #[test] fn galasians()   { assert_eq!(correct_text("galasians"), "Galatians"); }

    // Colossians
    #[test] fn colossian()   { assert_eq!(correct_text("colossian"), "Colossians"); }
    #[test] fn colosian()    { assert_eq!(correct_text("colosian"),  "Colossians"); }
    #[test] fn colosians()   { assert_eq!(correct_text("colosians"), "Colossians"); }

    // Thessalonians
    #[test] fn thessalonian()  { assert_eq!(correct_text("thessalonian"),  "Thessalonians"); }
    #[test] fn thessalonia()   { assert_eq!(correct_text("thessalonia"),   "Thessalonians"); }
    #[test] fn tessalonian()   { assert_eq!(correct_text("tessalonian"),   "Thessalonians"); }
    #[test] fn tessalonians()  { assert_eq!(correct_text("tessalonians"),  "Thessalonians"); }
    #[test] fn tesalonian()    { assert_eq!(correct_text("tesalonian"),    "Thessalonians"); }
    #[test] fn tesalonians()   { assert_eq!(correct_text("tesalonians"),   "Thessalonians"); }
    #[test] fn tessa_lonians() { assert_eq!(correct_text("tessa-lonians"), "Thessalonians"); }

    // Habakkuk
    #[test] fn habakuk()     { assert_eq!(correct_text("habakuk"),  "Habakkuk"); }
    #[test] fn habacuc()     { assert_eq!(correct_text("habacuc"),  "Habakkuk"); }
    #[test] fn habakku()     { assert_eq!(correct_text("habakku"),  "Habakkuk"); }
    #[test] fn habaku()      { assert_eq!(correct_text("habaku"),   "Habakkuk"); }

    // Ecclesiastes
    #[test] fn ecclesiast()   { assert_eq!(correct_text("ecclesiast"),   "Ecclesiastes"); }
    #[test] fn ecclesiaste()  { assert_eq!(correct_text("ecclesiaste"),  "Ecclesiastes"); }
    #[test] fn eklesiastes()  { assert_eq!(correct_text("eklesiastes"),  "Ecclesiastes"); }
    #[test] fn eklesiast()    { assert_eq!(correct_text("eklesiast"),    "Ecclesiastes"); }

    // Deuteronomy
    #[test] fn deutronomy()   { assert_eq!(correct_text("deutronomy"),  "Deuteronomy"); }
    #[test] fn duteronomy()   { assert_eq!(correct_text("duteronomy"),  "Deuteronomy"); }
    #[test] fn deuteronomi()  { assert_eq!(correct_text("deuteronomi"), "Deuteronomy"); }

    // Nehemiah
    #[test] fn nehemia()     { assert_eq!(correct_text("nehemia"),  "Nehemiah"); }

    // Zephaniah
    #[test] fn zephania()    { assert_eq!(correct_text("zephania"), "Zephaniah"); }
    #[test] fn zaphania()    { assert_eq!(correct_text("zaphania"), "Zephaniah"); }

    // Zechariah
    #[test] fn zacharia()    { assert_eq!(correct_text("zacharia"),  "Zechariah"); }
    #[test] fn zachariah()   { assert_eq!(correct_text("zachariah"), "Zechariah"); }

    // Malachi
    #[test] fn malaki()      { assert_eq!(correct_text("malaki"),  "Malachi"); }
    #[test] fn malacci()     { assert_eq!(correct_text("malacci"), "Malachi"); }
    #[test] fn malacy()      { assert_eq!(correct_text("malacy"),  "Malachi"); }

    // Obadiah
    #[test] fn obadia()      { assert_eq!(correct_text("obadia"), "Obadiah"); }

    // Nahum
    #[test] fn naum()        { assert_eq!(correct_text("naum"), "Nahum"); }

    // Lamentations
    #[test] fn lamentation() { assert_eq!(correct_text("lamentation"), "Lamentations"); }

    // Revelation
    #[test] fn revelations() { assert_eq!(correct_text("revelations"), "Revelation"); }

    // Chronicles
    #[test] fn cronicles()   { assert_eq!(correct_text("cronicles"), "Chronicles"); }
    #[test] fn cronicle()    { assert_eq!(correct_text("cronicle"),  "Chronicles"); }

    // Hebrews
    #[test] fn hebrew()      { assert_eq!(correct_text("hebrew"), "Hebrews"); }

    // ── Sentence-level corrections ────────────────────────────────────────────

    #[test]
    fn correction_in_sentence_corinthians() {
        assert_eq!(
            correct_text("Turn to First Corentin chapter 13."),
            "Turn to First Corinthians chapter 13."
        );
    }

    #[test]
    fn correction_in_sentence_ephesians() {
        assert_eq!(
            correct_text("Open your Bibles to Ephesian chapter 1 verse 3."),
            "Open your Bibles to Ephesians chapter 1 verse 3."
        );
    }

    #[test]
    fn correction_in_sentence_philippians() {
        assert_eq!(
            correct_text("As Paul writes in Filipian chapter 4 verse 13,"),
            "As Paul writes in Philippians chapter 4 verse 13,"
        );
    }

    #[test]
    fn correction_in_sentence_revelation() {
        assert_eq!(
            correct_text("The book of Revelations chapter 22 verse 20."),
            "The book of Revelation chapter 22 verse 20."
        );
    }

    #[test]
    fn correction_in_sentence_habakkuk() {
        assert_eq!(
            correct_text("Habakuk chapter 2 verse 4 says the just shall live by faith."),
            "Habakkuk chapter 2 verse 4 says the just shall live by faith."
        );
    }

    #[test]
    fn correction_multiple_errors_in_one_sentence() {
        assert_eq!(
            correct_text("From Ephesian chapter 1 to Corentin chapter 3."),
            "From Ephesians chapter 1 to Corinthians chapter 3."
        );
    }

    // ── Punctuation preservation ──────────────────────────────────────────────

    #[test]
    fn trailing_period_preserved() {
        assert_eq!(correct_text("Corentin."), "Corinthians.");
    }

    #[test]
    fn trailing_comma_preserved() {
        assert_eq!(correct_text("Ephesian,"), "Ephesians,");
    }

    #[test]
    fn trailing_exclamation_preserved() {
        assert_eq!(correct_text("Revelations!"), "Revelation!");
    }

    #[test]
    fn colon_suffix_preserved() {
        // "Ephesian:" (e.g. "Ephesian: chapter 1")
        assert_eq!(correct_text("Ephesian:"), "Ephesians:");
    }

    #[test]
    fn parentheses_preserved() {
        assert_eq!(correct_text("(Corentin)"), "(Corinthians)");
    }

    // ── Case sensitivity ──────────────────────────────────────────────────────

    #[test]
    fn match_is_case_insensitive_lower() {
        assert_eq!(correct_text("corentin"),  "Corinthians");
    }

    #[test]
    fn match_is_case_insensitive_title() {
        assert_eq!(correct_text("Corentin"),  "Corinthians");
    }

    #[test]
    fn match_is_case_insensitive_upper() {
        // ALL-CAPS input: the canonical form (Title Case) is substituted.
        assert_eq!(correct_text("CORENTIN"),  "Corinthians");
    }

    #[test]
    fn match_is_case_insensitive_mixed() {
        assert_eq!(correct_text("cOrEnTiN"),  "Corinthians");
    }

    // ── Identity: already-correct text unchanged ──────────────────────────────

    #[test]
    fn corinthians_unchanged()   { assert_eq!(correct_text("Corinthians"),  "Corinthians"); }
    #[test]
    fn ephesians_unchanged()     { assert_eq!(correct_text("Ephesians"),    "Ephesians"); }
    #[test]
    fn revelation_unchanged()    { assert_eq!(correct_text("Revelation"),   "Revelation"); }
    #[test]
    fn habakkuk_unchanged()      { assert_eq!(correct_text("Habakkuk"),     "Habakkuk"); }
    #[test]
    fn ecclesiastes_unchanged()  { assert_eq!(correct_text("Ecclesiastes"), "Ecclesiastes"); }
    #[test]
    fn philemon_unchanged()      { assert_eq!(correct_text("Philemon"),     "Philemon"); }
    #[test]
    fn philippians_unchanged()   { assert_eq!(correct_text("Philippians"),  "Philippians"); }
    #[test]
    fn zechariah_unchanged()     { assert_eq!(correct_text("Zechariah"),    "Zechariah"); }

    #[test]
    fn unrelated_sentence_unchanged() {
        let s = "For God so loved the world that he gave his only Son.";
        assert_eq!(correct_text(s), s);
    }

    #[test]
    fn numbers_and_punctuation_unchanged() {
        assert_eq!(correct_text("3:16"), "3:16");
        assert_eq!(correct_text("22"),   "22");
    }

    #[test]
    fn empty_string_unchanged() {
        assert_eq!(correct_text(""), "");
    }

    #[test]
    fn whitespace_only_returned_as_is() {
        // No alphabetic tokens — early-exit returns the original string intact.
        // Whisper never emits whitespace-only segments so this is an edge case.
        assert_eq!(correct_text("   "), "   ");
    }

    // ── correct_segment ───────────────────────────────────────────────────────

    #[test]
    fn correct_segment_mutates_text() {
        let mut s = seg("First Corentin chapter 13 verse 13.");
        correct_segment(&mut s);
        assert_eq!(s.text, "First Corinthians chapter 13 verse 13.");
    }

    #[test]
    fn correct_segment_leaves_other_fields_intact() {
        let mut s = seg("Ephesian 1:3");
        s.audio_start_ms = 1_000;
        s.audio_end_ms = 3_500;
        s.whisper_confidence = 0.87;
        s.is_duplicate = false;
        correct_segment(&mut s);
        assert_eq!(s.audio_start_ms, 1_000);
        assert_eq!(s.audio_end_ms, 3_500);
        assert!((s.whisper_confidence - 0.87).abs() < 1e-6);
        assert!(!s.is_duplicate);
    }

    // ── correct_batch ─────────────────────────────────────────────────────────

    #[test]
    fn correct_batch_applies_to_all_segments() {
        let mut batch = vec![
            seg("First Corentin chapter 13."),
            seg("For God so loved the world."),
            seg("Revelations chapter 22 verse 20."),
        ];
        correct_batch(&mut batch);
        assert_eq!(batch[0].text, "First Corinthians chapter 13.");
        assert_eq!(batch[1].text, "For God so loved the world.");
        assert_eq!(batch[2].text, "Revelation chapter 22 verse 20.");
    }

    #[test]
    fn correct_batch_empty_is_safe() {
        let mut batch: Vec<TranscriptionSegment> = vec![];
        correct_batch(&mut batch); // must not panic
    }

    // ── Ordinal prefixes are preserved ───────────────────────────────────────

    #[test]
    fn first_corinthians_prefix_preserved() {
        assert_eq!(correct_text("First Corentin"), "First Corinthians");
    }

    #[test]
    fn second_thessalonians_prefix_preserved() {
        assert_eq!(correct_text("Second Tessalonian"), "Second Thessalonians");
    }

    #[test]
    fn first_thessalonians_number_prefix_preserved() {
        assert_eq!(correct_text("1 Tessalonian"), "1 Thessalonians");
    }
}
