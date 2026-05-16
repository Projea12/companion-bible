//! Builds prompts for the Claude cloud AI layer.

// ─── System prompt ────────────────────────────────────────────────────────────

const SYSTEM_PROMPT: &str = "\
You are a scripture reference assistant for a live church sermon transcription system \
running in Nigeria. Your ONLY task is to identify the most likely Bible reference \
being discussed and return it as a single JSON object.\n\
\n\
Rules:\n\
1. Respond with ONLY valid JSON — no prose, no explanation, no markdown fences.\n\
2. NEVER invent, guess, or hallucinate verse content or references you are not confident about.\n\
3. If the speaker quotes a verse WITHOUT naming it (unattributed quotation), try to identify \
   the source from the wording alone — set \"unattributed\": true if you do.\n\
4. If you cannot determine the reference with confidence, set \"book\" to null.\n\
5. Do not repeat or paraphrase the transcript in your response.\n\
6. Output schema:\n\
   {\n\
     \"book\": \"<string|null>\",\n\
     \"chapter\": <int|null>,\n\
     \"verse\": <int|null>,\n\
     \"confidence\": <0.0-1.0>,\n\
     \"unattributed\": <bool>\n\
   }";

// ─── CloudPromptBuilder ───────────────────────────────────────────────────────

/// Constructs the user-turn message for the Claude cloud API call.
pub struct CloudPromptBuilder {
    active_book: Option<String>,
    active_chapter: Option<u8>,
    /// Full 60-second rolling transcript window.
    recent_transcript: Option<String>,
    anchor_scripture: Option<String>,
}

impl CloudPromptBuilder {
    pub fn new() -> Self {
        Self {
            active_book: None,
            active_chapter: None,
            recent_transcript: None,
            anchor_scripture: None,
        }
    }

    pub fn with_context(mut self, book: Option<&str>, chapter: Option<u8>) -> Self {
        self.active_book = book.map(str::to_owned);
        self.active_chapter = chapter;
        self
    }

    /// Full 60-second rolling transcript for context.
    pub fn with_transcript(mut self, transcript: &str) -> Self {
        if !transcript.is_empty() {
            self.recent_transcript = Some(transcript.to_owned());
        }
        self
    }

    pub fn with_anchor(mut self, anchor: &str) -> Self {
        if !anchor.is_empty() {
            self.anchor_scripture = Some(anchor.to_owned());
        }
        self
    }

    /// Returns `(system_prompt, user_content)` ready for the Messages API.
    pub fn build(&self, segment_text: &str) -> (String, String) {
        let mut user = String::new();

        if let Some(anchor) = &self.anchor_scripture {
            user.push_str(&format!("Sermon anchor scripture: {anchor}\n"));
        }

        if let Some(book) = &self.active_book {
            user.push_str(&format!("Current book being preached: {book}"));
            if let Some(ch) = self.active_chapter {
                user.push_str(&format!(", chapter {ch}"));
            }
            user.push('\n');
        }

        if let Some(transcript) = &self.recent_transcript {
            user.push_str("Recent 60-second transcript:\n");
            user.push_str(transcript);
            user.push('\n');
        }

        user.push_str("Current segment: ");
        user.push_str(segment_text);

        (SYSTEM_PROMPT.to_owned(), user)
    }

    pub fn system_prompt() -> &'static str {
        SYSTEM_PROMPT
    }
}

impl Default for CloudPromptBuilder {
    fn default() -> Self {
        Self::new()
    }
}

// ─── Tests ────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn system_prompt_requests_json_only() {
        assert!(SYSTEM_PROMPT.contains("ONLY valid JSON"));
        assert!(SYSTEM_PROMPT.contains("no prose"));
    }

    #[test]
    fn system_prompt_has_unattributed_quotation_instruction() {
        assert!(SYSTEM_PROMPT.contains("unattributed quotation"));
        assert!(SYSTEM_PROMPT.contains("\"unattributed\""));
    }

    #[test]
    fn system_prompt_has_hallucination_prevention() {
        assert!(SYSTEM_PROMPT.contains("NEVER invent"));
        assert!(SYSTEM_PROMPT.contains("hallucinate"));
    }

    #[test]
    fn build_includes_segment_text() {
        let (_, user) = CloudPromptBuilder::new().build("John 3:16");
        assert!(user.contains("John 3:16"));
    }

    #[test]
    fn build_includes_full_60s_transcript() {
        let transcript = "As we read in John chapter 3 this morning";
        let (_, user) = CloudPromptBuilder::new()
            .with_transcript(transcript)
            .build("verse 16");
        assert!(user.contains("60-second transcript"));
        assert!(user.contains(transcript));
    }

    #[test]
    fn build_includes_active_book_and_chapter() {
        let (_, user) = CloudPromptBuilder::new()
            .with_context(Some("Romans"), Some(8))
            .build("verse 28");
        assert!(user.contains("Romans"));
        assert!(user.contains("chapter 8"));
    }

    #[test]
    fn build_includes_anchor_scripture() {
        let (_, user) = CloudPromptBuilder::new()
            .with_anchor("John 3:16")
            .build("verse 17");
        assert!(user.contains("anchor scripture"));
        assert!(user.contains("John 3:16"));
    }

    #[test]
    fn empty_transcript_not_injected() {
        let (_, user) = CloudPromptBuilder::new().with_transcript("").build("test");
        assert!(!user.contains("transcript"));
    }

    #[test]
    fn output_schema_has_unattributed_field() {
        assert!(SYSTEM_PROMPT.contains("\"unattributed\""));
    }

    #[test]
    fn no_context_omits_book_line() {
        let (_, user) = CloudPromptBuilder::new().build("Romans 8:28");
        assert!(!user.contains("Current book"));
    }
}
