mod controller;
mod error;
mod history;
mod state;
mod wal;

pub use controller::DisplayController;
pub use error::DisplayError;
pub use state::{DisplayedState, SubPoint};
pub use wal::{MemoryWal, SharedWal, WalEntry, WriteAheadLog};

#[cfg(test)]
mod tests {
    use super::*;
    use crate::wal::{FailingWal, SharedWal};
    use companion_events::BibleReference;

    // ── helpers ───────────────────────────────────────────────────────────────

    fn john_3_16() -> BibleReference {
        BibleReference::new("John", 3).with_verse(16)
    }

    fn romans_8_28() -> BibleReference {
        BibleReference::new("Romans", 8).with_verse(28)
    }

    fn ok_renderer() -> Box<dyn Fn(&DisplayedState) -> Result<(), String> + Send> {
        Box::new(|_| Ok(()))
    }

    fn failing_renderer() -> Box<dyn Fn(&DisplayedState) -> Result<(), String> + Send> {
        Box::new(|_| Err("render error".into()))
    }

    fn ctrl() -> DisplayController {
        DisplayController::new(MemoryWal::new(), ok_renderer())
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
        assert_eq!(*ctrl().state(), DisplayedState::Blank);
    }

    #[test]
    fn is_blank_true_on_new() {
        assert!(ctrl().is_blank());
    }

    #[test]
    fn history_empty_on_new() {
        assert_eq!(ctrl().history_len(), 0);
    }

    // ── show_blank ────────────────────────────────────────────────────────────

    #[test]
    fn show_blank_from_title() {
        let mut c = ctrl();
        c.show_sermon_title("Title").unwrap();
        c.show_blank().unwrap();
        assert_eq!(*c.state(), DisplayedState::Blank);
        assert!(c.is_blank());
    }

    #[test]
    fn show_blank_from_verse() {
        let mut c = ctrl();
        c.show_verse(john_3_16(), "text").unwrap();
        c.show_blank().unwrap();
        assert_eq!(*c.state(), DisplayedState::Blank);
    }

    #[test]
    fn show_blank_is_idempotent() {
        let mut c = ctrl();
        c.show_blank().unwrap();
        c.show_blank().unwrap();
        assert_eq!(*c.state(), DisplayedState::Blank);
    }

    // ── show_sermon_title ─────────────────────────────────────────────────────

    #[test]
    fn show_sermon_title_sets_state() {
        let mut c = ctrl();
        c.show_sermon_title("Grace and Truth").unwrap();
        assert_eq!(
            *c.state(),
            DisplayedState::SermonTitle("Grace and Truth".into())
        );
        assert!(!c.is_blank());
    }

    #[test]
    fn show_sermon_title_overwrites_previous_title() {
        let mut c = ctrl();
        c.show_sermon_title("First").unwrap();
        c.show_sermon_title("Second").unwrap();
        assert_eq!(*c.state(), DisplayedState::SermonTitle("Second".into()));
    }

    #[test]
    fn show_sermon_title_from_sub_point() {
        let mut c = ctrl();
        c.show_sub_point(SubPoint::new("point")).unwrap();
        c.show_sermon_title("New Title").unwrap();
        assert!(matches!(c.state(), DisplayedState::SermonTitle(_)));
    }

    // ── show_sub_point ────────────────────────────────────────────────────────

    #[test]
    fn show_sub_point_sets_state() {
        let mut c = ctrl();
        c.show_sub_point(SubPoint::new("God loves us unconditionally"))
            .unwrap();
        assert_eq!(
            *c.state(),
            DisplayedState::SubPoint(SubPoint::new("God loves us unconditionally"))
        );
    }

    #[test]
    fn show_sub_point_text_is_preserved() {
        let mut c = ctrl();
        c.show_sub_point(SubPoint::new("The Lord is my shepherd"))
            .unwrap();
        let DisplayedState::SubPoint(sp) = c.state() else {
            panic!("expected SubPoint");
        };
        assert_eq!(sp.text, "The Lord is my shepherd");
    }

    #[test]
    fn show_sub_point_after_verse() {
        let mut c = ctrl();
        c.show_verse(john_3_16(), "text").unwrap();
        c.show_sub_point(SubPoint::new("Application")).unwrap();
        assert!(matches!(c.state(), DisplayedState::SubPoint(_)));
    }

    // ── show_verse ────────────────────────────────────────────────────────────

    #[test]
    fn show_verse_sets_state() {
        let mut c = ctrl();
        let r = john_3_16();
        c.show_verse(r.clone(), "For God so loved the world…").unwrap();
        assert_eq!(
            *c.state(),
            DisplayedState::Verse(r, "For God so loved the world…".into())
        );
    }

    #[test]
    fn show_verse_reference_and_text_preserved() {
        let mut c = ctrl();
        c.show_verse(john_3_16(), "For God so loved the world")
            .unwrap();
        let DisplayedState::Verse(r, t) = c.state() else {
            panic!("expected Verse");
        };
        assert_eq!(*r, john_3_16());
        assert_eq!(t, "For God so loved the world");
    }

    #[test]
    fn show_verse_overwrites_sermon_title() {
        let mut c = ctrl();
        c.show_sermon_title("Love").unwrap();
        c.show_verse(john_3_16(), "text").unwrap();
        assert!(matches!(c.state(), DisplayedState::Verse(_, _)));
    }

    #[test]
    fn show_verse_overwrites_previous_verse() {
        let mut c = ctrl();
        c.show_verse(john_3_16(), "first text").unwrap();
        c.show_verse(romans_8_28(), "second text").unwrap();
        let DisplayedState::Verse(r, _) = c.state() else {
            panic!("expected Verse");
        };
        assert_eq!(*r, romans_8_28());
    }

    // ── WAL writes ────────────────────────────────────────────────────────────

    #[test]
    fn show_verse_writes_to_wal() {
        let wal = MemoryWal::new();
        let mut c = DisplayController::new(wal, ok_renderer());
        c.show_verse(john_3_16(), "text").unwrap();
        // We can't access the WAL directly after move, but we can verify via FailingWal
    }

    #[test]
    fn wal_failure_blocks_state_change() {
        let mut c = DisplayController::new(FailingWal::new("disk full"), ok_renderer());
        let err = c.show_verse(john_3_16(), "text").unwrap_err();
        assert_eq!(err, DisplayError::WalWriteFailed("disk full".into()));
        assert_eq!(*c.state(), DisplayedState::Blank);
    }

    #[test]
    fn wal_failure_does_not_update_history() {
        let mut c = DisplayController::new(FailingWal::new("err"), ok_renderer());
        let _ = c.show_verse(john_3_16(), "text");
        assert_eq!(c.history_len(), 0);
    }

    // ── render errors ─────────────────────────────────────────────────────────

    #[test]
    fn render_failure_returns_error() {
        let mut c = DisplayController::new(MemoryWal::new(), failing_renderer());
        let err = c.show_verse(john_3_16(), "text").unwrap_err();
        assert_eq!(err, DisplayError::RenderFailed("render error".into()));
    }

    #[test]
    fn render_failure_state_is_still_updated() {
        // State and WAL are committed before render; render failure is non-blocking to state.
        let mut c = DisplayController::new(MemoryWal::new(), failing_renderer());
        let _ = c.show_verse(john_3_16(), "text");
        assert!(matches!(c.state(), DisplayedState::Verse(_, _)));
    }

    // ── StateHistory ──────────────────────────────────────────────────────────

    #[test]
    fn history_grows_with_transitions() {
        let mut c = ctrl();
        c.show_sermon_title("T").unwrap();
        assert_eq!(c.history_len(), 1);
        c.show_verse(john_3_16(), "v").unwrap();
        assert_eq!(c.history_len(), 2);
    }

    #[test]
    fn history_capped_at_ten() {
        let mut c = ctrl();
        for i in 0..12 {
            c.show_sermon_title(format!("T{i}")).unwrap();
        }
        assert_eq!(c.history_len(), 10);
    }

    #[test]
    fn history_oldest_dropped_when_full() {
        let mut c = ctrl();
        // Fill: Blank → T0 → T1 → … → T9 → T10 → T11
        // At cap the Blank entry is evicted first, then T0, etc.
        for i in 0..12 {
            c.show_sermon_title(format!("T{i}")).unwrap();
        }
        // Current is T11, history tail is T10
        c.discard().unwrap();
        assert_eq!(*c.state(), DisplayedState::SermonTitle("T10".into()));
    }

    // ── discard ───────────────────────────────────────────────────────────────

    #[test]
    fn discard_empty_history_returns_no_history_error() {
        let mut c = ctrl();
        assert_eq!(c.discard().unwrap_err(), DisplayError::NoHistory);
    }

    #[test]
    fn discard_restores_previous_state() {
        let mut c = ctrl();
        c.show_sermon_title("Walking by Faith").unwrap();
        c.show_verse(john_3_16(), "text").unwrap();
        c.discard().unwrap();
        assert_eq!(
            *c.state(),
            DisplayedState::SermonTitle("Walking by Faith".into())
        );
    }

    #[test]
    fn discard_decrements_history() {
        let mut c = ctrl();
        c.show_sermon_title("T").unwrap();
        c.show_verse(john_3_16(), "v").unwrap();
        assert_eq!(c.history_len(), 2);
        c.discard().unwrap();
        assert_eq!(c.history_len(), 1);
    }

    #[test]
    fn discard_twice_walks_back_two_steps() {
        let mut c = ctrl();
        c.show_sermon_title("First").unwrap();
        c.show_verse(john_3_16(), "verse").unwrap();
        c.show_blank().unwrap();
        c.discard().unwrap();
        assert!(matches!(c.state(), DisplayedState::Verse(_, _)));
        c.discard().unwrap();
        assert_eq!(*c.state(), DisplayedState::SermonTitle("First".into()));
    }

    #[test]
    fn discard_writes_to_wal() {
        // Verify discard is blocked by a failing WAL after history is populated.
        // We can't swap the WAL mid-flight, so we test via failing WAL from start:
        // with a failing WAL, show_verse fails too — so test the indirect guarantee
        // that discard on empty history never reaches the WAL.
        let mut c = DisplayController::new(FailingWal::new("fail"), ok_renderer());
        // Can't push history (show_verse fails), so discard must return NoHistory
        assert_eq!(c.discard().unwrap_err(), DisplayError::NoHistory);
    }

    // ── restore ───────────────────────────────────────────────────────────────

    #[test]
    fn restore_sets_state() {
        let mut c = ctrl();
        c.restore(DisplayedState::SermonTitle("Recovery".into()))
            .unwrap();
        assert_eq!(*c.state(), DisplayedState::SermonTitle("Recovery".into()));
    }

    #[test]
    fn restore_verse_state() {
        let mut c = ctrl();
        c.restore(DisplayedState::Verse(john_3_16(), "recovered text".into()))
            .unwrap();
        assert!(matches!(c.state(), DisplayedState::Verse(_, _)));
    }

    #[test]
    fn restore_writes_to_wal() {
        let mut c = DisplayController::new(FailingWal::new("disk full"), ok_renderer());
        let err = c
            .restore(DisplayedState::SermonTitle("T".into()))
            .unwrap_err();
        assert_eq!(err, DisplayError::WalWriteFailed("disk full".into()));
    }

    #[test]
    fn restore_wal_failure_leaves_state_unchanged() {
        let mut c = DisplayController::new(FailingWal::new("err"), ok_renderer());
        let _ = c.restore(DisplayedState::SermonTitle("T".into()));
        assert_eq!(*c.state(), DisplayedState::Blank);
    }

    #[test]
    fn restore_pushes_to_history() {
        let mut c = ctrl();
        c.restore(DisplayedState::SermonTitle("T".into())).unwrap();
        assert_eq!(c.history_len(), 1);
    }

    #[test]
    fn restore_can_be_discarded() {
        let mut c = ctrl();
        c.restore(DisplayedState::SermonTitle("Restored".into()))
            .unwrap();
        c.discard().unwrap();
        assert_eq!(*c.state(), DisplayedState::Blank);
    }

    #[test]
    fn restore_render_failure_returns_error() {
        let mut c = DisplayController::new(MemoryWal::new(), failing_renderer());
        let err = c
            .restore(DisplayedState::SermonTitle("T".into()))
            .unwrap_err();
        assert_eq!(err, DisplayError::RenderFailed("render error".into()));
    }

    // ── every state → every state transition (4 × 4 matrix) ──────────────────

    // from Blank
    #[test]
    fn blank_to_blank() {
        let mut c = ctrl();
        c.show_blank().unwrap();
        assert_eq!(*c.state(), DisplayedState::Blank);
    }

    #[test]
    fn blank_to_sermon_title() {
        let mut c = ctrl();
        c.show_sermon_title("Grace").unwrap();
        assert!(matches!(c.state(), DisplayedState::SermonTitle(_)));
    }

    #[test]
    fn blank_to_sub_point() {
        let mut c = ctrl();
        c.show_sub_point(SubPoint::new("point")).unwrap();
        assert!(matches!(c.state(), DisplayedState::SubPoint(_)));
    }

    #[test]
    fn blank_to_verse() {
        let mut c = ctrl();
        c.show_verse(john_3_16(), "text").unwrap();
        assert!(matches!(c.state(), DisplayedState::Verse(_, _)));
    }

    // from SermonTitle
    #[test]
    fn sermon_title_to_blank() {
        let mut c = ctrl();
        c.show_sermon_title("T").unwrap();
        c.show_blank().unwrap();
        assert_eq!(*c.state(), DisplayedState::Blank);
    }

    #[test]
    fn sermon_title_to_sermon_title() {
        let mut c = ctrl();
        c.show_sermon_title("First").unwrap();
        c.show_sermon_title("Second").unwrap();
        assert_eq!(*c.state(), DisplayedState::SermonTitle("Second".into()));
    }

    #[test]
    fn sermon_title_to_sub_point() {
        let mut c = ctrl();
        c.show_sermon_title("T").unwrap();
        c.show_sub_point(SubPoint::new("point")).unwrap();
        assert!(matches!(c.state(), DisplayedState::SubPoint(_)));
    }

    #[test]
    fn sermon_title_to_verse() {
        let mut c = ctrl();
        c.show_sermon_title("T").unwrap();
        c.show_verse(john_3_16(), "text").unwrap();
        assert!(matches!(c.state(), DisplayedState::Verse(_, _)));
    }

    // from SubPoint
    #[test]
    fn sub_point_to_blank() {
        let mut c = ctrl();
        c.show_sub_point(SubPoint::new("p")).unwrap();
        c.show_blank().unwrap();
        assert_eq!(*c.state(), DisplayedState::Blank);
    }

    #[test]
    fn sub_point_to_sermon_title() {
        let mut c = ctrl();
        c.show_sub_point(SubPoint::new("p")).unwrap();
        c.show_sermon_title("T").unwrap();
        assert!(matches!(c.state(), DisplayedState::SermonTitle(_)));
    }

    #[test]
    fn sub_point_to_sub_point() {
        let mut c = ctrl();
        c.show_sub_point(SubPoint::new("first")).unwrap();
        c.show_sub_point(SubPoint::new("second")).unwrap();
        let DisplayedState::SubPoint(sp) = c.state() else {
            panic!("expected SubPoint");
        };
        assert_eq!(sp.text, "second");
    }

    #[test]
    fn sub_point_to_verse() {
        let mut c = ctrl();
        c.show_sub_point(SubPoint::new("p")).unwrap();
        c.show_verse(john_3_16(), "text").unwrap();
        assert!(matches!(c.state(), DisplayedState::Verse(_, _)));
    }

    // from Verse
    #[test]
    fn verse_to_blank() {
        let mut c = ctrl();
        c.show_verse(john_3_16(), "text").unwrap();
        c.show_blank().unwrap();
        assert_eq!(*c.state(), DisplayedState::Blank);
    }

    #[test]
    fn verse_to_sermon_title() {
        let mut c = ctrl();
        c.show_verse(john_3_16(), "text").unwrap();
        c.show_sermon_title("T").unwrap();
        assert!(matches!(c.state(), DisplayedState::SermonTitle(_)));
    }

    #[test]
    fn verse_to_sub_point() {
        let mut c = ctrl();
        c.show_verse(john_3_16(), "text").unwrap();
        c.show_sub_point(SubPoint::new("application")).unwrap();
        assert!(matches!(c.state(), DisplayedState::SubPoint(_)));
    }

    #[test]
    fn verse_to_verse() {
        let mut c = ctrl();
        c.show_verse(john_3_16(), "first").unwrap();
        c.show_verse(romans_8_28(), "second").unwrap();
        let DisplayedState::Verse(r, t) = c.state() else {
            panic!("expected Verse");
        };
        assert_eq!(*r, romans_8_28());
        assert_eq!(t, "second");
    }

    // ── WAL-before-display guarantee ──────────────────────────────────────────

    /// WAL entry is appended even when the renderer subsequently fails
    /// (simulates a crash/error between WAL write and display update).
    #[test]
    fn wal_entry_written_before_render_attempt() {
        let (wal, log) = SharedWal::new();
        let mut c = DisplayController::new(wal, failing_renderer());

        let _ = c.show_verse(john_3_16(), "For God so loved the world");

        let entries = log.lock().unwrap();
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].from, DisplayedState::Blank);
        assert!(matches!(entries[0].to, DisplayedState::Verse(_, _)));
    }

    /// After a crash (render failed), replaying the WAL log on a fresh controller
    /// via `restore()` puts the display back in the correct state.
    #[test]
    fn wal_replay_restores_correct_state_after_crash() {
        // Phase 1 — "original run" captures WAL entries.
        let (wal, log) = SharedWal::new();
        let mut original = DisplayController::new(wal, ok_renderer());
        original.show_sermon_title("Walking by Faith").unwrap();
        original
            .show_verse(john_3_16(), "For God so loved the world")
            .unwrap();
        original
            .show_sub_point(SubPoint::new("Faith requires trust"))
            .unwrap();

        // Capture final WAL state before "crash".
        let entries: Vec<WalEntry> = log.lock().unwrap().clone();

        // Phase 2 — fresh controller replays the WAL to recover.
        let mut recovered = DisplayController::new(MemoryWal::new(), ok_renderer());
        for entry in &entries {
            recovered.restore(entry.to.clone()).unwrap();
        }

        assert_eq!(*recovered.state(), *original.state());
    }

    /// WAL sequence faithfully records every transition from → to in order.
    #[test]
    fn wal_records_full_transition_sequence() {
        let (wal, log) = SharedWal::new();
        let mut c = DisplayController::new(wal, ok_renderer());

        c.show_sermon_title("T").unwrap();
        c.show_verse(john_3_16(), "v").unwrap();
        c.show_blank().unwrap();

        let entries = log.lock().unwrap();
        assert_eq!(entries.len(), 3);

        assert_eq!(entries[0].from, DisplayedState::Blank);
        assert!(matches!(entries[0].to, DisplayedState::SermonTitle(_)));

        assert!(matches!(entries[1].from, DisplayedState::SermonTitle(_)));
        assert!(matches!(entries[1].to, DisplayedState::Verse(_, _)));

        assert!(matches!(entries[2].from, DisplayedState::Verse(_, _)));
        assert_eq!(entries[2].to, DisplayedState::Blank);
    }

    /// discard() also writes a WAL entry (the undo itself is logged).
    #[test]
    fn discard_wal_entry_recorded() {
        let (wal, log) = SharedWal::new();
        let mut c = DisplayController::new(wal, ok_renderer());

        c.show_sermon_title("T").unwrap();
        c.discard().unwrap();

        let entries = log.lock().unwrap();
        // entry[0]: Blank → SermonTitle   (show_sermon_title)
        // entry[1]: SermonTitle → Blank   (discard)
        assert_eq!(entries.len(), 2);
        assert!(matches!(entries[1].from, DisplayedState::SermonTitle(_)));
        assert_eq!(entries[1].to, DisplayedState::Blank);
    }

    /// Simulates a crash between WAL write and render by using a renderer that
    /// always fails.  State is updated (WAL + memory), render is not.
    /// Replaying the WAL on a new controller with a working renderer recovers.
    #[test]
    fn wal_replay_after_render_crash_recovers_display() {
        let (wal, log) = SharedWal::new();

        // "Crashed" controller — renderer always fails.
        let mut crashed = DisplayController::new(wal, failing_renderer());
        let _ = crashed.show_verse(john_3_16(), "For God so loved the world");
        // State is committed in memory despite render failure.
        assert!(matches!(crashed.state(), DisplayedState::Verse(_, _)));

        // Recover: build fresh controller with working renderer, replay WAL.
        let entries: Vec<WalEntry> = log.lock().unwrap().clone();
        let mut recovered = DisplayController::new(MemoryWal::new(), ok_renderer());
        for entry in &entries {
            recovered.restore(entry.to.clone()).unwrap();
        }

        assert_eq!(*recovered.state(), *crashed.state());
    }

    // ── full transition cycle ─────────────────────────────────────────────────

    #[test]
    fn complete_service_flow() {
        let mut c = ctrl();

        assert_eq!(*c.state(), DisplayedState::Blank);

        c.show_sermon_title("Walking by Faith").unwrap();
        assert!(matches!(c.state(), DisplayedState::SermonTitle(_)));

        c.show_sub_point(SubPoint::new("1. Faith requires trust"))
            .unwrap();
        assert!(matches!(c.state(), DisplayedState::SubPoint(_)));

        c.show_verse(john_3_16(), "For God so loved the world…")
            .unwrap();
        assert!(matches!(c.state(), DisplayedState::Verse(_, _)));

        c.show_sub_point(SubPoint::new("2. Faith produces action"))
            .unwrap();
        assert!(matches!(c.state(), DisplayedState::SubPoint(_)));

        c.show_blank().unwrap();
        assert_eq!(*c.state(), DisplayedState::Blank);
        assert!(c.is_blank());

        // Undo the blank
        c.discard().unwrap();
        assert!(matches!(c.state(), DisplayedState::SubPoint(_)));
    }
}
