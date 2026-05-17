use std::sync::Mutex;
use tauri::{AppHandle, Emitter, Manager, PhysicalPosition, PhysicalSize, State, WebviewWindow};
use companion_display::{DisplayMonitor, MonitorLayout};

// ─── window labels ────────────────────────────────────────────────────────────

const OPERATOR_LABEL: &str = "operator";
const CONGREGATION_LABEL: &str = "congregation";

// ─── application state ────────────────────────────────────────────────────────

#[derive(serde::Serialize, Clone, PartialEq, Default, Debug)]
#[serde(rename_all = "snake_case")]
enum DisplayMode {
    #[default]
    Idle,
    Blank,
    Verse,
    Title,
    Subpoint,
}

#[derive(Default)]
struct InternalState {
    display_mode: DisplayMode,
    session_active: bool,
}

struct ManagedState(Mutex<InternalState>);

#[derive(serde::Serialize)]
#[serde(rename_all = "camelCase")]
struct AppState {
    display_mode: DisplayMode,
    session_active: bool,
    congregation_visible: bool,
    total_screens: usize,
    has_secondary_screen: bool,
}

// ─── screen management ────────────────────────────────────────────────────────

fn monitor_count(app: &AppHandle) -> usize {
    app.get_webview_window(OPERATOR_LABEL)
        .and_then(|w| w.available_monitors().ok())
        .map(|m| m.len())
        .unwrap_or(1)
}

fn secondary_monitor(op_win: &WebviewWindow) -> Option<tauri::Monitor> {
    let primary = op_win.primary_monitor().ok().flatten()?;
    let monitors = op_win.available_monitors().ok()?;
    monitors
        .into_iter()
        .find(|m| m.position() != primary.position())
}

fn assign_congregation_to_secondary(app: &AppHandle) -> bool {
    let Some(cong_win) = congregation_window(app) else {
        return false;
    };
    let Some(op_win) = app.get_webview_window(OPERATOR_LABEL) else {
        return false;
    };
    let Some(screen) = secondary_monitor(&op_win) else {
        return false;
    };
    let pos = screen.position();
    let size = screen.size();
    let _ = cong_win.set_position(PhysicalPosition::new(pos.x, pos.y));
    let _ = cong_win.set_size(PhysicalSize::new(size.width, size.height));
    true
}

/// `true` when the congregation window's top-left corner lies outside the
/// primary monitor bounds, meaning it is positioned on the secondary screen.
fn congregation_on_secondary(app: &AppHandle) -> bool {
    let Some(op_win) = app.get_webview_window(OPERATOR_LABEL) else {
        return false;
    };
    let Some(primary) = op_win.primary_monitor().ok().flatten() else {
        return false;
    };
    let Some(cong_win) = congregation_window(app) else {
        return false;
    };
    let Ok(cong_pos) = cong_win.outer_position() else {
        return false;
    };
    let p_pos  = primary.position();
    let p_size = primary.size();
    let on_primary = cong_pos.x >= p_pos.x
        && cong_pos.x <  p_pos.x + p_size.width  as i32
        && cong_pos.y >= p_pos.y
        && cong_pos.y <  p_pos.y + p_size.height as i32;
    !on_primary
}

fn current_layout(app: &AppHandle) -> MonitorLayout {
    MonitorLayout::new(monitor_count(app), congregation_on_secondary(app))
}

fn watch_screens(app: AppHandle) {
    use companion_display::ScreenStatus;

    let mut monitor = DisplayMonitor::new(current_layout(&app));

    std::thread::Builder::new()
        .name("screen-watcher".into())
        .spawn(move || {
            loop {
                std::thread::sleep(std::time::Duration::from_secs(2));
                let layout = current_layout(&app);

                if let Some(status) = monitor.update(layout) {
                    let event_type = match status {
                        ScreenStatus::Connected => {
                            assign_congregation_to_secondary(&app);
                            if let Some(w) = congregation_window(&app) {
                                let _ = w.show();
                            }
                            "SECONDARY_SCREEN_CONNECTED"
                        }
                        ScreenStatus::Disconnected => {
                            if let Some(w) = congregation_window(&app) {
                                let _ = w.hide();
                            }
                            "SECONDARY_SCREEN_DISCONNECTED"
                        }
                        ScreenStatus::Swapped => "SCREEN_SWAP_DETECTED",
                    };
                    let _ = app.emit("app-event", serde_json::json!({ "type": event_type }));
                }
            }
        })
        .expect("failed to spawn screen-watcher thread");
}

// ─── state command ────────────────────────────────────────────────────────────

/// Return the full application state: display mode, session, screen info,
/// and congregation window visibility. Called by the operator on startup.
#[tauri::command]
fn get_app_state(app: AppHandle, state: State<ManagedState>) -> AppState {
    let s = state.0.lock().unwrap();
    let count = monitor_count(&app);
    let congregation_visible = congregation_window(&app)
        .and_then(|w| w.is_visible().ok())
        .unwrap_or(false);
    AppState {
        display_mode: s.display_mode.clone(),
        session_active: s.session_active,
        congregation_visible,
        total_screens: count,
        has_secondary_screen: count > 1,
    }
}

// ─── screen commands ──────────────────────────────────────────────────────────

#[tauri::command]
fn get_screen_info(app: AppHandle) -> serde_json::Value {
    let count = monitor_count(&app);
    serde_json::json!({ "totalScreens": count, "hasSecondaryScreen": count > 1 })
}

/// One-click fix for the swap scenario: reassigns the congregation window to
/// the secondary screen and emits `SCREEN_RESTORED` so the operator UI
/// can clear the swap alert.
#[tauri::command]
fn fix_screen_swap(app: AppHandle) {
    assign_congregation_to_secondary(&app);
    if let Some(w) = congregation_window(&app) {
        let _ = w.show();
        let _ = w.set_focus();
    }
    let _ = app.emit("app-event", serde_json::json!({ "type": "SCREEN_RESTORED" }));
}

#[tauri::command]
fn show_congregation_window(app: AppHandle) {
    assign_congregation_to_secondary(&app);
    if let Some(w) = congregation_window(&app) {
        let _ = w.show();
        let _ = w.set_focus();
    }
}

#[tauri::command]
fn hide_congregation_window(app: AppHandle) {
    if let Some(w) = congregation_window(&app) {
        let _ = w.hide();
    }
}

// ─── display commands ─────────────────────────────────────────────────────────

/// Parse "Book Chapter[:Verse[-VerseEnd]]" into the BibleReference JSON shape.
/// Uses rsplitn so multi-word book names (e.g. "1 Corinthians") are preserved.
fn parse_reference(s: &str) -> Option<serde_json::Value> {
    let mut parts = s.rsplitn(2, ' ');
    let chapter_verse = parts.next()?;
    let book = parts.next()?;
    if let Some((ch_str, verse_str)) = chapter_verse.split_once(':') {
        let chapter: u8 = ch_str.parse().ok()?;
        if let Some((from_str, to_str)) = verse_str.split_once('-') {
            let from: u8 = from_str.parse().ok()?;
            let to: u8 = to_str.parse().ok()?;
            Some(serde_json::json!({ "book": book, "chapter": chapter, "verse": from, "verse_end": to }))
        } else {
            let verse: u8 = verse_str.parse().ok()?;
            Some(serde_json::json!({ "book": book, "chapter": chapter, "verse": verse, "verse_end": null }))
        }
    } else {
        let chapter: u8 = chapter_verse.parse().ok()?;
        Some(serde_json::json!({ "book": book, "chapter": chapter, "verse": null, "verse_end": null }))
    }
}

#[tauri::command]
fn show_verse(app: AppHandle, state: State<ManagedState>, reference: String, text: String) {
    let Some(ref_json) = parse_reference(&reference) else { return };
    let _ = app.emit(
        "app-event",
        serde_json::json!({
            "type": "VERSE_LOADED",
            "reference": ref_json.clone(),
            "text": text,
            "translation": "KJV",
        }),
    );
    let _ = app.emit(
        "app-event",
        serde_json::json!({ "type": "VERSE_DISPLAYED", "reference": ref_json }),
    );
    state.0.lock().unwrap().display_mode = DisplayMode::Verse;
}

#[tauri::command]
fn discard_verse(app: AppHandle, state: State<ManagedState>) {
    let _ = app.emit("app-event", serde_json::json!({ "type": "DISPLAY_CLEARED" }));
    state.0.lock().unwrap().display_mode = DisplayMode::Idle;
}

#[tauri::command]
fn show_sermon_title(app: AppHandle, state: State<ManagedState>, title: String) {
    let _ = app.emit(
        "app-event",
        serde_json::json!({ "type": "SERMON_TITLE_SHOWN", "title": title }),
    );
    state.0.lock().unwrap().display_mode = DisplayMode::Title;
}

#[tauri::command]
fn show_sub_point(app: AppHandle, state: State<ManagedState>, sub_point: String) {
    let _ = app.emit(
        "app-event",
        serde_json::json!({ "type": "SUB_POINT_SHOWN", "text": sub_point }),
    );
    state.0.lock().unwrap().display_mode = DisplayMode::Subpoint;
}

/// Black out the congregation display entirely — no logo, no content.
#[tauri::command]
fn show_blank(app: AppHandle, state: State<ManagedState>) {
    let _ = app.emit("app-event", serde_json::json!({ "type": "DISPLAY_BLANKED" }));
    state.0.lock().unwrap().display_mode = DisplayMode::Blank;
}

// ─── session commands ─────────────────────────────────────────────────────────

#[tauri::command]
fn start_session(state: State<ManagedState>) {
    state.0.lock().unwrap().session_active = true;
    // TODO: start audio capture pipeline
}

#[tauri::command]
fn stop_session(state: State<ManagedState>) {
    state.0.lock().unwrap().session_active = false;
    // TODO: stop audio capture pipeline
}

// ─── operator action commands ─────────────────────────────────────────────────

#[tauri::command]
fn approve_detection(app: AppHandle, reference: String) {
    // TODO: look up verse text from Bible package and call show_verse
    let _ = (app, reference);
}

#[tauri::command]
fn reject_detection(_app: AppHandle, reference: String) {
    // TODO: log rejection to database
    let _ = reference;
}

#[tauri::command]
fn clear_congregation_display(app: AppHandle, state: State<ManagedState>) {
    let _ = app.emit("app-event", serde_json::json!({ "type": "DISPLAY_CLEARED" }));
    state.0.lock().unwrap().display_mode = DisplayMode::Idle;
}

// ─── helpers ──────────────────────────────────────────────────────────────────

fn congregation_window(app: &AppHandle) -> Option<WebviewWindow> {
    app.get_webview_window(CONGREGATION_LABEL)
}

// ─── tests ────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    // ── parse_reference ────────────────────────────────────────────────────────

    fn ref_val(
        book: &str,
        chapter: u8,
        verse: Option<u8>,
        verse_end: Option<u8>,
    ) -> serde_json::Value {
        serde_json::json!({ "book": book, "chapter": chapter, "verse": verse, "verse_end": verse_end })
    }

    #[test]
    fn parse_explicit_verse() {
        assert_eq!(
            parse_reference("John 3:16"),
            Some(ref_val("John", 3, Some(16), None))
        );
    }

    #[test]
    fn parse_chapter_only() {
        assert_eq!(
            parse_reference("Genesis 1"),
            Some(ref_val("Genesis", 1, None, None))
        );
    }

    #[test]
    fn parse_verse_range() {
        assert_eq!(
            parse_reference("Revelation 22:20-21"),
            Some(ref_val("Revelation", 22, Some(20), Some(21)))
        );
    }

    #[test]
    fn parse_multi_word_book() {
        assert_eq!(
            parse_reference("1 Corinthians 13:4"),
            Some(ref_val("1 Corinthians", 13, Some(4), None))
        );
    }

    #[test]
    fn parse_numbered_book_chapter_only() {
        assert_eq!(
            parse_reference("2 Kings 5"),
            Some(ref_val("2 Kings", 5, None, None))
        );
    }

    #[test]
    fn parse_three_word_book() {
        assert_eq!(
            parse_reference("Song of Solomon 3:2"),
            Some(ref_val("Song of Solomon", 3, Some(2), None))
        );
    }

    #[test]
    fn parse_large_chapter() {
        assert_eq!(
            parse_reference("Psalm 119:176"),
            Some(ref_val("Psalm", 119, Some(176), None))
        );
    }

    #[test]
    fn parse_empty_returns_none() {
        assert_eq!(parse_reference(""), None);
    }

    #[test]
    fn parse_book_only_returns_none() {
        assert_eq!(parse_reference("John"), None);
    }

    #[test]
    fn parse_non_numeric_chapter_returns_none() {
        assert_eq!(parse_reference("John abc"), None);
    }

    #[test]
    fn parse_overflow_chapter_returns_none() {
        // u8 max is 255; 300 does not fit
        assert_eq!(parse_reference("Psalm 300"), None);
    }

    // ── window management / state ──────────────────────────────────────────────

    #[test]
    fn display_mode_default_is_idle() {
        let s = InternalState::default();
        assert_eq!(s.display_mode, DisplayMode::Idle);
    }

    #[test]
    fn session_inactive_by_default() {
        let s = InternalState::default();
        assert!(!s.session_active);
    }

    #[test]
    fn display_mode_serializes_to_snake_case() {
        let cases = [
            (DisplayMode::Idle, "\"idle\""),
            (DisplayMode::Blank, "\"blank\""),
            (DisplayMode::Verse, "\"verse\""),
            (DisplayMode::Title, "\"title\""),
            (DisplayMode::Subpoint, "\"subpoint\""),
        ];
        for (mode, expected) in cases {
            let json = serde_json::to_string(&mode).unwrap();
            assert_eq!(json, expected);
        }
    }

    #[test]
    fn managed_state_tracks_display_mode() {
        let managed = ManagedState(Mutex::new(InternalState::default()));
        managed.0.lock().unwrap().display_mode = DisplayMode::Verse;
        assert_eq!(managed.0.lock().unwrap().display_mode, DisplayMode::Verse);
    }

    #[test]
    fn managed_state_tracks_session() {
        let managed = ManagedState(Mutex::new(InternalState::default()));
        managed.0.lock().unwrap().session_active = true;
        assert!(managed.0.lock().unwrap().session_active);
        managed.0.lock().unwrap().session_active = false;
        assert!(!managed.0.lock().unwrap().session_active);
    }

    #[test]
    fn display_mode_cycles_through_all_states() {
        let managed = ManagedState(Mutex::new(InternalState::default()));
        let states = [
            DisplayMode::Verse,
            DisplayMode::Title,
            DisplayMode::Subpoint,
            DisplayMode::Blank,
            DisplayMode::Idle,
        ];
        for state in states {
            managed.0.lock().unwrap().display_mode = state.clone();
            assert_eq!(managed.0.lock().unwrap().display_mode, state);
        }
    }
}

// ─── entry point ──────────────────────────────────────────────────────────────

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_autostart::init(
            tauri_plugin_autostart::MacosLauncher::LaunchAgent,
            None,
        ))
        .setup(|app| {
            app.manage(ManagedState(Mutex::new(InternalState::default())));
            let handle = app.handle().clone();
            assign_congregation_to_secondary(&handle);
            watch_screens(handle);
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            get_app_state,
            get_screen_info,
            show_congregation_window,
            hide_congregation_window,
            fix_screen_swap,
            show_verse,
            discard_verse,
            show_sermon_title,
            show_sub_point,
            show_blank,
            start_session,
            stop_session,
            approve_detection,
            reject_detection,
            clear_congregation_display,
        ])
        .run(tauri::generate_context!())
        .expect("error while running companion bible");
}
