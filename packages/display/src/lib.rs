mod controller;
mod state;

pub use controller::DisplayController;
pub use state::{DisplayedState, SubPoint};

#[cfg(test)]
mod tests {
    use super::*;
    use companion_events::BibleReference;

    // ── helpers ───────────────────────────────────────────────────────────────

    fn john_3_16() -> BibleReference {
        BibleReference::new("John", 3).with_verse(16)
    }

    fn romans_8_28() -> BibleReference {
        BibleReference::new("Romans", 8).with_verse(28)
    }

    // ── DisplayedState ────────────────────────────────────────────────────────

    #[test]
    fn displayed_state_default_is_blank() {
        assert_eq!(DisplayedState::default(), DisplayedState::Blank);
    }

    #[test]
    fn displayed_state_display_blank() {
        assert_eq!(DisplayedState::Blank.to_string(), "blank");
    }

    #[test]
    fn displayed_state_display_sermon_title() {
        let s = DisplayedState::SermonTitle("Grace".into());
        assert_eq!(s.to_string(), "title: Grace");
    }

    #[test]
    fn displayed_state_display_sub_point() {
        let s = DisplayedState::SubPoint(SubPoint::new("God is love"));
        assert_eq!(s.to_string(), "sub-point: God is love");
    }

    #[test]
    fn displayed_state_display_verse() {
        let s = DisplayedState::Verse(john_3_16(), "For God so loved…".into());
        assert_eq!(s.to_string(), "verse: John 3:16");
    }

    // ── SubPoint ──────────────────────────────────────────────────────────────

    #[test]
    fn sub_point_new_stores_text() {
        let sp = SubPoint::new("The Lord is my shepherd");
        assert_eq!(sp.text, "The Lord is my shepherd");
    }

    #[test]
    fn sub_point_clone_is_equal() {
        let sp = SubPoint::new("point");
        assert_eq!(sp.clone(), sp);
    }

    // ── DisplayController::new ────────────────────────────────────────────────

    #[test]
    fn new_starts_blank() {
        let ctrl = DisplayController::new();
        assert_eq!(*ctrl.state(), DisplayedState::Blank);
    }

    #[test]
    fn default_starts_blank() {
        let ctrl = DisplayController::default();
        assert_eq!(*ctrl.state(), DisplayedState::Blank);
    }

    #[test]
    fn is_blank_true_on_new() {
        assert!(DisplayController::new().is_blank());
    }

    // ── show_blank ────────────────────────────────────────────────────────────

    #[test]
    fn show_blank_from_title() {
        let mut ctrl = DisplayController::new();
        ctrl.show_sermon_title("Title");
        ctrl.show_blank();
        assert_eq!(*ctrl.state(), DisplayedState::Blank);
        assert!(ctrl.is_blank());
    }

    #[test]
    fn show_blank_from_verse() {
        let mut ctrl = DisplayController::new();
        ctrl.show_verse(john_3_16(), "text");
        ctrl.show_blank();
        assert_eq!(*ctrl.state(), DisplayedState::Blank);
    }

    #[test]
    fn show_blank_is_idempotent() {
        let mut ctrl = DisplayController::new();
        ctrl.show_blank();
        ctrl.show_blank();
        assert_eq!(*ctrl.state(), DisplayedState::Blank);
    }

    // ── show_sermon_title ─────────────────────────────────────────────────────

    #[test]
    fn show_sermon_title_sets_state() {
        let mut ctrl = DisplayController::new();
        ctrl.show_sermon_title("Grace and Truth");
        assert_eq!(
            *ctrl.state(),
            DisplayedState::SermonTitle("Grace and Truth".into())
        );
        assert!(!ctrl.is_blank());
    }

    #[test]
    fn show_sermon_title_overwrites_previous_title() {
        let mut ctrl = DisplayController::new();
        ctrl.show_sermon_title("First");
        ctrl.show_sermon_title("Second");
        assert_eq!(
            *ctrl.state(),
            DisplayedState::SermonTitle("Second".into())
        );
    }

    #[test]
    fn show_sermon_title_from_sub_point() {
        let mut ctrl = DisplayController::new();
        ctrl.show_sub_point(SubPoint::new("point"));
        ctrl.show_sermon_title("New Title");
        assert!(matches!(ctrl.state(), DisplayedState::SermonTitle(_)));
    }

    // ── show_sub_point ────────────────────────────────────────────────────────

    #[test]
    fn show_sub_point_sets_state() {
        let mut ctrl = DisplayController::new();
        ctrl.show_sub_point(SubPoint::new("God loves us unconditionally"));
        assert_eq!(
            *ctrl.state(),
            DisplayedState::SubPoint(SubPoint::new("God loves us unconditionally"))
        );
    }

    #[test]
    fn show_sub_point_text_is_preserved() {
        let mut ctrl = DisplayController::new();
        ctrl.show_sub_point(SubPoint::new("The Lord is my shepherd"));
        let DisplayedState::SubPoint(sp) = ctrl.state() else {
            panic!("expected SubPoint");
        };
        assert_eq!(sp.text, "The Lord is my shepherd");
    }

    #[test]
    fn show_sub_point_after_verse() {
        let mut ctrl = DisplayController::new();
        ctrl.show_verse(john_3_16(), "text");
        ctrl.show_sub_point(SubPoint::new("Application"));
        assert!(matches!(ctrl.state(), DisplayedState::SubPoint(_)));
    }

    // ── show_verse ────────────────────────────────────────────────────────────

    #[test]
    fn show_verse_sets_state() {
        let mut ctrl = DisplayController::new();
        let r = john_3_16();
        ctrl.show_verse(r.clone(), "For God so loved the world…");
        assert_eq!(
            *ctrl.state(),
            DisplayedState::Verse(r, "For God so loved the world…".into())
        );
    }

    #[test]
    fn show_verse_reference_and_text_preserved() {
        let mut ctrl = DisplayController::new();
        ctrl.show_verse(john_3_16(), "For God so loved the world");
        let DisplayedState::Verse(r, t) = ctrl.state() else {
            panic!("expected Verse");
        };
        assert_eq!(*r, john_3_16());
        assert_eq!(t, "For God so loved the world");
    }

    #[test]
    fn show_verse_overwrites_sermon_title() {
        let mut ctrl = DisplayController::new();
        ctrl.show_sermon_title("Love");
        ctrl.show_verse(john_3_16(), "text");
        assert!(matches!(ctrl.state(), DisplayedState::Verse(_, _)));
    }

    #[test]
    fn show_verse_overwrites_previous_verse() {
        let mut ctrl = DisplayController::new();
        ctrl.show_verse(john_3_16(), "first text");
        ctrl.show_verse(romans_8_28(), "second text");
        let DisplayedState::Verse(r, _) = ctrl.state() else {
            panic!("expected Verse");
        };
        assert_eq!(*r, romans_8_28());
    }

    // ── full transition cycle ─────────────────────────────────────────────────

    #[test]
    fn complete_service_flow() {
        let mut ctrl = DisplayController::new();

        // Service opens blank
        assert_eq!(*ctrl.state(), DisplayedState::Blank);

        // Preacher announces sermon title
        ctrl.show_sermon_title("Walking by Faith");
        assert!(matches!(ctrl.state(), DisplayedState::SermonTitle(_)));

        // First point
        ctrl.show_sub_point(SubPoint::new("1. Faith requires trust"));
        assert!(matches!(ctrl.state(), DisplayedState::SubPoint(_)));

        // Scripture reference
        ctrl.show_verse(john_3_16(), "For God so loved the world…");
        assert!(matches!(ctrl.state(), DisplayedState::Verse(_, _)));

        // Second point
        ctrl.show_sub_point(SubPoint::new("2. Faith produces action"));
        assert!(matches!(ctrl.state(), DisplayedState::SubPoint(_)));

        // Blackout between sections
        ctrl.show_blank();
        assert_eq!(*ctrl.state(), DisplayedState::Blank);
        assert!(ctrl.is_blank());
    }
}
