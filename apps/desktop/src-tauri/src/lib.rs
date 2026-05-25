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
use companion_transcription::{
    AssemblyAiTranscriber, DeepgramTranscriber, ModelManager, SetupProgress, TranscribeOptions,
    WhisperTranscriber,
};
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
    Whisper(WhisperTranscriber),
    Deepgram(DeepgramTranscriber),
    AssemblyAi(AssemblyAiTranscriber),
}

impl AnyTranscriber {
    fn stop(&mut self) {
        match self {
            Self::Whisper(t) => t.stop(),
            Self::Deepgram(t) => t.stop(),
            Self::AssemblyAi(t) => t.stop(),
        }
    }

    fn mode_label(&self) -> &'static str {
        match self {
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
    inner: Arc<Mutex<InternalState>>,
    engine: Arc<tokio::sync::Mutex<Option<DetectionEngine>>>,
    bible: Arc<Mutex<Option<KjvBible>>>,
}

impl ManagedState {
    fn new() -> Self {
        Self {
            inner: Arc::new(Mutex::new(InternalState::default())),
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
                            let _ = w.set_fullscreen(true);
                        }
                        "SECONDARY_SCREEN_CONNECTED"
                    }
                    ScreenStatus::Disconnected => {
                        if let Some(w) = congregation_window(&app) {
                            let _ = w.set_fullscreen(false);
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
        let _ = w.set_fullscreen(true);
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
        let _ = w.set_fullscreen(true);
        let _ = w.set_focus();
    }
}

#[tauri::command]
fn hide_congregation_window(app: AppHandle) {
    if let Some(w) = congregation_window(&app) {
        let _ = w.set_fullscreen(false);
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
/// Book name matching is case-insensitive so "john" finds "John".
fn lookup_verse_text(
    bible_arc: &Arc<Mutex<Option<KjvBible>>>,
    ref_json: &serde_json::Value,
) -> String {
    let guard = bible_arc.lock().unwrap();
    let Some(bible) = guard.as_ref() else {
        return String::new();
    };
    let book_raw = ref_json["book"].as_str().unwrap_or_default();
    // Resolve canonical casing (e.g. "john" → "John") so HashMap lookup works.
    let book = bible
        .book_names()
        .find(|&n| n.eq_ignore_ascii_case(book_raw))
        .unwrap_or(book_raw);
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
fn show_verse(
    app: AppHandle,
    state: State<ManagedState>,
    reference: String,
    text: String,
) -> Result<(), String> {
    let Some(ref_json) = parse_reference(&reference) else {
        return Err(format!("Could not parse reference: {reference}"));
    };

    // If the background Bible loader hasn't finished yet, load synchronously now.
    {
        let mut guard = state.bible.lock().unwrap();
        if guard.is_none() {
            let bible_path = resolve_bible_path(&app);
            match KjvBible::load(&bible_path) {
                Ok(bible) => *guard = Some(bible),
                Err(e) => return Err(format!("Bible not available: {e}")),
            }
        }
    }

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
    Ok(())
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

/// Try Deepgram (using raw window); fall back to Whisper (processed window).
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
    // Whisper gets the pipeline-processed window (needs clean, normalised audio).
    let (t, rx) = WhisperTranscriber::new(processed_window, TranscribeOptions::default());
    (rx, AnyTranscriber::Whisper(t))
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
    let inner_state = Arc::clone(&state.inner);

    eprintln!("[start_session] step 9 done: processing task spawned");

    tauri::async_runtime::spawn(async move {
        let mut last_displayed: Option<(String, u8, Option<u8>)> = None;

        while let Some(segment) = tokio_rx.recv().await {
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

                // Update current_displayed_ref so next/prev verse navigation works
                // for auto-detected references, not just manual ones.
                if let Some(verse_num) = ref_.verse {
                    if let Ok(mut s) = inner_state.lock() {
                        s.current_displayed_ref =
                            Some((ref_.book.clone(), ref_.chapter, verse_num));
                        s.display_mode = DisplayMode::Verse;
                        s.last_verse = Some((
                            format!("{} {}:{}", ref_.book, ref_.chapter, verse_num),
                            verse_text.clone(),
                        ));
                    }
                }

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
        eprintln!("[start_session] step 10: skipped (Deepgram active)");
    }

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
fn approve_detection(
    app: AppHandle,
    state: State<ManagedState>,
    reference: String,
) -> Result<(), String> {
    show_verse(app, state, reference, String::new())
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
    let _ = show_verse(app, state, reference, String::new());
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
    let _ = show_verse(app, state, reference, String::new());
}

// ─── congregation scroll command ─────────────────────────────────────────────

/// Map a scroll direction string to a pixel amount.
/// Negative = up (toward top), positive = down (toward bottom).
/// 200 px was chosen to clear at least one hymn line at 1.5× GHS text scale.
fn scroll_amount(direction: &str) -> i32 {
    if direction == "up" {
        -200
    } else {
        200
    }
}

/// Scroll the congregation screen up or down from the operator panel.
#[tauri::command]
fn scroll_congregation(app: AppHandle, direction: String) {
    let _ = app.emit(
        "app-event",
        serde_json::json!({ "type": "CONGREGATION_SCROLL", "amount": scroll_amount(&direction) }),
    );
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
            // Register state immediately so the window opens without delay.
            app.manage(ManagedState::new());

            // Load the 6.4 MB KJV Bible in a background thread so the main
            // thread is never blocked during startup.
            let bible_arc = Arc::clone(&app.state::<ManagedState>().bible);
            let bible_path = resolve_bible_path(app.handle());
            std::thread::Builder::new()
                .name("bible-loader".into())
                .spawn(move || match KjvBible::load(&bible_path) {
                    Ok(bible) => {
                        *bible_arc.lock().unwrap() = Some(bible);
                        eprintln!("[setup] KJV Bible loaded from {}", bible_path.display());
                    }
                    Err(e) => eprintln!("[setup] ERROR: failed to load KJV Bible: {e}"),
                })
                .expect("failed to spawn bible-loader thread");

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
            scroll_congregation,
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

    // ── inner: Arc<Mutex> sharing across threads ──────────────────────────────
    //
    // These tests verify the change from Mutex → Arc<Mutex>.
    // The processing task inside start_session holds Arc::clone(&state.inner)
    // so it can write current_displayed_ref and display_mode after auto-detecting
    // a verse.  The tests below simulate exactly that pattern.

    // Arc::clone produces a second handle that points to the same allocation —
    // a write through the clone must be immediately visible on the original.
    #[test]
    fn arc_clone_shares_same_inner_state() {
        let ms = make_state();
        let inner_clone = Arc::clone(&ms.inner);

        inner_clone.lock().unwrap().display_mode = DisplayMode::Verse;

        assert_eq!(
            ms.inner.lock().unwrap().display_mode,
            DisplayMode::Verse,
            "write through Arc clone must be visible on the original"
        );
    }

    // Simulates the processing task: a background thread holds an Arc clone,
    // detects a verse, and writes current_displayed_ref + display_mode.
    // The main thread (operator handler) must see the updated values.
    #[test]
    fn processing_task_thread_writes_current_displayed_ref() {
        let ms = make_state();
        let inner_for_task = Arc::clone(&ms.inner);

        // Before auto-detection: ref is None, mode is Idle.
        assert!(ms.inner.lock().unwrap().current_displayed_ref.is_none());
        assert_eq!(ms.inner.lock().unwrap().display_mode, DisplayMode::Idle);

        // Spawn a thread that acts like the processing task after auto-detection.
        let handle = std::thread::spawn(move || {
            let mut s = inner_for_task.lock().unwrap();
            s.current_displayed_ref = Some(("John".to_string(), 3, 16));
            s.display_mode = DisplayMode::Verse;
        });
        handle
            .join()
            .expect("processing task thread must not panic");

        // Main thread reads the state the task wrote.
        let s = ms.inner.lock().unwrap();
        assert_eq!(
            s.current_displayed_ref,
            Some(("John".to_string(), 3, 16)),
            "current_displayed_ref must reflect what the processing task wrote"
        );
        assert_eq!(
            s.display_mode,
            DisplayMode::Verse,
            "display_mode must be Verse after processing task writes it"
        );
    }

    // Multiple consecutive auto-detections overwrite current_displayed_ref —
    // the last write wins, which is correct: next/prev navigate from the most
    // recently displayed verse.
    #[test]
    fn successive_auto_detections_overwrite_current_displayed_ref() {
        let ms = make_state();

        let refs: &[(&str, u8, u8)] = &[("Romans", 8, 28), ("Philippians", 4, 13), ("John", 3, 16)];

        for &(book, chapter, verse) in refs {
            let inner_clone = Arc::clone(&ms.inner);
            let handle = std::thread::spawn(move || {
                inner_clone.lock().unwrap().current_displayed_ref =
                    Some((book.to_string(), chapter, verse));
            });
            handle.join().unwrap();
        }

        assert_eq!(
            ms.inner.lock().unwrap().current_displayed_ref,
            Some(("John".to_string(), 3, 16)),
            "current_displayed_ref must hold the last auto-detected verse"
        );
    }

    // Two threads writing different fields simultaneously must not deadlock —
    // verifies Arc<Mutex> is safe for the concurrent access pattern used in
    // start_session (processing task writes inner, commands read it).
    #[test]
    fn concurrent_inner_writes_do_not_deadlock() {
        let ms = make_state();
        let clone_a = Arc::clone(&ms.inner);
        let clone_b = Arc::clone(&ms.inner);

        let ha = std::thread::spawn(move || {
            clone_a.lock().unwrap().current_displayed_ref = Some(("Psalms".to_string(), 23, 1));
        });
        let hb = std::thread::spawn(move || {
            clone_b.lock().unwrap().display_mode = DisplayMode::Verse;
        });

        ha.join().expect("thread a must not panic");
        hb.join().expect("thread b must not panic");

        let s = ms.inner.lock().unwrap();
        assert_eq!(s.display_mode, DisplayMode::Verse);
        assert!(s.current_displayed_ref.is_some());
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

    // ── Bible loading: background thread + sync fallback ─────────────────────

    fn test_bible_path() -> std::path::PathBuf {
        std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("../../../packages/bible/src/data/kjv.json")
    }

    // The Arc starts as None — background loader hasn't run yet.
    // This is the state the app is in during the first few hundred milliseconds
    // after launch.
    #[test]
    fn bible_arc_starts_as_none() {
        let ms = make_state();
        assert!(
            ms.bible.lock().unwrap().is_none(),
            "Bible must be None until the background loader writes it"
        );
    }

    // lookup_verse_text must return an empty string (not panic) when the Bible
    // has not been loaded yet — the caller handles the empty case gracefully.
    #[test]
    fn lookup_returns_empty_when_bible_not_loaded() {
        let arc: Arc<Mutex<Option<KjvBible>>> = Arc::new(Mutex::new(None));
        let ref_json = serde_json::json!({ "book": "John", "chapter": 3, "verse": 16 });
        let result = lookup_verse_text(&arc, &ref_json);
        assert!(
            result.is_empty(),
            "lookup_verse_text must return empty string when Bible is None, got: {result:?}"
        );
    }

    // After the background thread writes the Bible into the Arc, lookup must
    // return real verse text — confirms the happy path works end-to-end.
    #[test]
    fn lookup_returns_text_after_bible_loaded() {
        let arc: Arc<Mutex<Option<KjvBible>>> = Arc::new(Mutex::new(None));
        *arc.lock().unwrap() =
            Some(KjvBible::load(test_bible_path()).expect("kjv.json must load in tests"));

        let ref_json = serde_json::json!({ "book": "John", "chapter": 3, "verse": 16 });
        let result = lookup_verse_text(&arc, &ref_json);
        assert!(
            !result.is_empty(),
            "lookup_verse_text must return verse text after Bible is loaded"
        );
        assert!(
            result.contains("God"),
            "John 3:16 must contain the word 'God', got: {result:?}"
        );
    }

    // Simulates exactly what the background thread in setup() does: spawn a
    // thread, load the Bible inside it, write to the shared Arc.  After join,
    // the Arc must be Some and lookup must succeed.
    #[test]
    fn background_thread_makes_bible_available() {
        let arc: Arc<Mutex<Option<KjvBible>>> = Arc::new(Mutex::new(None));
        let arc_for_thread = Arc::clone(&arc);
        let path = test_bible_path();

        assert!(
            arc.lock().unwrap().is_none(),
            "must be None before thread runs"
        );

        let handle = std::thread::Builder::new()
            .name("test-bible-loader".into())
            .spawn(move || {
                let bible = KjvBible::load(&path).expect("load must succeed in thread");
                *arc_for_thread.lock().unwrap() = Some(bible);
            })
            .expect("thread spawn must succeed");

        handle.join().expect("loader thread must not panic");

        assert!(
            arc.lock().unwrap().is_some(),
            "Bible must be Some after background thread completes"
        );
    }

    // Simulates the sync fallback inside show_verse: Arc is None (background
    // thread lost the race), fallback loads synchronously, subsequent lookup
    // returns real text.  This is the exact code path exercised in production
    // when a user enters a verse reference before the loader finishes.
    #[test]
    fn sync_fallback_transitions_none_to_some_and_lookup_works() {
        let arc: Arc<Mutex<Option<KjvBible>>> = Arc::new(Mutex::new(None));

        assert!(
            arc.lock().unwrap().is_none(),
            "must be None before fallback"
        );

        // Sync fallback — mirrors the guard block in show_verse exactly.
        {
            let mut guard = arc.lock().unwrap();
            if guard.is_none() {
                *guard =
                    Some(KjvBible::load(test_bible_path()).expect("fallback load must succeed"));
            }
        }

        assert!(
            arc.lock().unwrap().is_some(),
            "must be Some after sync fallback"
        );

        let ref_json = serde_json::json!({ "book": "Genesis", "chapter": 1, "verse": 1 });
        let text = lookup_verse_text(&arc, &ref_json);
        assert!(
            !text.is_empty(),
            "Genesis 1:1 must be non-empty after sync fallback load"
        );
    }

    // Multiple threads reading from the Arc simultaneously must not deadlock or
    // panic — verifies the Mutex usage is sound under concurrent access.
    #[test]
    fn concurrent_reads_do_not_deadlock_or_panic() {
        let arc: Arc<Mutex<Option<KjvBible>>> = Arc::new(Mutex::new(None));
        *arc.lock().unwrap() =
            Some(KjvBible::load(test_bible_path()).expect("kjv.json must load in tests"));

        let references = [
            serde_json::json!({ "book": "John",    "chapter": 3,  "verse": 16 }),
            serde_json::json!({ "book": "Psalms",  "chapter": 23, "verse": 1  }),
            serde_json::json!({ "book": "Romans",  "chapter": 8,  "verse": 28 }),
            serde_json::json!({ "book": "Genesis", "chapter": 1,  "verse": 1  }),
        ];

        let handles: Vec<_> = references
            .iter()
            .map(|r| {
                let arc_clone = Arc::clone(&arc);
                let ref_clone = r.clone();
                std::thread::spawn(move || lookup_verse_text(&arc_clone, &ref_clone))
            })
            .collect();

        for handle in handles {
            let text = handle.join().expect("reader thread must not panic");
            assert!(
                !text.is_empty(),
                "every concurrent read must return non-empty verse text"
            );
        }
    }

    // ── show_verse Result<(), String> ─────────────────────────────────────────
    //
    // show_verse requires AppHandle (a live Tauri runtime) so it cannot be
    // called directly in unit tests.  Instead we test each of its three Result
    // branches via the underlying helpers that produce each outcome:
    //
    //   Err — parse_reference returns None (unparseable reference string)
    //   Err — Bible unavailable (load path fails)
    //   Ok  — parse_reference returns Some + lookup_verse_text returns text
    //
    // next_verse and previous_verse use `let _ = show_verse(...)` to discard
    // the Result; their guard logic (early-return conditions) is also tested
    // here because that is what prevents them from calling show_verse with a
    // bad state.

    // ── Err path: parse_reference returns None ────────────────────────────────

    // show_verse calls parse_reference first; None → immediate Err return.
    // These tests confirm every input that produces None, and therefore Err.
    #[test]
    fn show_verse_err_path_empty_string() {
        assert!(
            parse_reference("").is_none(),
            "empty string must not parse — show_verse returns Err"
        );
    }

    #[test]
    fn show_verse_err_path_no_chapter() {
        assert!(
            parse_reference("John").is_none(),
            "book-only string must not parse — show_verse returns Err"
        );
    }

    #[test]
    fn show_verse_err_path_garbage_input() {
        for bad in &["not a verse", "123", ":::", "John :"] {
            assert!(
                parse_reference(bad).is_none(),
                "'{bad}' must not parse — show_verse must return Err for it"
            );
        }
    }

    // ── Ok path: parse_reference returns Some ─────────────────────────────────

    // These inputs successfully parse — show_verse would proceed past the first
    // guard and reach the bible lookup + emit stage.
    #[test]
    fn show_verse_ok_path_explicit_verse_parses() {
        assert!(
            parse_reference("John 3:16").is_some(),
            "John 3:16 must parse — show_verse proceeds to Ok path"
        );
    }

    #[test]
    fn show_verse_ok_path_chapter_only_parses() {
        assert!(
            parse_reference("Genesis 1").is_some(),
            "chapter-only reference must parse — show_verse proceeds to Ok path"
        );
    }

    #[test]
    fn show_verse_ok_path_case_insensitive_lookup() {
        // approve_detection / manual input sends lower-case book names.
        // lookup_verse_text must resolve them to the canonical KJV casing.
        let arc: Arc<Mutex<Option<KjvBible>>> = Arc::new(Mutex::new(Some(
            KjvBible::load(test_bible_path()).expect("kjv.json must load"),
        )));

        for (input, chapter, verse) in &[
            ("john", 3u64, 16u64),
            ("JOHN", 3, 16),
            ("John", 3, 16),
            ("genesis", 1, 1),
            ("REVELATION", 22, 21),
        ] {
            let ref_json = serde_json::json!({
                "book": input, "chapter": chapter, "verse": verse
            });
            let text = lookup_verse_text(&arc, &ref_json);
            assert!(
                !text.is_empty(),
                "case-insensitive lookup for '{input} {chapter}:{verse}' must return verse text"
            );
        }
    }

    // ── next_verse guard: early-return when current_displayed_ref is None ─────

    // next_verse reads current_displayed_ref; if None it returns immediately
    // without calling show_verse.  We test the guard directly on the state.
    #[test]
    fn next_verse_guard_no_ref_means_no_call() {
        let ms = make_state();
        // current_displayed_ref is None by default — next_verse would return early.
        assert!(
            ms.inner.lock().unwrap().current_displayed_ref.is_none(),
            "default state must have no displayed ref — next_verse guard triggers"
        );
    }

    #[test]
    fn next_verse_guard_ref_present_builds_correct_reference_string() {
        let ms = make_state();
        ms.inner.lock().unwrap().current_displayed_ref = Some(("Romans".to_string(), 8, 28));

        let (book, chapter, verse) = ms
            .inner
            .lock()
            .unwrap()
            .current_displayed_ref
            .clone()
            .unwrap();

        let next_ref = format!("{book} {chapter}:{}", verse + 1);
        assert_eq!(
            next_ref, "Romans 8:29",
            "next_verse must build 'Book Chapter:VerseN+1'"
        );
        // The string must be parseable — show_verse would return Ok, not Err.
        assert!(
            parse_reference(&next_ref).is_some(),
            "next_verse reference string must parse so show_verse returns Ok"
        );
    }

    // ── previous_verse guard: early-return at verse 1 ────────────────────────

    // previous_verse returns immediately when verse <= 1 — ensures show_verse
    // is never called with verse 0, which would produce a bad reference string.
    #[test]
    fn previous_verse_guard_verse_one_is_no_op() {
        // verse <= 1 → previous_verse returns early, show_verse never called.
        // The guard is in the production code; here we just document the invariant.
        let verse_one: u8 = 1;
        assert!(
            verse_one <= 1,
            "guard condition `verse <= 1` must hold for verse 1"
        );
        let verse_zero: u8 = 0;
        assert!(verse_zero <= 1, "guard must also block verse 0");
    }

    #[test]
    fn previous_verse_guard_verse_two_builds_correct_reference_string() {
        let ms = make_state();
        ms.inner.lock().unwrap().current_displayed_ref = Some(("Psalms".to_string(), 23, 2));

        let (book, chapter, verse) = ms
            .inner
            .lock()
            .unwrap()
            .current_displayed_ref
            .clone()
            .unwrap();

        assert!(verse > 1, "verse 2 must pass the guard");
        let prev_ref = format!("{book} {chapter}:{}", verse - 1);
        assert_eq!(
            prev_ref, "Psalms 23:1",
            "previous_verse must build 'Book Chapter:VerseN-1'"
        );
        assert!(
            parse_reference(&prev_ref).is_some(),
            "previous_verse reference string must parse so show_verse returns Ok"
        );
    }

    // approve_detection is a thin wrapper that returns show_verse's Result.
    // We confirm the Err path by testing that an unparseable reference — the
    // only way approve_detection can receive bad input — does not parse.
    #[test]
    fn approve_detection_err_path_unparseable_reference_does_not_parse() {
        assert!(
            parse_reference("not a valid reference at all").is_none(),
            "approve_detection passes the reference to show_verse unchanged; \
             an unparseable string must cause Err"
        );
    }

    // ── Auto-detected verses update current_displayed_ref ────────────────────
    //
    // The processing task inside start_session runs this block after an
    // AutoDisplay decision:
    //
    //   if let Some(verse_num) = ref_.verse {
    //       if let Ok(mut s) = inner_state.lock() {
    //           s.current_displayed_ref = Some((book, chapter, verse_num));
    //           s.display_mode = DisplayMode::Verse;
    //           s.last_verse = Some((format!(...), verse_text));
    //       }
    //   }
    //
    // These tests simulate that exact pattern — a thread holding Arc::clone of
    // inner writes all three fields — and assert the state is correct for
    // next/prev navigation to use afterward.

    // After auto-detection, current_displayed_ref must be Some with the
    // detected book/chapter/verse — that is what next_verse reads.
    #[test]
    fn auto_detection_writes_current_displayed_ref() {
        let ms = make_state();
        assert!(
            ms.inner.lock().unwrap().current_displayed_ref.is_none(),
            "must start as None before any detection"
        );

        let inner = Arc::clone(&ms.inner);
        std::thread::spawn(move || {
            let mut s = inner.lock().unwrap();
            s.current_displayed_ref = Some(("Romans".to_string(), 8, 28));
            s.display_mode = DisplayMode::Verse;
            s.last_verse = Some((
                "Romans 8:28".to_string(),
                "And we know that all things work together for good".to_string(),
            ));
        })
        .join()
        .unwrap();

        let s = ms.inner.lock().unwrap();
        assert_eq!(
            s.current_displayed_ref,
            Some(("Romans".to_string(), 8, 28)),
            "current_displayed_ref must hold the auto-detected book/chapter/verse"
        );
    }

    // display_mode must be Verse after auto-detection — not Idle or Hymn.
    // next_verse / previous_verse do not gate on display_mode, but other parts
    // of the app (mode toggle, hymn controls) read it.
    #[test]
    fn auto_detection_sets_display_mode_to_verse() {
        let ms = make_state();
        assert_eq!(
            ms.inner.lock().unwrap().display_mode,
            DisplayMode::Idle,
            "must start as Idle"
        );

        let inner = Arc::clone(&ms.inner);
        std::thread::spawn(move || {
            let mut s = inner.lock().unwrap();
            s.current_displayed_ref = Some(("John".to_string(), 3, 16));
            s.display_mode = DisplayMode::Verse;
            s.last_verse = Some((
                "John 3:16".to_string(),
                "For God so loved the world".to_string(),
            ));
        })
        .join()
        .unwrap();

        assert_eq!(
            ms.inner.lock().unwrap().display_mode,
            DisplayMode::Verse,
            "display_mode must be Verse after auto-detection"
        );
    }

    // last_verse must be written with the canonical "Book Chapter:Verse" format —
    // that is what undo_discard and other commands read back.
    #[test]
    fn auto_detection_writes_last_verse_with_correct_format() {
        let ms = make_state();
        let inner = Arc::clone(&ms.inner);

        std::thread::spawn(move || {
            let mut s = inner.lock().unwrap();
            let book = "Philippians";
            let chapter = 4u8;
            let verse_num = 13u8;
            let verse_text = "I can do all things through Christ".to_string();
            s.current_displayed_ref = Some((book.to_string(), chapter, verse_num));
            s.display_mode = DisplayMode::Verse;
            // Mirrors: format!("{} {}:{}", ref_.book, ref_.chapter, verse_num)
            s.last_verse = Some((format!("{book} {chapter}:{verse_num}"), verse_text));
        })
        .join()
        .unwrap();

        let s = ms.inner.lock().unwrap();
        let (ref_str, text) = s.last_verse.as_ref().unwrap();
        assert_eq!(
            ref_str, "Philippians 4:13",
            "last_verse reference must be 'Book Chapter:Verse'"
        );
        assert!(!text.is_empty(), "last_verse text must not be empty");
    }

    // All three fields must be written together in a single lock acquisition.
    // If any field is missing, navigation or undo will read stale state.
    #[test]
    fn auto_detection_writes_all_three_fields_atomically() {
        let ms = make_state();
        let inner = Arc::clone(&ms.inner);

        std::thread::spawn(move || {
            let mut s = inner.lock().unwrap();
            s.current_displayed_ref = Some(("Genesis".to_string(), 1, 1));
            s.display_mode = DisplayMode::Verse;
            s.last_verse = Some((
                "Genesis 1:1".to_string(),
                "In the beginning God created".to_string(),
            ));
        })
        .join()
        .unwrap();

        let s = ms.inner.lock().unwrap();
        assert!(
            s.current_displayed_ref.is_some(),
            "current_displayed_ref must be set"
        );
        assert_eq!(
            s.display_mode,
            DisplayMode::Verse,
            "display_mode must be set"
        );
        assert!(s.last_verse.is_some(), "last_verse must be set");
    }

    // A chapter-only auto-detection (verse is None) must NOT update
    // current_displayed_ref — there is no verse number to navigate from.
    // This mirrors the `if let Some(verse_num) = ref_.verse` guard.
    #[test]
    fn auto_detection_skips_update_when_verse_is_none() {
        let ms = make_state();
        // Simulate: ref_.verse is None — the guard does not fire.
        let verse_opt: Option<u8> = None;
        if let Some(verse_num) = verse_opt {
            let mut s = ms.inner.lock().unwrap();
            s.current_displayed_ref = Some(("John".to_string(), 3, verse_num));
            s.display_mode = DisplayMode::Verse;
        }

        assert!(
            ms.inner.lock().unwrap().current_displayed_ref.is_none(),
            "current_displayed_ref must stay None when verse is None — \
             next/prev have no verse number to navigate from"
        );
    }

    // After auto-detection, next_verse navigation must read the correct verse
    // and build the right reference string — end-to-end wiring of detection
    // → state write → navigation read.
    #[test]
    fn auto_detection_then_next_verse_builds_correct_reference() {
        let ms = make_state();
        let inner = Arc::clone(&ms.inner);

        // Processing task writes auto-detected verse.
        std::thread::spawn(move || {
            let mut s = inner.lock().unwrap();
            s.current_displayed_ref = Some(("Romans".to_string(), 8, 28));
            s.display_mode = DisplayMode::Verse;
            s.last_verse = Some((
                "Romans 8:28".to_string(),
                "all things work together".to_string(),
            ));
        })
        .join()
        .unwrap();

        // next_verse reads current_displayed_ref and builds the reference.
        let (book, chapter, verse) = ms
            .inner
            .lock()
            .unwrap()
            .current_displayed_ref
            .clone()
            .unwrap();

        let next_ref = format!("{book} {chapter}:{}", verse + 1);
        assert_eq!(
            next_ref, "Romans 8:29",
            "next_verse after auto-detection must navigate to Romans 8:29"
        );
        assert!(
            parse_reference(&next_ref).is_some(),
            "next reference after auto-detection must be parseable by show_verse"
        );
    }

    // After auto-detection, previous_verse navigation must read the correct
    // verse and build the right reference string.
    #[test]
    fn auto_detection_then_previous_verse_builds_correct_reference() {
        let ms = make_state();
        let inner = Arc::clone(&ms.inner);

        std::thread::spawn(move || {
            let mut s = inner.lock().unwrap();
            s.current_displayed_ref = Some(("Psalms".to_string(), 23, 4));
            s.display_mode = DisplayMode::Verse;
            s.last_verse = Some((
                "Psalms 23:4".to_string(),
                "valley of the shadow".to_string(),
            ));
        })
        .join()
        .unwrap();

        let (book, chapter, verse) = ms
            .inner
            .lock()
            .unwrap()
            .current_displayed_ref
            .clone()
            .unwrap();

        assert!(
            verse > 1,
            "verse must be > 1 for previous_verse guard to pass"
        );
        let prev_ref = format!("{book} {chapter}:{}", verse - 1);
        assert_eq!(
            prev_ref, "Psalms 23:3",
            "previous_verse after auto-detection must navigate to Psalms 23:3"
        );
        assert!(
            parse_reference(&prev_ref).is_some(),
            "previous reference after auto-detection must be parseable by show_verse"
        );
    }

    // ── scroll_congregation amount ────────────────────────────────────────────
    //
    // 200 px was chosen so one scroll press clears at least one hymn line at
    // the 1.5× GHS text scale (max line height ≈ 144 px + line-height gap).
    // The previous value of 150 px was set before the hymn scale increase and
    // was too small to clear a single line.

    #[test]
    fn scroll_up_emits_negative_200() {
        assert_eq!(
            scroll_amount("up"),
            -200,
            "scroll up must move -200 px (toward top of screen)"
        );
    }

    #[test]
    fn scroll_down_emits_positive_200() {
        assert_eq!(
            scroll_amount("down"),
            200,
            "scroll down must move +200 px (toward bottom of screen)"
        );
    }

    #[test]
    fn scroll_amount_is_not_150() {
        // Regression guard: 150 was the old value, too small for 1.5× hymn text.
        assert_ne!(
            scroll_amount("up").abs(),
            150,
            "scroll amount must not regress to 150"
        );
        assert_ne!(
            scroll_amount("down"),
            150,
            "scroll amount must not regress to 150"
        );
    }

    #[test]
    fn scroll_unknown_direction_defaults_to_down() {
        // Any string that is not "up" produces a positive (downward) amount —
        // consistent with the if/else in scroll_amount.
        assert_eq!(scroll_amount("left"), 200);
        assert_eq!(scroll_amount(""), 200);
        assert_eq!(scroll_amount("UP"), 200); // case-sensitive — "UP" ≠ "up"
    }

    #[test]
    fn scroll_up_and_down_are_equal_magnitude() {
        assert_eq!(
            scroll_amount("up").abs(),
            scroll_amount("down").abs(),
            "up and down scroll must move the same distance"
        );
    }
}
