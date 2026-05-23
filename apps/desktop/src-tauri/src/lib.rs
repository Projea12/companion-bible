use companion_ai::{LocalAI, LocalAIConfig};
use companion_arbitrator::DisplayAction;
use companion_audio::{AudioSystem, BuiltinMicInput, RingBuffer};
use companion_bible::KjvBible;
use companion_database::{
    CalibrationRepository, ChurchRepository, DetectionEventRepository, PoolConfig, VerseRepository,
};
use companion_display::{DisplayMonitor, MonitorLayout};
use companion_engine::{
    DetectionEngine, DisplayMode as EngineDisplayMode, EngineConfig, HymnSession, LocalAiHandle,
};
use companion_events::AppEvent;
use companion_hymns::HymnBook;
use companion_transcription::{AssemblyAiTranscriber, DeepgramTranscriber, TranscribeOptions};
#[cfg(not(target_os = "windows"))]
use companion_transcription::{ModelManager, SetupProgress, WhisperTranscriber};
use std::sync::{Arc, Mutex};
use tauri::{AppHandle, Emitter, Manager, PhysicalPosition, PhysicalSize, State, WebviewWindow};

// ─── window labels ────────────────────────────────────────────────────────────

const OPERATOR_LABEL: &str = "operator";
const CONGREGATION_LABEL: &str = "congregation";

// ─── display mode ─────────────────────────────────────────────────────────────

#[derive(serde::Serialize, Clone, PartialEq, Default, Debug)]
#[serde(rename_all = "snake_case")]
enum DisplayMode {
    #[default]
    Idle,
    Blank,
    Verse,
    Title,
    Subpoint,
    Hymn,
}

// ─── pipeline ─────────────────────────────────────────────────────────────────

enum AnyTranscriber {
    #[cfg(not(target_os = "windows"))]
    Whisper(WhisperTranscriber),
    Deepgram(DeepgramTranscriber),
    AssemblyAi(AssemblyAiTranscriber),
}

impl AnyTranscriber {
    fn stop(&mut self) {
        match self {
            #[cfg(not(target_os = "windows"))]
            Self::Whisper(t) => t.stop(),
            Self::Deepgram(t) => t.stop(),
            Self::AssemblyAi(t) => t.stop(),
        }
    }

    fn mode_label(&self) -> &'static str {
        match self {
            #[cfg(not(target_os = "windows"))]
            Self::Whisper(_) => "whisper",
            Self::Deepgram(_) => "deepgram",
            Self::AssemblyAi(_) => "assemblyai",
        }
    }
}

struct Pipeline {
    audio: AudioSystem,
    transcriber: AnyTranscriber,
}

// SAFETY: AudioSystem, WhisperTranscriber, and DeepgramTranscriber are Send.
unsafe impl Send for Pipeline {}

// ─── internal state ───────────────────────────────────────────────────────────

struct InternalState {
    display_mode: DisplayMode,
    session_active: bool,
    last_verse: Option<(String, String)>,
    current_displayed_ref: Option<(String, u8, u8)>, // (book, chapter, verse)
    sermon_active: bool,
    sermon_title: Option<String>,
    sub_points: Vec<String>,
    current_sub_point_index: i32,
    selected_device_id: Option<String>,
    assemblyai_api_key: Option<String>,
    deepgram_api_key: Option<String>,
    openai_api_key: Option<String>,
    pipeline: Option<Pipeline>,
    /// Hymn session used when no audio session is active (manual load).
    hymn_session: Option<HymnSession>,
}

impl Default for InternalState {
    fn default() -> Self {
        Self {
            display_mode: DisplayMode::Idle,
            session_active: false,
            last_verse: None,
            current_displayed_ref: None,
            sermon_active: false,
            sermon_title: None,
            sub_points: Vec::new(),
            current_sub_point_index: -1,
            selected_device_id: None,
            assemblyai_api_key: None,
            deepgram_api_key: None,
            openai_api_key: None,
            pipeline: None,
            hymn_session: None,
        }
    }
}

// ─── managed state ────────────────────────────────────────────────────────────

struct ManagedState {
    inner: Mutex<InternalState>,
    engine: Arc<tokio::sync::Mutex<Option<DetectionEngine>>>,
    bible: Arc<Mutex<Option<KjvBible>>>,
}

impl ManagedState {
    fn new() -> Self {
        Self {
            inner: Mutex::new(InternalState::default()),
            engine: Arc::new(tokio::sync::Mutex::new(None)),
            bible: Arc::new(Mutex::new(None)),
        }
    }
}

// ─── AppState (serialised to frontend) ───────────────────────────────────────

#[derive(serde::Serialize)]
#[serde(rename_all = "camelCase")]
struct AppState {
    display_mode: DisplayMode,
    session_active: bool,
    congregation_visible: bool,
    total_screens: usize,
    has_secondary_screen: bool,
    sermon_active: bool,
    sermon_title: Option<String>,
    sub_point_index: i32,
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
    let p_pos = primary.position();
    let p_size = primary.size();
    let on_primary = cong_pos.x >= p_pos.x
        && cong_pos.x < p_pos.x + p_size.width as i32
        && cong_pos.y >= p_pos.y
        && cong_pos.y < p_pos.y + p_size.height as i32;
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
        .spawn(move || loop {
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
        })
        .expect("failed to spawn screen-watcher thread");
}

// ─── state command ────────────────────────────────────────────────────────────

#[tauri::command]
fn get_app_state(app: AppHandle, state: State<ManagedState>) -> AppState {
    let s = state.inner.lock().unwrap();
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
        sermon_active: s.sermon_active,
        sermon_title: s.sermon_title.clone(),
        sub_point_index: s.current_sub_point_index,
    }
}

// ─── screen commands ──────────────────────────────────────────────────────────

#[tauri::command]
fn get_screen_info(app: AppHandle) -> serde_json::Value {
    let count = monitor_count(&app);
    serde_json::json!({ "totalScreens": count, "hasSecondaryScreen": count > 1 })
}

#[tauri::command]
fn fix_screen_swap(app: AppHandle) {
    assign_congregation_to_secondary(&app);
    if let Some(w) = congregation_window(&app) {
        let _ = w.show();
        let _ = w.set_focus();
    }
    let _ = app.emit(
        "app-event",
        serde_json::json!({ "type": "SCREEN_RESTORED" }),
    );
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

fn parse_reference(s: &str) -> Option<serde_json::Value> {
    let (book, chapter_verse) = s.rsplit_once(' ')?;
    if let Some((ch_str, verse_str)) = chapter_verse.split_once(':') {
        let chapter: u8 = ch_str.parse().ok()?;
        if let Some((from_str, to_str)) = verse_str.split_once('-') {
            let from: u8 = from_str.parse().ok()?;
            let to: u8 = to_str.parse().ok()?;
            Some(
                serde_json::json!({ "book": book, "chapter": chapter, "verse": from, "verse_end": to }),
            )
        } else {
            let verse: u8 = verse_str.parse().ok()?;
            Some(
                serde_json::json!({ "book": book, "chapter": chapter, "verse": verse, "verse_end": null }),
            )
        }
    } else {
        let chapter: u8 = chapter_verse.parse().ok()?;
        Some(
            serde_json::json!({ "book": book, "chapter": chapter, "verse": null, "verse_end": null }),
        )
    }
}

/// Look up verse text from the in-memory KjvBible given a parsed reference JSON.
fn lookup_verse_text(
    bible_arc: &Arc<Mutex<Option<KjvBible>>>,
    ref_json: &serde_json::Value,
) -> String {
    let guard = bible_arc.lock().unwrap();
    let Some(bible) = guard.as_ref() else {
        return String::new();
    };
    let book = ref_json["book"].as_str().unwrap_or_default();
    let chapter = ref_json["chapter"].as_u64().unwrap_or(1) as u8;
    let Some(verse_u64) = ref_json["verse"].as_u64() else {
        return String::new();
    };
    bible
        .get_verse(book, chapter, verse_u64 as u8)
        .map(|v| v.text.clone())
        .unwrap_or_default()
}

#[tauri::command]
fn show_verse(app: AppHandle, state: State<ManagedState>, reference: String, text: String) {
    let Some(ref_json) = parse_reference(&reference) else {
        return;
    };
    let actual_text = if text.is_empty() {
        lookup_verse_text(&state.bible, &ref_json)
    } else {
        text
    };
    let _ = app.emit(
        "app-event",
        serde_json::json!({
            "type": "VERSE_LOADED",
            "reference": ref_json.clone(),
            "text": actual_text.clone(),
            "translation": "KJV",
        }),
    );
    let _ = app.emit(
        "app-event",
        serde_json::json!({ "type": "VERSE_DISPLAYED", "reference": ref_json }),
    );
    let mut s = state.inner.lock().unwrap();
    s.display_mode = DisplayMode::Verse;
    s.last_verse = Some((reference.clone(), actual_text));
    // Track current position for next/prev verse navigation.
    if let Some(rv) = ref_json["verse"].as_u64() {
        let book = ref_json["book"].as_str().unwrap_or_default().to_string();
        let chapter = ref_json["chapter"].as_u64().unwrap_or(1) as u8;
        s.current_displayed_ref = Some((book, chapter, rv as u8));
    }
}

#[tauri::command]
fn discard_verse(app: AppHandle, state: State<ManagedState>) {
    let _ = app.emit(
        "app-event",
        serde_json::json!({ "type": "DISPLAY_CLEARED" }),
    );
    state.inner.lock().unwrap().display_mode = DisplayMode::Idle;
}

#[tauri::command]
fn undo_discard(app: AppHandle, state: State<ManagedState>) {
    let last = state.inner.lock().unwrap().last_verse.clone();
    let Some((reference, text)) = last else {
        return;
    };
    let Some(ref_json) = parse_reference(&reference) else {
        return;
    };
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
    state.inner.lock().unwrap().display_mode = DisplayMode::Verse;
}

#[tauri::command]
fn show_sermon_title(app: AppHandle, state: State<ManagedState>, title: String) {
    let _ = app.emit(
        "app-event",
        serde_json::json!({ "type": "SERMON_TITLE_SHOWN", "title": title }),
    );
    state.inner.lock().unwrap().display_mode = DisplayMode::Title;
}

#[tauri::command]
fn show_sub_point(app: AppHandle, state: State<ManagedState>, sub_point: String) {
    let _ = app.emit(
        "app-event",
        serde_json::json!({ "type": "SUB_POINT_SHOWN", "text": sub_point }),
    );
    state.inner.lock().unwrap().display_mode = DisplayMode::Subpoint;
}

#[tauri::command]
fn show_blank(app: AppHandle, state: State<ManagedState>) {
    let _ = app.emit(
        "app-event",
        serde_json::json!({ "type": "DISPLAY_BLANKED" }),
    );
    state.inner.lock().unwrap().display_mode = DisplayMode::Blank;
}

#[tauri::command]
fn clear_congregation_display(app: AppHandle, state: State<ManagedState>) {
    let _ = app.emit(
        "app-event",
        serde_json::json!({ "type": "DISPLAY_CLEARED" }),
    );
    state.inner.lock().unwrap().display_mode = DisplayMode::Idle;
}

// ─── session commands ─────────────────────────────────────────────────────────

// ─── Transcription backend helper ────────────────────────────────────────────

/// Try Deepgram; on macOS/Linux fall back to Whisper, on Windows error out.
#[cfg(not(target_os = "windows"))]
async fn try_deepgram_or_whisper(
    deepgram_key: Option<String>,
    raw_window: Arc<Mutex<companion_audio::SlidingWindow>>,
    processed_window: Arc<Mutex<companion_audio::SlidingWindow>>,
    app: &AppHandle,
) -> (companion_transcription::SegmentReceiver, AnyTranscriber) {
    if let Some(ref key) = deepgram_key {
        match DeepgramTranscriber::try_connect(key).await {
            Ok(_) => {
                eprintln!("[start_session] step 7a: Deepgram OK — using raw audio path");
                let (mut dg, rx) = DeepgramTranscriber::new(key.clone(), raw_window);
                dg.start();
                let _ = app.emit(
                    "app-event",
                    serde_json::json!({
                        "type": "TRANSCRIPTION_MODE_CHANGED", "mode": "deepgram",
                    }),
                );
                return (rx, AnyTranscriber::Deepgram(dg));
            }
            Err(e) => {
                eprintln!(
                    "[start_session] step 7a: Deepgram failed ({e}) — falling back to Whisper"
                );
                let _ = app.emit(
                    "app-event",
                    serde_json::json!({
                        "type": "TRANSCRIPTION_MODE_CHANGED",
                        "mode": "whisper",
                        "reason": format!("Deepgram unavailable: {e}"),
                    }),
                );
            }
        }
    } else {
        eprintln!("[start_session] step 7a: no Deepgram key — using Whisper");
    }
    let (t, rx) = WhisperTranscriber::new(processed_window, TranscribeOptions::default());
    (rx, AnyTranscriber::Whisper(t))
}

/// On Windows, Whisper is unavailable — Deepgram is the only local fallback.
#[cfg(target_os = "windows")]
async fn try_deepgram_or_whisper(
    deepgram_key: Option<String>,
    raw_window: Arc<Mutex<companion_audio::SlidingWindow>>,
    _processed_window: Arc<Mutex<companion_audio::SlidingWindow>>,
    app: &AppHandle,
) -> (companion_transcription::SegmentReceiver, AnyTranscriber) {
    if let Some(ref key) = deepgram_key {
        match DeepgramTranscriber::try_connect(key).await {
            Ok(_) => {
                eprintln!("[start_session] step 7a: Deepgram OK");
                let (mut dg, rx) = DeepgramTranscriber::new(key.clone(), raw_window);
                dg.start();
                let _ = app.emit(
                    "app-event",
                    serde_json::json!({
                        "type": "TRANSCRIPTION_MODE_CHANGED", "mode": "deepgram",
                    }),
                );
                return (rx, AnyTranscriber::Deepgram(dg));
            }
            Err(e) => {
                eprintln!("[start_session] step 7a: Deepgram failed ({e}) — no Whisper fallback on Windows");
            }
        }
    }
    // No Deepgram key and no Whisper on Windows — return a channel that never sends.
    // The user must configure AssemblyAI or Deepgram in the settings.
    let (tx, rx) = companion_transcription::segment_channel();
    drop(tx);
    eprintln!(
        "[start_session] Windows: no STT backend available — configure AssemblyAI or Deepgram"
    );
    let (mut dg, _) = DeepgramTranscriber::new(String::new(), raw_window);
    dg.stop();
    (rx, AnyTranscriber::Deepgram(dg))
}

/// Start the full audio → transcription → detection pipeline.
///
/// Loads the KJV Bible, connects to SQLite, creates the DetectionEngine,
/// then starts the audio capture + Whisper transcription loop.  A background
/// task forwards DetectionDecisions to the display layer.
#[tauri::command]
async fn start_session(app: AppHandle, state: State<'_, ManagedState>) -> Result<(), String> {
    {
        if state.inner.lock().unwrap().session_active {
            return Ok(());
        }
    }

    eprintln!("[start_session] step 1: loading KJV Bible");

    // ── 1. Load KJV Bible (twice: one for state lookups, one for engine) ──────
    let bible_path = resolve_bible_path(&app);
    eprintln!("[start_session] bible path: {}", bible_path.display());
    let (bible_state, bible_engine) = tokio::task::spawn_blocking({
        let p = bible_path.clone();
        move || -> Result<(KjvBible, KjvBible), String> {
            let b1 = KjvBible::load(&p).map_err(|e| e.to_string())?;
            let b2 = KjvBible::load(&p).map_err(|e| e.to_string())?;
            Ok((b1, b2))
        }
    })
    .await
    .map_err(|e| e.to_string())??;

    *state.bible.lock().unwrap() = Some(bible_state);
    eprintln!("[start_session] step 1 done: Bible loaded");

    // ── 2. Connect to database ────────────────────────────────────────────────
    eprintln!("[start_session] step 2: connecting to database");
    let db_path = app
        .path()
        .app_data_dir()
        .map_err(|e| e.to_string())?
        .join("companion.db");
    eprintln!("[start_session] db path: {}", db_path.display());
    let pool = companion_database::connect(&db_path, &PoolConfig::default())
        .await
        .map_err(|e| e.to_string())?;
    eprintln!("[start_session] step 2 done: database connected");

    // ── 3. Create repos + ensure default church exists ────────────────────────
    eprintln!("[start_session] step 3: creating repositories");
    let church_repo = ChurchRepository::new(pool.clone());
    let calibration_repo = CalibrationRepository::new(pool.clone());
    let detection_repo = DetectionEventRepository::new(pool.clone());
    let verse_repo = VerseRepository::new(pool.clone());
    let _ = church_repo
        .get_or_create("companion-default", "Default Church", "local")
        .await;
    eprintln!("[start_session] step 3 done");

    // ── 4. Build a session-scoped sermon ID ───────────────────────────────────
    let sermon_id = format!(
        "session-{}",
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs()
    );

    // ── 5a. Load Phi-3 local AI (optional — degrades gracefully if missing) ────
    let phi3_path = std::path::PathBuf::from(
        "/Users/johnolugbemi/Documents/companion-bible/models/phi3/Phi-3-mini-4k-instruct-q4.gguf",
    );
    let local_ai_handle = if phi3_path.exists() {
        eprintln!("[start_session] step 5a: loading Phi-3 via phi3-worker subprocess");
        match LocalAI::load(LocalAIConfig::new(phi3_path)) {
            Ok(ai) => {
                eprintln!("[start_session] step 5a done: Phi-3 worker ready");
                Some(LocalAiHandle::spawn(ai))
            }
            Err(e) => {
                eprintln!("[start_session] step 5a: Phi-3 unavailable ({e}), continuing without");
                None
            }
        }
    } else {
        eprintln!("[start_session] step 5a: Phi-3 model not found, continuing without");
        None
    };

    // ── 5. Create DetectionEngine ─────────────────────────────────────────────
    eprintln!("[start_session] step 5: creating detection engine");
    let openai_api_key = state.inner.lock().unwrap().openai_api_key.clone();
    let mut engine = DetectionEngine::new(
        EngineConfig {
            sermon_id,
            openai_api_key,
            api_key: None,
        },
        bible_engine,
        church_repo,
        calibration_repo,
        detection_repo,
        verse_repo,
        local_ai_handle,
    )
    .await
    .map_err(|e| format!("{e:?}"))?;
    eprintln!("[start_session] step 5 done: engine ready");

    // ── 6. Wire event relay: engine → Tauri emitter ───────────────────────────
    let (event_tx, event_rx) = std::sync::mpsc::channel::<AppEvent>();
    engine.set_event_sender(event_tx);

    let app_relay = app.clone();
    std::thread::Builder::new()
        .name("event-relay".into())
        .spawn(move || {
            while let Ok(ev) = event_rx.recv() {
                let _ = app_relay.emit("app-event", &ev);
            }
        })
        .map_err(|e| e.to_string())?;

    *state.engine.lock().await = Some(engine);

    // ── 7. Build audio system ────────────────────────────────────────────────
    eprintln!("[start_session] step 7: building audio system");
    let buffer = Arc::new(RingBuffer::<f32>::with_default_capacity());
    let audio = AudioSystem::new(Box::new(BuiltinMicInput::new()), Arc::clone(&buffer));
    // raw_window: downsample-only, no noise processing — for cloud APIs.
    // window: full pipeline (gate → RNNoise → normalize) — for local Whisper.
    let raw_window = audio.raw_window();
    let processed_window = audio.window();
    eprintln!("[start_session] step 7 done: audio system built");

    // ── 7a. Choose transcription backend: AssemblyAI → Deepgram → Whisper ───
    let assemblyai_key = state.inner.lock().unwrap().assemblyai_api_key.clone();
    let deepgram_key = state.inner.lock().unwrap().deepgram_api_key.clone();

    // ── 7b. Create transcriber and get seg_rx ────────────────────────────────
    let (seg_rx_raw, transcriber) = if let Some(ref key) = assemblyai_key {
        eprintln!("[start_session] step 7a: testing AssemblyAI connection…");
        match AssemblyAiTranscriber::try_connect(key).await {
            Ok(_) => {
                eprintln!("[start_session] step 7a: AssemblyAI OK — using raw audio path");
                // Raw window: send unprocessed audio, AssemblyAI handles its own denoising.
                let (mut aai, rx) = AssemblyAiTranscriber::new(key.clone(), raw_window.clone());
                aai.start();
                let _ = app.emit(
                    "app-event",
                    serde_json::json!({
                        "type": "TRANSCRIPTION_MODE_CHANGED", "mode": "assemblyai",
                    }),
                );
                (rx, AnyTranscriber::AssemblyAi(aai))
            }
            Err(e) => {
                eprintln!("[start_session] step 7a: AssemblyAI failed ({e}) — trying Deepgram");
                try_deepgram_or_whisper(
                    deepgram_key,
                    raw_window.clone(),
                    processed_window.clone(),
                    &app,
                )
                .await
            }
        }
    } else {
        eprintln!("[start_session] step 7a: no AssemblyAI key — trying Deepgram");
        try_deepgram_or_whisper(
            deepgram_key,
            raw_window.clone(),
            processed_window.clone(),
            &app,
        )
        .await
    };

    // ── 8. Bridge blocking SegmentReceiver → tokio channel ───────────────────
    eprintln!("[start_session] step 8: spawning seg-bridge thread");
    let (seg_tx, mut tokio_rx) =
        tokio::sync::mpsc::channel::<companion_transcription::TranscriptionSegment>(32);

    std::thread::Builder::new()
        .name("seg-bridge".into())
        .spawn(move || {
            while let Some(batch) = seg_rx_raw.recv() {
                for seg in batch {
                    if seg_tx.blocking_send(seg).is_err() {
                        break;
                    }
                }
            }
        })
        .map_err(|e| e.to_string())?;
    eprintln!("[start_session] step 8 done: seg-bridge thread spawned");

    // ── 9. Spawn segment-processing task ─────────────────────────────────────
    eprintln!("[start_session] step 9: spawning segment-processing task");
    let engine_arc = Arc::clone(&state.engine);
    let bible_arc = Arc::clone(&state.bible);
    let app_display = app.clone();
    let inner_arc = Arc::new(Mutex::new(())); // used only for display-mode updates below

    eprintln!("[start_session] step 9 done: processing task spawned");

    tauri::async_runtime::spawn(async move {
        let _ = inner_arc; // keep alive
                           // Track last displayed reference to suppress duplicate emissions.
        let mut last_displayed: Option<(String, u8, Option<u8>)> = None;

        while let Some(segment) = tokio_rx.recv().await {
            // Emit TRANSCRIPTION_COMPLETED so the operator transcript panel shows text.
            let _ = app_display.emit(
                "app-event",
                &AppEvent::TranscriptionCompleted {
                    chunk_id: segment.audio_start_ms,
                    text: segment.text.clone(),
                    duration_ms: segment.audio_end_ms.saturating_sub(segment.audio_start_ms) as u32,
                },
            );

            let decision = {
                let mut guard = engine_arc.lock().await;
                match guard.as_mut() {
                    Some(eng) => eng.process(segment).await,
                    None => break,
                }
            };

            if let (DisplayAction::AutoDisplay, Some(ref_)) = (decision.action, decision.reference)
            {
                // Suppress if this is the same reference already on screen.
                let key = (ref_.book.clone(), ref_.chapter, ref_.verse);
                if last_displayed.as_ref() == Some(&key) {
                    continue;
                }
                last_displayed = Some(key);

                let verse_text = {
                    let guard = bible_arc.lock().unwrap();
                    if let (Some(bible), Some(verse_num)) = (guard.as_ref(), ref_.verse) {
                        bible
                            .get_verse(&ref_.book, ref_.chapter, verse_num)
                            .map(|v| v.text.clone())
                            .unwrap_or_default()
                    } else {
                        String::new()
                    }
                };

                let ev_ref = companion_events::BibleReference {
                    book: ref_.book.clone(),
                    chapter: ref_.chapter,
                    verse: ref_.verse,
                    verse_end: ref_.verse_end,
                };
                let _ = app_display.emit(
                    "app-event",
                    &AppEvent::VerseLoaded {
                        reference: ev_ref.clone(),
                        text: verse_text,
                        translation: "KJV".into(),
                    },
                );
                let _ =
                    app_display.emit("app-event", &AppEvent::VerseDisplayed { reference: ev_ref });
            }
        }
    });

    // ── 10. Load Whisper model (only when using Whisper backend) ─────────────
    let mut transcriber = transcriber;
    #[cfg(not(target_os = "windows"))]
    if matches!(transcriber, AnyTranscriber::Whisper(_)) {
        eprintln!("[start_session] step 10: loading Whisper model (may take a while)");
        let app_data_dir = app.path().app_data_dir().map_err(|e| e.to_string())?;
        eprintln!("[start_session] app_data_dir: {}", app_data_dir.display());
        let app_progress = app.clone();
        let model = tokio::task::spawn_blocking(move || {
            eprintln!("[start_session] step 10: ModelManager::setup starting");
            let result = ModelManager::new(&app_data_dir).setup(|progress| {
                eprintln!(
                    "[start_session] step 10: model download progress {:?}",
                    progress
                );
                match &progress {
                    SetupProgress::Downloading {
                        bytes_done,
                        bytes_total,
                    } => {
                        let pct = bytes_total
                            .filter(|&t| t > 0)
                            .map(|t| ((*bytes_done as f64 / t as f64) * 100.0) as u8)
                            .unwrap_or(0);
                        let _ = app_progress.emit(
                            "app-event",
                            serde_json::json!({
                                "type": "MODEL_DOWNLOAD_PROGRESS",
                                "bytesDone": bytes_done,
                                "bytesTotal": bytes_total,
                                "percent": pct,
                            }),
                        );
                    }
                    SetupProgress::Loading => {
                        let _ = app_progress.emit(
                            "app-event",
                            serde_json::json!({
                                "type": "MODEL_DOWNLOAD_PROGRESS",
                                "bytesDone": 0u64,
                                "bytesTotal": Option::<u64>::None,
                                "percent": 100u8,
                                "label": "Loading model…",
                            }),
                        );
                    }
                    _ => {}
                }
            });
            eprintln!(
                "[start_session] step 10: ModelManager::setup finished: {}",
                result.is_ok()
            );
            result
        })
        .await
        .map_err(|e| e.to_string())?
        .map_err(|e| e.to_string())?;
        eprintln!("[start_session] step 10 done: Whisper model loaded");
        if let AnyTranscriber::Whisper(ref mut wt) = transcriber {
            wt.start(model);
        }
    } else {
        eprintln!("[start_session] step 10: skipped (non-Whisper backend)");
    }
    #[cfg(target_os = "windows")]
    eprintln!("[start_session] step 10: skipped (Whisper not available on Windows)");

    // ── 11. Start audio capture ───────────────────────────────────────────────
    eprintln!("[start_session] step 11: starting audio capture");
    let mut audio = audio;
    audio
        .start(Arc::clone(&buffer), 0.005, 0.1)
        .map_err(|e| e.to_string())?;
    eprintln!("[start_session] step 11 done: audio started");

    // ── 11b. Audio level diagnostic (before audio is moved into pipeline) ───────
    // If reading stays 0.000, macOS microphone permission is likely denied.
    eprintln!(
        "[audio-check] immediate level: {:.4}",
        audio.current_level()
    );

    // ── 12. Store pipeline + mark session active ──────────────────────────────
    eprintln!("[start_session] step 12: storing pipeline");
    {
        let mut s = state.inner.lock().unwrap();
        s.pipeline = Some(Pipeline { audio, transcriber });
        s.session_active = true;
    }
    eprintln!("[start_session] step 12 done: session active!");

    let device_id = state
        .inner
        .lock()
        .unwrap()
        .selected_device_id
        .clone()
        .unwrap_or_else(|| "default".into());
    let _ = app.emit("app-event", &AppEvent::AudioCaptureStarted { device_id });

    Ok(())
}

/// Stop the audio pipeline and clear all session state.
#[tauri::command]
async fn stop_session(app: AppHandle, state: State<'_, ManagedState>) -> Result<(), String> {
    // Extract pipeline before any awaits (never hold std::Mutex across .await)
    let pipeline_opt = {
        let mut s = state.inner.lock().unwrap();
        s.session_active = false;
        s.pipeline.take()
    };

    if let Some(mut p) = pipeline_opt {
        p.transcriber.stop(); // works for both Whisper and Deepgram
        p.audio.stop();
    }

    // Clear engine; the event-relay thread exits when the engine's sender drops.
    *state.engine.lock().await = None;
    // Bible stays loaded — it's static data and show_verse must work after stop.

    let _ = app.emit("app-event", &AppEvent::AudioCaptureStopped);
    Ok(())
}

// ─── sermon commands ──────────────────────────────────────────────────────────

#[tauri::command]
fn start_sermon(
    app: AppHandle,
    state: State<ManagedState>,
    title: Option<String>,
    pastor: Option<String>,
    anchor_scripture: Option<String>,
) {
    {
        let mut s = state.inner.lock().unwrap();
        s.sermon_active = true;
        s.sermon_title = title.clone();
        s.sub_points.clear();
        s.current_sub_point_index = -1;
    }
    let _ = app.emit(
        "app-event",
        &AppEvent::SermonStarted {
            title,
            pastor,
            anchor_scripture,
        },
    );
}

#[tauri::command]
fn end_sermon(app: AppHandle, state: State<ManagedState>) {
    {
        let mut s = state.inner.lock().unwrap();
        s.sermon_active = false;
        s.sermon_title = None;
        s.sub_points.clear();
        s.current_sub_point_index = -1;
    }
    let _ = app.emit("app-event", &AppEvent::SermonEnded { summary: None });
}

#[tauri::command]
fn add_sub_point(app: AppHandle, state: State<ManagedState>, text: String) {
    let index = {
        let mut s = state.inner.lock().unwrap();
        if !s.sub_points.contains(&text) {
            s.sub_points.push(text.clone());
        }
        (s.sub_points.len() - 1) as u32
    };
    let _ = app.emit("app-event", &AppEvent::SubPointAdded { text, index });
}

#[tauri::command]
fn next_sub_point(app: AppHandle, state: State<ManagedState>) {
    let sub_point_text = {
        let mut s = state.inner.lock().unwrap();
        let next_idx = s.current_sub_point_index + 1;
        if next_idx < s.sub_points.len() as i32 {
            s.current_sub_point_index = next_idx;
            s.sub_points.get(next_idx as usize).cloned()
        } else {
            None
        }
    };
    if let Some(text) = sub_point_text {
        let _ = app.emit(
            "app-event",
            serde_json::json!({ "type": "SUB_POINT_SHOWN", "text": text }),
        );
        state.inner.lock().unwrap().display_mode = DisplayMode::Subpoint;
    }
}

// ─── hymn commands ────────────────────────────────────────────────────────────

/// Switch the operator display between "bible" and "hymn" mode.
#[tauri::command]
async fn set_display_mode(state: State<'_, ManagedState>, mode: String) -> Result<(), String> {
    let engine_mode = match mode.as_str() {
        "hymn" => EngineDisplayMode::Hymn,
        _ => EngineDisplayMode::Bible,
    };
    let local_mode = match mode.as_str() {
        "hymn" => DisplayMode::Hymn,
        _ => DisplayMode::Idle,
    };
    if let Some(eng) = state.engine.lock().await.as_mut() {
        eng.set_display_mode(engine_mode);
    }
    state.inner.lock().unwrap().display_mode = local_mode;
    Ok(())
}

/// Manually load a hymn by number (operator input).
#[tauri::command]
async fn load_hymn(
    app: AppHandle,
    state: State<'_, ManagedState>,
    number: u16,
) -> Result<bool, String> {
    let mut engine_guard = state.engine.lock().await;
    if let Some(eng) = engine_guard.as_mut() {
        // Active session — let the engine own the session and emit via its channel.
        let loaded = eng.load_hymn(number);
        if loaded {
            state.inner.lock().unwrap().display_mode = DisplayMode::Hymn;
        }
        return Ok(loaded);
    }
    drop(engine_guard);

    // No active session — load manually, store session, emit events directly.
    let Some(session) = HymnSession::load(number) else {
        return Ok(false);
    };
    let book = HymnBook::global();
    let title = book
        .get(number)
        .map(|h| h.title.clone())
        .unwrap_or_default();

    if let Some(companion_engine::HymnSessionEvent::Loaded {
        number: n,
        section_index,
        stanza_number,
        is_chorus,
        ref lines,
        ..
    }) = session.start_event()
    {
        let _ = app.emit("app-event", &AppEvent::HymnDetected { number: n, title });
        let _ = app.emit(
            "app-event",
            &AppEvent::HymnSectionAdvanced {
                number: n,
                section_index,
                stanza_number,
                is_chorus,
                lines: lines.clone(),
            },
        );
    }
    let mut s = state.inner.lock().unwrap();
    s.hymn_session = Some(session);
    s.display_mode = DisplayMode::Hymn;
    Ok(true)
}

/// Manually advance the active hymn to the next section (operator button).
#[tauri::command]
async fn next_hymn_stanza(app: AppHandle, state: State<'_, ManagedState>) -> Result<bool, String> {
    // Prefer the engine's session (active audio session).
    if let Some(eng) = state.engine.lock().await.as_mut() {
        return Ok(eng.advance_hymn());
    }

    // Sessionless path — advance the stored HymnSession and emit directly.
    let event = {
        let mut s = state.inner.lock().unwrap();
        s.hymn_session.as_mut().and_then(|sess| sess.advance())
    };
    let Some(event) = event else {
        return Ok(false);
    };
    match event {
        companion_engine::HymnSessionEvent::Advanced {
            number,
            section_index,
            stanza_number,
            is_chorus,
            lines,
        } => {
            let _ = app.emit(
                "app-event",
                &AppEvent::HymnSectionAdvanced {
                    number,
                    section_index,
                    stanza_number,
                    is_chorus,
                    lines,
                },
            );
        }
        companion_engine::HymnSessionEvent::Completed { number } => {
            let _ = app.emit("app-event", &AppEvent::HymnCompleted { number });
            state.inner.lock().unwrap().hymn_session = None;
        }
        _ => {}
    }
    Ok(true)
}

// ─── operator action commands ─────────────────────────────────────────────────

/// Confirm a detected reference — looks up verse text and displays it.
#[tauri::command]
fn approve_detection(app: AppHandle, state: State<ManagedState>, reference: String) {
    show_verse(app, state, reference, String::new());
}

/// Reject a detected reference — removes it from consideration.
#[tauri::command]
fn reject_detection(_app: AppHandle, reference: String) {
    // Detection event is already logged by the engine; no additional DB call needed.
    let _ = reference;
}

// ─── transcription config commands ───────────────────────────────────────────

#[tauri::command]
fn set_assemblyai_key(state: State<ManagedState>, key: String) {
    state.inner.lock().unwrap().assemblyai_api_key = if key.trim().is_empty() {
        None
    } else {
        Some(key.trim().to_string())
    };
}

#[tauri::command]
fn set_deepgram_key(state: State<ManagedState>, key: String) {
    state.inner.lock().unwrap().deepgram_api_key = if key.trim().is_empty() {
        None
    } else {
        Some(key.trim().to_string())
    };
}

#[tauri::command]
fn set_openai_key(state: State<ManagedState>, key: String) {
    state.inner.lock().unwrap().openai_api_key = if key.trim().is_empty() {
        None
    } else {
        Some(key.trim().to_string())
    };
}

#[tauri::command]
fn get_transcription_mode(state: State<ManagedState>) -> String {
    let s = state.inner.lock().unwrap();
    match &s.pipeline {
        Some(p) => p.transcriber.mode_label().to_string(),
        None => {
            if s.assemblyai_api_key.is_some() {
                "assemblyai".into()
            } else if s.deepgram_api_key.is_some() {
                "deepgram".into()
            } else {
                "whisper".into()
            }
        }
    }
}

// ─── verse navigation commands ────────────────────────────────────────────────

/// Advance to the next verse in the currently displayed chapter.
#[tauri::command]
fn next_verse(app: AppHandle, state: State<ManagedState>) {
    let (book, chapter, verse) = {
        let s = state.inner.lock().unwrap();
        match s.current_displayed_ref.clone() {
            Some(r) => r,
            None => return,
        }
    };
    let next = {
        let guard = state.bible.lock().unwrap();
        let Some(bible) = guard.as_ref() else { return };
        let Ok(total) = bible.verse_count(&book, chapter) else {
            return;
        };
        if verse >= total {
            return;
        }
        verse + 1
    };
    let reference = format!("{book} {chapter}:{next}");
    show_verse(app, state, reference, String::new());
}

/// Go back to the previous verse in the currently displayed chapter.
#[tauri::command]
fn previous_verse(app: AppHandle, state: State<ManagedState>) {
    let (book, chapter, verse) = {
        let s = state.inner.lock().unwrap();
        match s.current_displayed_ref.clone() {
            Some(r) => r,
            None => return,
        }
    };
    if verse <= 1 {
        return;
    }
    let reference = format!("{book} {chapter}:{}", verse - 1);
    show_verse(app, state, reference, String::new());
}

// ─── audio device commands ────────────────────────────────────────────────────

#[tauri::command]
fn get_audio_devices() -> Vec<serde_json::Value> {
    use companion_audio::AudioInput;
    let input = BuiltinMicInput::new();
    input
        .available_devices()
        .unwrap_or_default()
        .into_iter()
        .map(|d| serde_json::json!({ "id": d.id, "name": d.name }))
        .collect()
}

#[tauri::command]
fn select_audio_device(state: State<ManagedState>, device_id: String) {
    state.inner.lock().unwrap().selected_device_id = Some(device_id);
}

// ─── health command ───────────────────────────────────────────────────────────

#[tauri::command]
fn get_system_health(state: State<ManagedState>) -> serde_json::Value {
    let s = state.inner.lock().unwrap();
    serde_json::json!({
        "session": s.session_active,
        "audio": s.pipeline.as_ref().map(|p| p.audio.is_connected()).unwrap_or(false),
        "sermon": s.sermon_active,
    })
}

// ─── helpers ─────────────────────────────────────────────────────────────────

fn congregation_window(app: &AppHandle) -> Option<WebviewWindow> {
    app.get_webview_window(CONGREGATION_LABEL)
}

/// Resolve the KJV Bible path: resource dir in production, workspace path in dev.
fn resolve_bible_path(app: &AppHandle) -> std::path::PathBuf {
    if cfg!(debug_assertions) {
        // In dev, walk up from this crate to the workspace root.
        std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("../../../packages/bible/src/data/kjv.json")
    } else {
        app.path()
            .resource_dir()
            .unwrap_or_default()
            .join("kjv.json")
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
            let managed = ManagedState::new();
            // Load the KJV Bible immediately so show_verse works even before
            // a session is started (e.g. manual override at app launch).
            let bible_path = resolve_bible_path(app.handle());
            match KjvBible::load(&bible_path) {
                Ok(bible) => {
                    *managed.bible.lock().unwrap() = Some(bible);
                    eprintln!("[setup] KJV Bible loaded from {}", bible_path.display());
                }
                Err(e) => {
                    eprintln!("[setup] WARNING: failed to load KJV Bible: {e}");
                }
            }
            app.manage(managed);
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
            undo_discard,
            show_sermon_title,
            show_sub_point,
            show_blank,
            clear_congregation_display,
            start_session,
            stop_session,
            start_sermon,
            end_sermon,
            add_sub_point,
            next_sub_point,
            approve_detection,
            reject_detection,
            set_display_mode,
            load_hymn,
            next_hymn_stanza,
            next_verse,
            previous_verse,
            set_assemblyai_key,
            set_deepgram_key,
            set_openai_key,
            get_transcription_mode,
            get_audio_devices,
            select_audio_device,
            get_system_health,
        ])
        .run(tauri::generate_context!())
        .expect("error while running companion bible");
}

// ─── tests ────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    // ── helpers ───────────────────────────────────────────────────────────────

    fn ref_val(
        book: &str,
        chapter: u8,
        verse: Option<u8>,
        verse_end: Option<u8>,
    ) -> serde_json::Value {
        serde_json::json!({ "book": book, "chapter": chapter, "verse": verse, "verse_end": verse_end })
    }

    fn make_state() -> ManagedState {
        ManagedState::new()
    }

    // ── parse_reference ───────────────────────────────────────────────────────

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
        assert_eq!(parse_reference("Psalm 300"), None);
    }

    // ── InternalState defaults ────────────────────────────────────────────────

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
    fn sermon_inactive_by_default() {
        let s = InternalState::default();
        assert!(!s.sermon_active);
        assert!(s.sermon_title.is_none());
        assert!(s.sub_points.is_empty());
        assert_eq!(s.current_sub_point_index, -1);
    }

    // ── DisplayMode serialisation ─────────────────────────────────────────────

    #[test]
    fn display_mode_serializes_to_snake_case() {
        let cases = [
            (DisplayMode::Idle, "\"idle\""),
            (DisplayMode::Blank, "\"blank\""),
            (DisplayMode::Verse, "\"verse\""),
            (DisplayMode::Title, "\"title\""),
            (DisplayMode::Subpoint, "\"subpoint\""),
            (DisplayMode::Hymn, "\"hymn\""),
        ];
        for (mode, expected) in cases {
            assert_eq!(serde_json::to_string(&mode).unwrap(), expected);
        }
    }

    // ── ManagedState ops ──────────────────────────────────────────────────────

    #[test]
    fn managed_state_tracks_display_mode() {
        let ms = make_state();
        ms.inner.lock().unwrap().display_mode = DisplayMode::Verse;
        assert_eq!(ms.inner.lock().unwrap().display_mode, DisplayMode::Verse);
    }

    #[test]
    fn managed_state_tracks_session() {
        let ms = make_state();
        ms.inner.lock().unwrap().session_active = true;
        assert!(ms.inner.lock().unwrap().session_active);
        ms.inner.lock().unwrap().session_active = false;
        assert!(!ms.inner.lock().unwrap().session_active);
    }

    #[test]
    fn managed_state_tracks_sermon_lifecycle() {
        let ms = make_state();
        {
            let mut s = ms.inner.lock().unwrap();
            s.sermon_active = true;
            s.sermon_title = Some("Walking by Faith".into());
            s.sub_points.push("Intro".into());
            s.sub_points.push("Main Point".into());
            s.current_sub_point_index = 0;
        }
        {
            let s = ms.inner.lock().unwrap();
            assert!(s.sermon_active);
            assert_eq!(s.sermon_title.as_deref(), Some("Walking by Faith"));
            assert_eq!(s.sub_points.len(), 2);
            assert_eq!(s.current_sub_point_index, 0);
        }
        // End sermon
        {
            let mut s = ms.inner.lock().unwrap();
            s.sermon_active = false;
            s.sermon_title = None;
            s.sub_points.clear();
            s.current_sub_point_index = -1;
        }
        let s = ms.inner.lock().unwrap();
        assert!(!s.sermon_active);
        assert!(s.sermon_title.is_none());
        assert!(s.sub_points.is_empty());
        assert_eq!(s.current_sub_point_index, -1);
    }

    #[test]
    fn sub_point_index_advances_correctly() {
        let ms = make_state();
        {
            let mut s = ms.inner.lock().unwrap();
            s.sermon_active = true;
            s.sub_points = vec!["A".into(), "B".into(), "C".into()];
            s.current_sub_point_index = -1;
        }
        // Simulate next_sub_point logic
        let text = {
            let mut s = ms.inner.lock().unwrap();
            let next = s.current_sub_point_index + 1;
            if next < s.sub_points.len() as i32 {
                s.current_sub_point_index = next;
                s.sub_points.get(next as usize).cloned()
            } else {
                None
            }
        };
        assert_eq!(text.as_deref(), Some("A"));
        assert_eq!(ms.inner.lock().unwrap().current_sub_point_index, 0);
    }

    #[test]
    fn next_sub_point_clamps_at_end() {
        let ms = make_state();
        {
            let mut s = ms.inner.lock().unwrap();
            s.sub_points = vec!["Only".into()];
            s.current_sub_point_index = 0;
        }
        let text = {
            let mut s = ms.inner.lock().unwrap();
            let next = s.current_sub_point_index + 1;
            if next < s.sub_points.len() as i32 {
                s.current_sub_point_index = next;
                s.sub_points.get(next as usize).cloned()
            } else {
                None
            }
        };
        assert!(text.is_none(), "must return None past last sub-point");
        assert_eq!(ms.inner.lock().unwrap().current_sub_point_index, 0);
    }

    #[test]
    fn stop_session_when_not_active_is_noop() {
        let ms = make_state();
        assert!(!ms.inner.lock().unwrap().session_active);
        // Simulating stop logic without AppHandle
        let pipeline_opt = {
            let mut s = ms.inner.lock().unwrap();
            s.session_active = false;
            s.pipeline.take()
        };
        assert!(pipeline_opt.is_none());
        assert!(!ms.inner.lock().unwrap().session_active);
    }

    #[test]
    fn display_mode_cycles_through_all_states() {
        let ms = make_state();
        for mode in [
            DisplayMode::Verse,
            DisplayMode::Title,
            DisplayMode::Subpoint,
            DisplayMode::Blank,
            DisplayMode::Hymn,
            DisplayMode::Idle,
        ] {
            ms.inner.lock().unwrap().display_mode = mode.clone();
            assert_eq!(ms.inner.lock().unwrap().display_mode, mode);
        }
    }

    #[test]
    fn last_verse_stored_and_retrieved() {
        let ms = make_state();
        ms.inner.lock().unwrap().last_verse =
            Some(("John 3:16".into(), "For God so loved…".into()));
        let got = ms.inner.lock().unwrap().last_verse.clone();
        assert_eq!(got.as_ref().map(|(r, _)| r.as_str()), Some("John 3:16"));
    }

    // ── resolve_bible_path (dev mode only) ────────────────────────────────────

    #[test]
    fn bible_path_exists_in_dev() {
        let path = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("../../../packages/bible/src/data/kjv.json");
        let canonical = path.canonicalize();
        assert!(
            canonical.is_ok(),
            "kjv.json not found at expected dev path: {}",
            path.display()
        );
    }
}
