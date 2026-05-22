//! Builds inference prompts for the local Phi-3 model.

// ─── Phi-3 chat tokens ────────────────────────────────────────────────────────

const SYS_START: &str = "<|system|>\n";
const SYS_END: &str = "\n<|end|>\n";
const USER_START: &str = "<|user|>\n";
const USER_END: &str = "\n<|end|>\n";
const ASSISTANT_START: &str = "<|assistant|>\n";

// ─── System prompt ────────────────────────────────────────────────────────────

const SYSTEM_PROMPT: &str = "\
You are a Bible reference assistant for a live sermon transcription system. \
Your ONLY task is to identify the most likely scripture reference being discussed \
and return it as a single JSON object.\n\
\n\
Rules:\n\
1. Respond with ONLY valid JSON — no prose, no explanation, no markdown.\n\
2. If you cannot determine the reference with confidence, set \"book\" to null.\n\
3. NEVER invent or hallucinate verse content. Only identify the reference.\n\
4. Do not repeat or paraphrase the transcript.\n\
5. Output schema: {\"book\":\"<string|null>\",\"chapter\":<int|null>,\"verse\":<int|null>,\"confidence\":<0.0-1.0>}";

// ─── SermonPromptBuilder ──────────────────────────────────────────────────────

/// Constructs a Phi-3-formatted prompt for scripture reference classification.
pub struct SermonPromptBuilder {
    active_book: Option<String>,
    active_chapter: Option<u8>,
    recent_transcript: Option<String>,
}

impl SermonPromptBuilder {
    pub fn new() -> Self {
        Self {
            active_book: None,
            active_chapter: None,
            recent_transcript: None,
        }
    }

    /// Prime the builder with the current sermon context.
    pub fn with_context(mut self, book: Option<&str>, chapter: Option<u8>) -> Self {
        self.active_book = book.map(str::to_owned);
        self.active_chapter = chapter;
        self
    }

    /// Provide the rolling transcript window.
    pub fn with_transcript(mut self, transcript: &str) -> Self {
        if !transcript.is_empty() {
            self.recent_transcript = Some(transcript.to_owned());
        }
        self
    }

    /// Build the full Phi-3 prompt for `segment_text`.
    pub fn build(&self, segment_text: &str) -> String {
        let mut user = String::new();

        if let Some(book) = &self.active_book {
            user.push_str(&format!("Current sermon book: {book}"));
            if let Some(ch) = self.active_chapter {
                user.push_str(&format!(", chapter {ch}"));
            }
            user.push('\n');
        }

        if let Some(transcript) = &self.recent_transcript {
            user.push_str("Recent transcript:\n");
            user.push_str(transcript);
            user.push('\n');
        }

        user.push_str("Segment: ");
        user.push_str(segment_text);

        format!("{SYS_START}{SYSTEM_PROMPT}{SYS_END}{USER_START}{user}{USER_END}{ASSISTANT_START}")
    }
}

impl Default for SermonPromptBuilder {
    fn default() -> Self {
        Self::new()
    }
}

// ─── Tests ────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn prompt_contains_phi3_chat_tokens() {
        let prompt = SermonPromptBuilder::new().build("verse 16");
        assert!(prompt.contains("<|system|>"));
        assert!(prompt.contains("<|user|>"));
        assert!(prompt.contains("<|assistant|>"));
        assert!(prompt.contains("<|end|>"));
    }

    #[test]
    fn prompt_contains_hallucination_prevention_rules() {
        let prompt = SermonPromptBuilder::new().build("John 3");
        assert!(prompt.contains("NEVER invent or hallucinate"));
        assert!(prompt.contains("ONLY valid JSON"));
    }

    #[test]
    fn prompt_injects_active_book_and_chapter() {
        let prompt = SermonPromptBuilder::new()
            .with_context(Some("Romans"), Some(8))
            .build("verse 1");
        assert!(prompt.contains("Romans"));
        assert!(prompt.contains("chapter 8"));
    }

    #[test]
    fn prompt_injects_rolling_transcript() {
        let prompt = SermonPromptBuilder::new()
            .with_transcript("As we read in Romans chapter 8")
            .build("verse 1");
        assert!(prompt.contains("Recent transcript:"));
        assert!(prompt.contains("Romans chapter 8"));
    }

    #[test]
    fn prompt_without_context_omits_context_line() {
        let prompt = SermonPromptBuilder::new().build("John 3:16");
        assert!(!prompt.contains("Current sermon book:"));
    }

    #[test]
    fn prompt_ends_with_assistant_start_token() {
        let prompt = SermonPromptBuilder::new().build("test");
        assert!(prompt.ends_with("<|assistant|>\n"));
    }

    #[test]
    fn prompt_contains_json_schema_description() {
        let prompt = SermonPromptBuilder::new().build("Romans 8:28");
        assert!(prompt.contains("\"book\""));
        assert!(prompt.contains("\"chapter\""));
        assert!(prompt.contains("\"verse\""));
        assert!(prompt.contains("\"confidence\""));
    }

    #[test]
    fn empty_transcript_not_injected() {
        let prompt = SermonPromptBuilder::new()
            .with_transcript("")
            .build("Hebrews 11:1");
        assert!(!prompt.contains("Recent transcript:"));
    }

    #[test]
    fn segment_text_always_present() {
        let prompt = SermonPromptBuilder::new().build("Hebrews 11:1");
        assert!(prompt.contains("Hebrews 11:1"));
    }
}
