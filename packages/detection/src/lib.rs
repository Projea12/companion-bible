pub mod book_data;
pub mod engine;
pub mod hymn_detector;
pub mod normalizer;

pub use book_data::{build_book_alternation, canonical_name};
pub use engine::{MatchCompleteness, PatternEngine, PatternResult};
pub use hymn_detector::detect_hymn_number;
pub use normalizer::NumberNormalizer;
