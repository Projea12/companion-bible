use tauri::{AppHandle, Emitter, Manager, PhysicalPosition, PhysicalSize, WebviewWindow};

// ─── window labels ────────────────────────────────────────────────────────────

const OPERATOR_LABEL: &str = "operator";
const CONGREGATION_LABEL: &str = "congregation";

// ─── screen info type ─────────────────────────────────────────────────────────

#[derive(serde::Serialize)]
#[serde(rename_all = "camelCase")]
struct ScreenInfo {
    total_screens: usize,
    has_secondary_screen: bool,
}

// ─── screen management ────────────────────────────────────────────────────────

/// How many monitors are currently connected, as seen from the operator window.
fn monitor_count(app: &AppHandle) -> usize {
    app.get_webview_window(OPERATOR_LABEL)
        .and_then(|w| w.available_monitors().ok())
        .map(|m| m.len())
        .unwrap_or(1)
}

/// Find the first non-primary monitor.
fn secondary_monitor(op_win: &WebviewWindow) -> Option<tauri::Monitor> {
    let primary = op_win.primary_monitor().ok().flatten()?;
    let monitors = op_win.available_monitors().ok()?;
    monitors
        .into_iter()
        .find(|m| m.position() != primary.position())
}

/// Move and resize the congregation window to fill the secondary monitor.
/// Returns true if a secondary monitor was found and the window was repositioned.
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

/// Spawn a background thread that polls monitor count every 2 s and emits
/// SECONDARY_SCREEN_CONNECTED / SECONDARY_SCREEN_DISCONNECTED on changes.
fn watch_screens(app: AppHandle) {
    let initial_count = monitor_count(&app);

    std::thread::Builder::new()
        .name("screen-watcher".into())
        .spawn(move || {
            let mut last_count = initial_count;
            loop {
                std::thread::sleep(std::time::Duration::from_secs(2));

                let current = monitor_count(&app);
                if current == last_count {
                    continue;
                }

                let had_secondary = last_count > 1;
                let has_secondary = current > 1;
                last_count = current;

                if has_secondary && !had_secondary {
                    assign_congregation_to_secondary(&app);
                    let _ = app.emit(
                        "app-event",
                        serde_json::json!({ "type": "SECONDARY_SCREEN_CONNECTED" }),
                    );
                } else if !has_secondary && had_secondary {
                    if let Some(w) = congregation_window(&app) {
                        let _ = w.hide();
                    }
                    let _ = app.emit(
                        "app-event",
                        serde_json::json!({ "type": "SECONDARY_SCREEN_DISCONNECTED" }),
                    );
                }
            }
        })
        .expect("failed to spawn screen-watcher thread");
}

// ─── screen commands ──────────────────────────────────────────────────────────

/// Return the current monitor count and whether a secondary screen is present.
#[tauri::command]
fn get_screen_info(app: AppHandle) -> ScreenInfo {
    let count = monitor_count(&app);
    ScreenInfo {
        total_screens: count,
        has_secondary_screen: count > 1,
    }
}

// ─── congregation window commands ─────────────────────────────────────────────

/// Make the congregation window visible on the secondary display.
/// Re-assigns the window position each time in case the monitor changed.
#[tauri::command]
fn show_congregation_window(app: AppHandle) {
    assign_congregation_to_secondary(&app);
    if let Some(w) = congregation_window(&app) {
        let _ = w.show();
        let _ = w.set_focus();
    }
}

/// Hide the congregation window (keeps it resident in memory for fast reshow).
#[tauri::command]
fn hide_congregation_window(app: AppHandle) {
    if let Some(w) = congregation_window(&app) {
        let _ = w.hide();
    }
}

// ─── session commands ─────────────────────────────────────────────────────────

#[tauri::command]
fn start_session(_app: AppHandle) {
    // TODO: start audio capture pipeline
}

#[tauri::command]
fn stop_session(_app: AppHandle) {
    // TODO: stop audio capture pipeline
}

// ─── operator action commands ─────────────────────────────────────────────────

#[tauri::command]
fn approve_detection(app: AppHandle, reference: String) {
    // TODO: load verse text and emit VERSE_LOADED event
    let _ = (app, reference);
}

#[tauri::command]
fn reject_detection(_app: AppHandle, reference: String) {
    // TODO: log rejection to database
    let _ = reference;
}

#[tauri::command]
fn clear_congregation_display(app: AppHandle) {
    let _ = app.emit("app-event", serde_json::json!({ "type": "DISPLAY_CLEARED" }));
}

// ─── helpers ──────────────────────────────────────────────────────────────────

fn congregation_window(app: &AppHandle) -> Option<WebviewWindow> {
    app.get_webview_window(CONGREGATION_LABEL)
}

// ─── entry point ──────────────────────────────────────────────────────────────

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .setup(|app| {
            let handle = app.handle().clone();
            assign_congregation_to_secondary(&handle);
            watch_screens(handle);
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            get_screen_info,
            show_congregation_window,
            hide_congregation_window,
            start_session,
            stop_session,
            approve_detection,
            reject_detection,
            clear_congregation_display,
        ])
        .run(tauri::generate_context!())
        .expect("error while running companion bible");
}
