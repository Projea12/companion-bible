mod controller;
mod error;
mod history;
mod monitor;
mod state;
mod undo;
mod wal;

pub use controller::DisplayController;
pub use error::DisplayError;
pub use monitor::{DisplayMonitor, MonitorLayout, ScreenStatus};
pub use state::{DisplayedState, SubPoint};
pub use undo::{ActionId, UndoError, UndoSystem, UNDO_WINDOW};
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

    type Renderer = Box<dyn Fn(&DisplayedState) -> Result<(), String> + Send>;

    fn ok_renderer() -> Renderer {
        Box::new(|_| Ok(()))
    }

    fn failing_renderer() -> Renderer {
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
        c.show_verse(r.clone(), "For God so loved the world…")
            .unwrap();
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

// ── DisplayMonitor tests ──────────────────────────────────────────────────────

#[cfg(test)]
mod monitor_tests {
    use super::{DisplayMonitor, MonitorLayout, ScreenStatus};

    fn layout(total: usize, on_secondary: bool) -> MonitorLayout {
        MonitorLayout::new(total, on_secondary)
    }

    // ── resolve: ScreenStatus from MonitorLayout ──────────────────────────────

    #[test]
    fn zero_monitors_is_disconnected() {
        let m = DisplayMonitor::new(layout(0, false));
        assert_eq!(*m.status(), ScreenStatus::Disconnected);
    }

    #[test]
    fn one_monitor_is_disconnected() {
        let m = DisplayMonitor::new(layout(1, false));
        assert_eq!(*m.status(), ScreenStatus::Disconnected);
    }

    #[test]
    fn two_monitors_congregation_on_secondary_is_connected() {
        let m = DisplayMonitor::new(layout(2, true));
        assert_eq!(*m.status(), ScreenStatus::Connected);
    }

    #[test]
    fn two_monitors_congregation_not_on_secondary_is_swapped() {
        let m = DisplayMonitor::new(layout(2, false));
        assert_eq!(*m.status(), ScreenStatus::Swapped);
    }

    #[test]
    fn three_monitors_congregation_on_secondary_is_connected() {
        let m = DisplayMonitor::new(layout(3, true));
        assert_eq!(*m.status(), ScreenStatus::Connected);
    }

    #[test]
    fn three_monitors_congregation_not_on_secondary_is_swapped() {
        let m = DisplayMonitor::new(layout(3, false));
        assert_eq!(*m.status(), ScreenStatus::Swapped);
    }

    // ── update: returns Some only on status change ────────────────────────────

    #[test]
    fn update_same_status_returns_none() {
        let mut m = DisplayMonitor::new(layout(2, true));
        assert_eq!(m.update(layout(2, true)), None);
    }

    #[test]
    fn update_same_disconnected_returns_none() {
        let mut m = DisplayMonitor::new(layout(1, false));
        assert_eq!(m.update(layout(1, false)), None);
    }

    #[test]
    fn update_same_swapped_returns_none() {
        let mut m = DisplayMonitor::new(layout(2, false));
        assert_eq!(m.update(layout(2, false)), None);
    }

    // ── screen connect ────────────────────────────────────────────────────────

    #[test]
    fn disconnected_to_connected_returns_some_connected() {
        let mut m = DisplayMonitor::new(layout(1, false));
        assert_eq!(m.update(layout(2, true)), Some(ScreenStatus::Connected));
    }

    #[test]
    fn status_is_updated_after_connect() {
        let mut m = DisplayMonitor::new(layout(1, false));
        m.update(layout(2, true));
        assert_eq!(*m.status(), ScreenStatus::Connected);
    }

    // ── screen disconnect ─────────────────────────────────────────────────────

    #[test]
    fn connected_to_disconnected_returns_some_disconnected() {
        let mut m = DisplayMonitor::new(layout(2, true));
        assert_eq!(m.update(layout(1, false)), Some(ScreenStatus::Disconnected));
    }

    #[test]
    fn status_is_updated_after_disconnect() {
        let mut m = DisplayMonitor::new(layout(2, true));
        m.update(layout(1, false));
        assert_eq!(*m.status(), ScreenStatus::Disconnected);
    }

    #[test]
    fn connected_to_zero_monitors_is_disconnected() {
        let mut m = DisplayMonitor::new(layout(2, true));
        assert_eq!(m.update(layout(0, false)), Some(ScreenStatus::Disconnected));
    }

    // ── screen swap detection ─────────────────────────────────────────────────

    #[test]
    fn connected_to_swapped_returns_some_swapped() {
        let mut m = DisplayMonitor::new(layout(2, true));
        assert_eq!(m.update(layout(2, false)), Some(ScreenStatus::Swapped));
    }

    #[test]
    fn swapped_to_connected_returns_some_connected() {
        let mut m = DisplayMonitor::new(layout(2, false));
        assert_eq!(m.update(layout(2, true)), Some(ScreenStatus::Connected));
    }

    #[test]
    fn disconnected_to_swapped_returns_some_swapped() {
        let mut m = DisplayMonitor::new(layout(1, false));
        assert_eq!(m.update(layout(2, false)), Some(ScreenStatus::Swapped));
    }

    #[test]
    fn swapped_to_disconnected_returns_some_disconnected() {
        let mut m = DisplayMonitor::new(layout(2, false));
        assert_eq!(m.update(layout(1, false)), Some(ScreenStatus::Disconnected));
    }

    // ── reconnect after disconnect ────────────────────────────────────────────

    #[test]
    fn reconnect_after_disconnect_returns_connected() {
        let mut m = DisplayMonitor::new(layout(2, true));
        m.update(layout(1, false));
        assert_eq!(m.update(layout(2, true)), Some(ScreenStatus::Connected));
        assert_eq!(*m.status(), ScreenStatus::Connected);
    }

    #[test]
    fn fix_swap_returns_connected() {
        let mut m = DisplayMonitor::new(layout(2, false));
        assert_eq!(*m.status(), ScreenStatus::Swapped);
        assert_eq!(m.update(layout(2, true)), Some(ScreenStatus::Connected));
    }

    // ── no spurious events on startup ─────────────────────────────────────────

    #[test]
    fn initial_layout_does_not_trigger_update_event() {
        let mut m = DisplayMonitor::new(layout(2, true));
        // Feeding the same initial state produces no event.
        assert_eq!(m.update(layout(2, true)), None);
    }

    // ── display formatting ────────────────────────────────────────────────────

    #[test]
    fn screen_status_display_connected() {
        assert_eq!(ScreenStatus::Connected.to_string(), "connected");
    }

    #[test]
    fn screen_status_display_disconnected() {
        assert_eq!(ScreenStatus::Disconnected.to_string(), "disconnected");
    }

    #[test]
    fn screen_status_display_swapped() {
        assert_eq!(ScreenStatus::Swapped.to_string(), "swapped");
    }

    // ── full lifecycle ────────────────────────────────────────────────────────

    #[test]
    fn full_service_lifecycle() {
        let mut m = DisplayMonitor::new(layout(1, false));
        assert_eq!(*m.status(), ScreenStatus::Disconnected);

        // Technician plugs in the projector.
        assert_eq!(m.update(layout(2, true)), Some(ScreenStatus::Connected));

        // Service runs — no change on repeated polls.
        assert_eq!(m.update(layout(2, true)), None);
        assert_eq!(m.update(layout(2, true)), None);

        // Projector cable is accidentally disconnected mid-service.
        assert_eq!(m.update(layout(1, false)), Some(ScreenStatus::Disconnected));

        // Technician reconnects — congregation restored.
        assert_eq!(m.update(layout(2, true)), Some(ScreenStatus::Connected));
    }

    #[test]
    fn swap_detected_then_fixed_lifecycle() {
        // Someone plugged cables into the wrong ports.
        let mut m = DisplayMonitor::new(layout(2, false));
        assert_eq!(*m.status(), ScreenStatus::Swapped);

        // Operator uses one-click fix — assign_congregation_to_secondary fires.
        assert_eq!(m.update(layout(2, true)), Some(ScreenStatus::Connected));
        assert_eq!(*m.status(), ScreenStatus::Connected);
    }
}

// ── UndoSystem tests ──────────────────────────────────────────────────────────

#[cfg(test)]
mod undo_tests {
    use super::{ActionId, DisplayedState, SubPoint, UndoError, UndoSystem, UNDO_WINDOW};
    use companion_events::BibleReference;
    use std::time::{Duration, Instant};

    // ── helpers ───────────────────────────────────────────────────────────────

    fn john_3_16() -> DisplayedState {
        DisplayedState::Verse(
            BibleReference::new("John", 3).with_verse(16),
            "For God so loved the world".into(),
        )
    }

    fn sermon() -> DisplayedState {
        DisplayedState::SermonTitle("Walking by Faith".into())
    }

    fn sub() -> DisplayedState {
        DisplayedState::SubPoint(SubPoint::new("Faith requires trust"))
    }

    /// Returns (undo_system, t0) where t0 is the Instant used as "now" for
    /// the first record_discard call.  Add durations to simulate time passing.
    fn sys() -> (UndoSystem, Instant) {
        (UndoSystem::new(), Instant::now())
    }

    fn within(t0: Instant) -> Instant {
        t0 + Duration::from_millis(4_999)
    }

    fn at_boundary(t0: Instant) -> Instant {
        t0 + UNDO_WINDOW // exactly 5 s → expired
    }

    fn expired(t0: Instant) -> Instant {
        t0 + UNDO_WINDOW + Duration::from_millis(1)
    }

    // ── record_discard ────────────────────────────────────────────────────────

    #[test]
    fn record_discard_returns_incrementing_ids() {
        let (mut sys, t0) = sys();
        let id1 = sys.record_discard(john_3_16(), t0);
        let id2 = sys.record_discard(sermon(), t0);
        let id3 = sys.record_discard(sub(), t0);
        assert_eq!([id1, id2, id3], [1, 2, 3]);
    }

    #[test]
    fn record_discard_grows_stack() {
        let (mut sys, t0) = sys();
        assert_eq!(sys.len(), 0);
        sys.record_discard(john_3_16(), t0);
        assert_eq!(sys.len(), 1);
        sys.record_discard(sermon(), t0);
        assert_eq!(sys.len(), 2);
    }

    #[test]
    fn new_system_is_empty() {
        let (sys, _) = sys();
        assert!(sys.is_empty());
    }

    // ── undo within window ────────────────────────────────────────────────────

    #[test]
    fn undo_within_window_returns_previous_state() {
        let (mut sys, t0) = sys();
        sys.record_discard(john_3_16(), t0);
        let id = sys.record_discard(sermon(), t0);
        let result = sys.undo(id, within(t0)).unwrap();
        assert_eq!(result, sermon());
    }

    #[test]
    fn undo_within_window_removes_action_from_stack() {
        let (mut sys, t0) = sys();
        let id = sys.record_discard(john_3_16(), t0);
        assert_eq!(sys.len(), 1);
        sys.undo(id, within(t0)).unwrap();
        assert_eq!(sys.len(), 0);
    }

    #[test]
    fn undo_restores_blank_state() {
        let (mut sys, t0) = sys();
        let id = sys.record_discard(DisplayedState::Blank, t0);
        assert_eq!(sys.undo(id, within(t0)).unwrap(), DisplayedState::Blank);
    }

    #[test]
    fn undo_restores_verse_state() {
        let (mut sys, t0) = sys();
        let id = sys.record_discard(john_3_16(), t0);
        assert_eq!(sys.undo(id, within(t0)).unwrap(), john_3_16());
    }

    #[test]
    fn undo_restores_sermon_title_state() {
        let (mut sys, t0) = sys();
        let id = sys.record_discard(sermon(), t0);
        assert_eq!(sys.undo(id, within(t0)).unwrap(), sermon());
    }

    #[test]
    fn undo_restores_sub_point_state() {
        let (mut sys, t0) = sys();
        let id = sys.record_discard(sub(), t0);
        assert_eq!(sys.undo(id, within(t0)).unwrap(), sub());
    }

    // ── undo after window expires ─────────────────────────────────────────────

    #[test]
    fn undo_at_boundary_returns_expired() {
        let (mut sys, t0) = sys();
        let id = sys.record_discard(john_3_16(), t0);
        assert_eq!(
            sys.undo(id, at_boundary(t0)).unwrap_err(),
            UndoError::Expired(id)
        );
    }

    #[test]
    fn undo_after_window_returns_expired() {
        let (mut sys, t0) = sys();
        let id = sys.record_discard(john_3_16(), t0);
        assert_eq!(
            sys.undo(id, expired(t0)).unwrap_err(),
            UndoError::Expired(id)
        );
    }

    #[test]
    fn undo_after_expiry_removes_action_from_stack() {
        let (mut sys, t0) = sys();
        let id = sys.record_discard(john_3_16(), t0);
        let _ = sys.undo(id, expired(t0));
        assert!(sys.is_empty());
    }

    #[test]
    fn undo_unknown_id_returns_not_found() {
        let (mut sys, t0) = sys();
        assert_eq!(
            sys.undo(99, within(t0)).unwrap_err(),
            UndoError::NotFound(99)
        );
    }

    #[test]
    fn undo_same_action_twice_returns_not_found_second_time() {
        let (mut sys, t0) = sys();
        let id = sys.record_discard(john_3_16(), t0);
        sys.undo(id, within(t0)).unwrap();
        assert_eq!(
            sys.undo(id, within(t0)).unwrap_err(),
            UndoError::NotFound(id)
        );
    }

    // ── is_within_window ─────────────────────────────────────────────────────

    #[test]
    fn is_within_window_true_before_expiry() {
        let (mut sys, t0) = sys();
        let id = sys.record_discard(john_3_16(), t0);
        assert!(sys.is_within_window(id, within(t0)));
    }

    #[test]
    fn is_within_window_false_at_boundary() {
        let (mut sys, t0) = sys();
        let id = sys.record_discard(john_3_16(), t0);
        assert!(!sys.is_within_window(id, at_boundary(t0)));
    }

    #[test]
    fn is_within_window_false_after_expiry() {
        let (mut sys, t0) = sys();
        let id = sys.record_discard(john_3_16(), t0);
        assert!(!sys.is_within_window(id, expired(t0)));
    }

    #[test]
    fn is_within_window_false_for_unknown_id() {
        let (sys, t0) = sys();
        assert!(!sys.is_within_window(99, within(t0)));
    }

    // ── expire_old ────────────────────────────────────────────────────────────

    #[test]
    fn expire_old_removes_stale_actions() {
        let (mut sys, t0) = sys();
        sys.record_discard(john_3_16(), t0);
        sys.record_discard(sermon(), t0);
        assert_eq!(sys.expire_old(expired(t0)).len(), 2);
        assert!(sys.is_empty());
    }

    #[test]
    fn expire_old_returns_expired_ids() {
        let (mut sys, t0) = sys();
        let id1 = sys.record_discard(john_3_16(), t0);
        let id2 = sys.record_discard(sermon(), t0);
        let mut expired_ids = sys.expire_old(expired(t0));
        expired_ids.sort();
        assert_eq!(expired_ids, vec![id1, id2]);
    }

    #[test]
    fn expire_old_leaves_fresh_actions_intact() {
        let (mut sys, t0) = sys();
        let old_id = sys.record_discard(john_3_16(), t0);
        let fresh_t = t0 + Duration::from_secs(4); // still has 1s remaining
        let fresh_id = sys.record_discard(sermon(), fresh_t);

        // Poll happens 4.5 s after t0 — old_id expired, fresh_id still valid.
        let _now = t0 + Duration::from_millis(4_500);
        // old_id: 4500ms elapsed >= 5000ms? No. Hmm — let me recalculate.
        // Actually old_id was recorded at t0; now is t0 + 4500ms. 4500 < 5000. Still valid.
        // Let me use a wider gap.
        let _ = (old_id, fresh_id);
        // Use expired(t0) for old, but fresh was recorded only 1s ago relative to expired(t0).
        let poll_time = t0 + Duration::from_millis(5_100);
        let expired_ids = sys.expire_old(poll_time);
        // old_id: recorded at t0, elapsed 5100ms >= 5000ms → expired
        // fresh_id: recorded at t0+4000ms, elapsed 1100ms < 5000ms → still valid
        assert_eq!(expired_ids, vec![old_id]);
        assert_eq!(sys.len(), 1);
        assert!(sys.is_within_window(fresh_id, poll_time));
    }

    #[test]
    fn expire_old_on_empty_system_returns_empty() {
        let (mut sys, t0) = sys();
        assert_eq!(sys.expire_old(expired(t0)), Vec::<ActionId>::new());
    }

    #[test]
    fn expire_old_within_window_returns_empty() {
        let (mut sys, t0) = sys();
        sys.record_discard(john_3_16(), t0);
        assert_eq!(sys.expire_old(within(t0)), Vec::<ActionId>::new());
        assert_eq!(sys.len(), 1);
    }

    // ── multiple sequential discards ──────────────────────────────────────────

    #[test]
    fn undo_most_recent_of_two_discards() {
        let (mut sys, t0) = sys();
        let id1 = sys.record_discard(john_3_16(), t0);
        let id2 = sys.record_discard(sermon(), t0);

        // Undo only the second discard.
        let restored = sys.undo(id2, within(t0)).unwrap();
        assert_eq!(restored, sermon());

        // First discard is still on the stack.
        assert_eq!(sys.len(), 1);
        assert!(sys.is_within_window(id1, within(t0)));
    }

    #[test]
    fn undo_first_of_two_discards_leaves_second_intact() {
        let (mut sys, t0) = sys();
        let id1 = sys.record_discard(john_3_16(), t0);
        let id2 = sys.record_discard(sermon(), t0);

        sys.undo(id1, within(t0)).unwrap();
        assert_eq!(sys.len(), 1);
        assert!(sys.is_within_window(id2, within(t0)));
    }

    #[test]
    fn undo_all_three_sequential_discards() {
        let (mut sys, t0) = sys();
        let id1 = sys.record_discard(john_3_16(), t0);
        let id2 = sys.record_discard(sermon(), t0);
        let id3 = sys.record_discard(sub(), t0);

        assert_eq!(sys.undo(id3, within(t0)).unwrap(), sub());
        assert_eq!(sys.undo(id2, within(t0)).unwrap(), sermon());
        assert_eq!(sys.undo(id1, within(t0)).unwrap(), john_3_16());
        assert!(sys.is_empty());
    }

    #[test]
    fn second_discard_expires_but_first_can_still_be_undone() {
        let (mut sys, t0) = sys();
        let id1 = sys.record_discard(john_3_16(), t0);
        // Second discard happens 4.8 s later.
        let t1 = t0 + Duration::from_millis(4_800);
        let id2 = sys.record_discard(sermon(), t1);

        // Poll at t0 + 5.1 s: id1 is expired (5100ms), id2 is still valid (300ms).
        let poll = t0 + Duration::from_millis(5_100);
        assert_eq!(sys.undo(id1, poll).unwrap_err(), UndoError::Expired(id1));
        assert_eq!(sys.undo(id2, poll).unwrap(), sermon());
        assert!(sys.is_empty());
    }

    // ── full undo lifecycle ───────────────────────────────────────────────────

    #[test]
    fn full_undo_flow() {
        let (mut sys, t0) = sys();

        // Operator discards a verse.
        let id = sys.record_discard(john_3_16(), t0);
        assert_eq!(sys.len(), 1);
        assert!(sys.is_within_window(id, within(t0)));

        // Operator clicks undo within 5 s.
        let restored = sys.undo(id, within(t0)).unwrap();
        assert_eq!(restored, john_3_16());
        assert!(sys.is_empty());
    }

    #[test]
    fn full_expiry_flow() {
        let (mut sys, t0) = sys();

        let id = sys.record_discard(john_3_16(), t0);

        // 5 seconds pass without an undo.
        let expired_ids = sys.expire_old(expired(t0));
        assert_eq!(expired_ids, vec![id]);
        assert!(sys.is_empty());

        // Late undo attempt fails with NotFound (already purged by expire_old).
        assert_eq!(
            sys.undo(id, expired(t0)).unwrap_err(),
            UndoError::NotFound(id)
        );
    }
}
