use tauri::{AppHandle, Emitter, Manager, WebviewWindow};

// ─── window labels ────────────────────────────────────────────────────────────

const OPERATOR_LABEL: &str = "operator";
const CONGREGATION_LABEL: &str = "congregation";

// ─── congregation window commands ─────────────────────────────────────────────

/// Make the congregation window visible on the secondary display.
#[tauri::command]
fn show_congregation_window(app: AppHandle) {
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

/// Signal the backend to begin audio capture and detection.
/// Full implementation wired in task 6.x; shell stub emitted here.
#[tauri::command]
fn start_session(_app: AppHandle) {
    // TODO: start audio capture pipeline
}

/// Signal the backend to stop audio capture and detection.
#[tauri::command]
fn stop_session(_app: AppHandle) {
    // TODO: stop audio capture pipeline
}

// ─── operator action commands ─────────────────────────────────────────────────

/// Operator approved a detection — display it in the congregation window.
#[tauri::command]
fn approve_detection(app: AppHandle, reference: String) {
    // TODO: load verse text and emit VERSE_LOADED event
    let _ = (app, reference);
}

/// Operator rejected a detection — do nothing on the congregation display.
#[tauri::command]
fn reject_detection(_app: AppHandle, reference: String) {
    // TODO: log rejection to database
    let _ = reference;
}

/// Clear the congregation display and return to the idle state.
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
        .plugin(tauri_plugin_window_state::Builder::default().build())
        .invoke_handler(tauri::generate_handler![
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
