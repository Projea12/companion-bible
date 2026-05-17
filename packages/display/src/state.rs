use companion_events::BibleReference;

/// A single sermon outline point shown on the congregation display.
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct SubPoint {
    pub text: String,
}

impl SubPoint {
    pub fn new(text: impl Into<String>) -> Self {
        Self { text: text.into() }
    }
}

/// Every distinct state the congregation display can be in.
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum DisplayedState {
    /// Screen is completely black — used during transitions or blackout moments.
    Blank,
    /// A sermon title is shown full-screen.
    SermonTitle(String),
    /// An outline sub-point is shown.
    SubPoint(SubPoint),
    /// A scripture verse is shown: (reference, verse text).
    Verse(BibleReference, String),
}

impl Default for DisplayedState {
    fn default() -> Self {
        Self::Blank
    }
}

impl std::fmt::Display for DisplayedState {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Blank => write!(f, "blank"),
            Self::SermonTitle(t) => write!(f, "title: {t}"),
            Self::SubPoint(sp) => write!(f, "sub-point: {}", sp.text),
            Self::Verse(r, _) => write!(f, "verse: {r}"),
        }
    }
}
