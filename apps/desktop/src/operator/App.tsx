import { useCallback, useEffect, useState } from 'react';
import { invoke } from '@tauri-apps/api/core';
import { listen } from '@tauri-apps/api/event';
import type { AppEvent, AppState } from '@companion-bible/types';
import { TranscriptPanel } from './TranscriptPanel';
import { VerseQueuePanel } from './VerseQueuePanel';
import { ManualOverride } from './ManualOverride';
import { SermonControls } from './SermonControls';
import type { SermonSetup } from './SermonControls';
import { StatusBar } from './StatusBar';
import type { AudioStatus, InternetStatus, AiStatus, StorageStatus } from './StatusBar';
import { useTranscript } from './useTranscript';
import { useVerseQueue } from './useVerseQueue';

// ── types ─────────────────────────────────────────────────────────────────────

interface DisplayedVerse {
  reference: string;
  text: string;
  translation: string;
}

// ── root component ────────────────────────────────────────────────────────────

export function App() {
  // session
  const [sessionActive, setSessionActive] = useState(false);
  const [sessionStarting, setSessionStarting] = useState(false);
  const [sessionError, setSessionError] = useState<string | null>(null);
  const [modelDownloadPercent, setModelDownloadPercent] = useState<number | null>(null);
  const [congregationVisible, setCongregationVisible] = useState(false);
  const [totalScreens, setTotalScreens] = useState(1);
  const [hasSecondary, setHasSecondary] = useState(false);

  // sermon context
  const [sermonActive, setSermonActive] = useState(false);
  const [sermonTitle, setSermonTitle] = useState<string | null>(null);
  const [currentSubPoint, setCurrentSubPoint] = useState<string | null>(null);
  const [subPoints, setSubPoints] = useState<string[]>([]);
  const [subPointIndex, setSubPointIndex] = useState(-1);

  // live transcript
  const transcript = useTranscript();

  // verse queue
  const queue = useVerseQueue();

  // display
  const [displayedVerse, setDisplayedVerse] = useState<DisplayedVerse | null>(null);

  // transcription
  const [assemblyaiKey, setAssemblyaiKey] = useState('');
  const [deepgramKey, setDeepgramKey] = useState('');
  const [openaiKey, setOpenaiKey] = useState('');
  const [transcriptionMode, setTranscriptionMode] = useState<'assemblyai' | 'deepgram' | 'whisper'>(
    'whisper',
  );

  // settings panel
  const [settingsOpen, setSettingsOpen] = useState(false);

  // undo (5-second window enforced on the frontend)
  const [undoExpiresAt, setUndoExpiresAt] = useState<number | null>(null);
  const [undoSecsLeft, setUndoSecsLeft] = useState(0);

  // status
  const [internet, setInternet] = useState<InternetStatus>('offline');
  const [audio, setAudio] = useState<AudioStatus>('idle');
  const [ai, setAi] = useState<AiStatus>('idle');
  const [storage, setStorage] = useState<StorageStatus>('ample');

  // ── startup ────────────────────────────────────────────────────────────────

  useEffect(() => {
    void invoke<AppState>('get_app_state').then((s) => {
      setSessionActive(s.sessionActive);
      setCongregationVisible(s.congregationVisible);
      setTotalScreens(s.totalScreens);
      setHasSecondary(s.hasSecondaryScreen);
      if (s.sessionActive) setAudio('flowing');
    });
  }, []);

  // ── undo countdown ─────────────────────────────────────────────────────────

  useEffect(() => {
    if (!undoExpiresAt) return;
    const tick = () => {
      const left = Math.ceil((undoExpiresAt - Date.now()) / 1000);
      if (left <= 0) {
        setUndoExpiresAt(null);
        setUndoSecsLeft(0);
      } else {
        setUndoSecsLeft(left);
      }
    };
    tick();
    const id = setInterval(tick, 200);
    return () => clearInterval(id);
  }, [undoExpiresAt]);

  // ── Tauri event listener ──────────────────────────────────────────────────

  useEffect(() => {
    const unlistenPromise = listen<AppEvent>('app-event', ({ payload }) => {
      switch (payload.type) {
        case 'MODEL_DOWNLOAD_PROGRESS':
          setModelDownloadPercent((payload as unknown as { percent: number }).percent);
          break;

        case 'SECONDARY_SCREEN_CONNECTED':
        case 'SECONDARY_SCREEN_DISCONNECTED':
          void invoke<AppState>('get_app_state').then((s) => {
            setTotalScreens(s.totalScreens);
            setHasSecondary(s.hasSecondaryScreen);
          });
          break;

        case 'TRANSCRIPTION_COMPLETED':
          transcript.addLine(payload.chunk_id, payload.text);
          break;

        case 'SCRIPTURE_REFERENCE_DETECTED': {
          const ref = payload.references[0];
          if (!ref) break;
          const label = formatRef(ref.book, ref.chapter, ref.verse);
          queue.enqueue(label, 85);
          transcript.markDetection(payload.source_text, label);
          break;
        }

        case 'VERSE_LOADED':
          setDisplayedVerse({
            reference: formatRef(
              payload.reference.book,
              payload.reference.chapter,
              payload.reference.verse,
            ),
            text: payload.text,
            translation: payload.translation,
          });
          break;

        case 'DISPLAY_CLEARED':
        case 'DISPLAY_BLANKED':
          setDisplayedVerse(null);
          break;

        case 'SERMON_TITLE_SHOWN':
          setSermonTitle(payload.title);
          break;

        case 'SUB_POINT_SHOWN':
          setCurrentSubPoint(payload.text);
          break;

        case 'SERMON_STARTED':
          setSermonActive(true);
          if (payload.title) setSermonTitle(payload.title);
          break;

        case 'SERMON_ENDED':
          setSermonActive(false);
          setSermonTitle(null);
          setCurrentSubPoint(null);
          setSubPoints([]);
          setSubPointIndex(-1);
          break;

        case 'SUB_POINT_ADDED':
          setSubPoints((prev) => (prev.includes(payload.text) ? prev : [...prev, payload.text]));
          break;

        case 'INTERNET_CONNECTED':
          setInternet('online');
          break;

        case 'INTERNET_DISCONNECTED':
          setInternet('offline');
          break;

        case 'TRANSCRIPTION_MODE_CHANGED': {
          const mode = (payload as unknown as { mode: string }).mode;
          setTranscriptionMode(
            mode === 'assemblyai' ? 'assemblyai' : mode === 'deepgram' ? 'deepgram' : 'whisper',
          );
          break;
        }

        case 'AUDIO_CAPTURE_STARTED':
          setAudio('flowing');
          break;

        case 'AUDIO_CAPTURE_STOPPED':
          setAudio('idle');
          transcript.clear();
          queue.clear();
          break;

        case 'AUDIO_QUALITY_DEGRADED':
          setAudio('degraded');
          break;

        case 'AI_QUERY_STARTED':
        case 'AI_RESPONSE_RECEIVED':
          setAi('all-layers');
          break;

        case 'AI_QUERY_FAILED':
          setAi('pattern-only');
          break;

        case 'AI_LAYERS_CHANGED': {
          const layerMap: Record<typeof payload.layers, AiStatus> = {
            all: 'all-layers',
            'local-only': 'local-only',
            'pattern-only': 'pattern-only',
          };
          setAi(layerMap[payload.layers]);
          break;
        }

        case 'STORAGE_STATUS':
          setStorage(payload.level);
          break;

        case 'HEALTH_CHECK_PASSED':
          if (payload.component === 'ai') setAi('all-layers');
          break;

        case 'HEALTH_CHECK_FAILED':
          if (payload.component === 'ai') setAi('pattern-only');
          if (payload.component === 'audio') setAudio('lost');
          break;
      }
    });

    return () => {
      void unlistenPromise.then((fn) => fn());
    };
  }, [transcript, queue]);

  // ── actions ───────────────────────────────────────────────────────────────

  const handleStartSession = useCallback(() => {
    setSessionStarting(true);
    setSessionError(null);
    invoke('start_session')
      .then(() => {
        setSessionActive(true);
        setAudio('flowing');
        setModelDownloadPercent(null);
      })
      .catch((err: unknown) => {
        setSessionError(String(err));
      })
      .finally(() => {
        setSessionStarting(false);
      });
  }, []);

  const handleStopSession = useCallback(() => {
    void invoke('stop_session').then(() => {
      setSessionActive(false);
      setAudio('idle');
      transcript.clear();
      queue.clear();
    });
  }, [transcript, queue]);

  const handleToggleCongregation = useCallback(() => {
    const cmd = congregationVisible ? 'hide_congregation_window' : 'show_congregation_window';
    void invoke(cmd).then(() => setCongregationVisible((v) => !v));
  }, [congregationVisible]);

  const handleConfirmVerse = useCallback(
    (id: number, label: string) => {
      void invoke('show_verse', { reference: label, text: '' }).then(() => {
        queue.remove(id);
      });
    },
    [queue],
  );

  const handleRejectVerse = useCallback(
    (id: number, label: string) => {
      void invoke('reject_detection', { reference: label }).then(() => {
        queue.remove(id);
      });
    },
    [queue],
  );

  const handleDiscard = useCallback(() => {
    if (!displayedVerse) return;
    void invoke('discard_verse').then(() => {
      setUndoExpiresAt(Date.now() + 5000);
      setUndoSecsLeft(5);
    });
  }, [displayedVerse]);

  const handleUndo = useCallback(() => {
    if (!undoExpiresAt || Date.now() > undoExpiresAt) return;
    void invoke('undo_discard').then(() => {
      setUndoExpiresAt(null);
      setUndoSecsLeft(0);
    });
  }, [undoExpiresAt]);

  const handleManualOverride = useCallback((ref: string) => {
    void invoke('show_verse', { reference: ref, text: '' });
  }, []);

  const handleSaveAssemblyAiKey = useCallback(() => {
    void invoke('set_assemblyai_key', { key: assemblyaiKey });
  }, [assemblyaiKey]);

  const handleSaveDeepgramKey = useCallback(() => {
    void invoke('set_deepgram_key', { key: deepgramKey });
  }, [deepgramKey]);

  const handleSaveOpenAiKey = useCallback(() => {
    void invoke('set_openai_key', { key: openaiKey });
  }, [openaiKey]);

  const handleNextVerse = useCallback(() => {
    void invoke('next_verse');
  }, []);

  const handlePrevVerse = useCallback(() => {
    void invoke('previous_verse');
  }, []);

  const handleStartService = useCallback((setup: SermonSetup) => {
    void invoke('start_sermon', {
      title: setup.title || null,
      pastor: setup.pastor || null,
      anchorScripture: setup.anchorScripture || null,
    }).then(() => {
      setSermonActive(true);
      if (setup.title) setSermonTitle(setup.title);
    });
  }, []);

  const handleEndService = useCallback(() => {
    void invoke('end_sermon').then(() => {
      setSermonActive(false);
      setSermonTitle(null);
      setCurrentSubPoint(null);
      setSubPoints([]);
      setSubPointIndex(-1);
    });
  }, []);

  const handleAddSubPoint = useCallback((text: string) => {
    void invoke('add_sub_point', { text }).then(() => {
      setSubPoints((prev) => [...prev, text]);
    });
  }, []);

  const handleNextSubPoint = useCallback(() => {
    void invoke('next_sub_point').then(() => {
      setSubPointIndex((i) => {
        const next = i + 1;
        setCurrentSubPoint(subPoints[next] ?? null);
        return next;
      });
    });
  }, [subPoints]);

  // ── keyboard shortcuts ────────────────────────────────────────────────────

  useEffect(() => {
    const onKey = (e: KeyboardEvent) => {
      const inInput = (e.target as HTMLElement).tagName === 'INPUT';
      if (e.code === 'Space' && !inInput) {
        e.preventDefault();
        handleDiscard();
      }
      if (e.ctrlKey && e.key === 'z' && !inInput) {
        e.preventDefault();
        handleUndo();
      }
      if (e.code === 'ArrowRight' && !inInput) {
        e.preventDefault();
        handleNextVerse();
      }
      if (e.code === 'ArrowLeft' && !inInput) {
        e.preventDefault();
        handlePrevVerse();
      }
    };
    window.addEventListener('keydown', onKey);
    return () => window.removeEventListener('keydown', onKey);
  }, [handleDiscard, handleUndo, handleNextVerse, handlePrevVerse]);

  // ── render ────────────────────────────────────────────────────────────────

  const showUndo = undoExpiresAt !== null && undoSecsLeft > 0;

  return (
    <div className="op-layout">
      {/* ── Header ── */}
      <header className="op-header">
        <div className="op-header-left">
          <span className="op-brand">Companion Bible</span>
          {sessionActive && (
            <span className={`mode-pill mode-pill--${transcriptionMode}`}>
              {transcriptionMode === 'assemblyai'
                ? 'AssemblyAI'
                : transcriptionMode === 'deepgram'
                  ? 'Deepgram'
                  : 'Whisper'}
            </span>
          )}
        </div>
        <div className="op-header-controls">
          {sessionError && <span className="session-error">{sessionError}</span>}
          {sessionActive ? (
            <button className="btn btn-danger" onClick={handleStopSession}>
              Stop Session
            </button>
          ) : (
            <button
              className="btn btn-primary"
              onClick={handleStartSession}
              disabled={sessionStarting}
            >
              {sessionStarting
                ? modelDownloadPercent !== null
                  ? `Downloading… ${modelDownloadPercent}%`
                  : 'Starting…'
                : 'Start Session'}
            </button>
          )}
          <button
            className="btn btn-secondary"
            disabled={!hasSecondary}
            onClick={handleToggleCongregation}
            title={hasSecondary ? undefined : 'No secondary screen detected'}
          >
            {congregationVisible ? 'Hide Screen' : 'Show Screen'}
          </button>
          {!sessionActive && (
            <button
              className={`btn btn-secondary btn-icon${settingsOpen ? ' btn-icon--active' : ''}`}
              onClick={() => setSettingsOpen((o) => !o)}
              title="API key settings"
              aria-expanded={settingsOpen}
            >
              ⚙
            </button>
          )}
        </div>
      </header>

      {/* ── Settings panel (API keys) ── */}
      {settingsOpen && !sessionActive && (
        <div className="op-settings-panel">
          <div className="settings-panel-inner">
            <p className="settings-hint">
              Transcription: AssemblyAI → Deepgram → Whisper (offline). Detection: OpenAI (primary).
            </p>
            <div className="settings-key-grid">
              <div className="settings-key-row">
                <div className="settings-key-meta">
                  <span className="settings-key-label">AssemblyAI</span>
                  <span className="settings-key-tag settings-key-tag--primary">Recommended</span>
                </div>
                <div className="settings-key-input-row">
                  <input
                    className="settings-input"
                    type="password"
                    placeholder="aai-…"
                    value={assemblyaiKey}
                    onChange={(e) => setAssemblyaiKey(e.target.value)}
                    onBlur={handleSaveAssemblyAiKey}
                    autoComplete="off"
                  />
                  <button className="btn btn-secondary btn-sm" onClick={handleSaveAssemblyAiKey}>
                    Save
                  </button>
                </div>
                {assemblyaiKey && <span className="settings-key-saved">✓ Saved</span>}
              </div>

              <div className="settings-key-divider" />

              <div className="settings-key-row">
                <div className="settings-key-meta">
                  <span className="settings-key-label">Deepgram</span>
                  <span className="settings-key-tag">Fallback</span>
                </div>
                <div className="settings-key-input-row">
                  <input
                    className="settings-input"
                    type="password"
                    placeholder="Paste Deepgram key…"
                    value={deepgramKey}
                    onChange={(e) => setDeepgramKey(e.target.value)}
                    onBlur={handleSaveDeepgramKey}
                    autoComplete="off"
                  />
                  <button className="btn btn-secondary btn-sm" onClick={handleSaveDeepgramKey}>
                    Save
                  </button>
                </div>
                {deepgramKey && <span className="settings-key-saved">✓ Saved</span>}
              </div>

              <div className="settings-key-divider" />

              <div className="settings-key-row">
                <div className="settings-key-meta">
                  <span className="settings-key-label">OpenAI</span>
                  <span className="settings-key-tag settings-key-tag--primary">
                    Verse Detection
                  </span>
                </div>
                <div className="settings-key-input-row">
                  <input
                    className="settings-input"
                    type="password"
                    placeholder="sk-…"
                    value={openaiKey}
                    onChange={(e) => setOpenaiKey(e.target.value)}
                    onBlur={handleSaveOpenAiKey}
                    autoComplete="off"
                  />
                  <button className="btn btn-secondary btn-sm" onClick={handleSaveOpenAiKey}>
                    Save
                  </button>
                </div>
                {openaiKey && <span className="settings-key-saved">✓ Saved</span>}
              </div>
            </div>
          </div>
        </div>
      )}

      {/* ── Sermon Bar ── */}
      <div className="op-sermon-bar">
        <div className="sermon-slot">
          <span className="sermon-slot-label">Sermon</span>
          <span className="sermon-slot-value sermon-title">{sermonTitle ?? '—'}</span>
        </div>
        <div className="sermon-divider" />
        <div className="sermon-slot">
          <span className="sermon-slot-label">Sub-point</span>
          <span className="sermon-slot-value sermon-subpoint">{currentSubPoint ?? '—'}</span>
        </div>
      </div>

      {/* ── Main ── */}
      <main className="op-main">
        {/* ── Left: transcript + queue ── */}
        <div className="op-col op-col-left">
          <section className="op-panel op-panel-transcript">
            <h2 className="op-panel-heading">Live Transcript</h2>
            <TranscriptPanel lines={transcript.lines} sessionActive={sessionActive} />
          </section>

          <VerseQueuePanel
            items={queue.items}
            sessionActive={sessionActive}
            onConfirm={handleConfirmVerse}
            onReject={handleRejectVerse}
          />
        </div>

        {/* ── Right: sermon controls + verse + discard + override + undo ── */}
        <div className="op-col op-col-right">
          <SermonControls
            sermonActive={sermonActive}
            subPoints={subPoints}
            subPointIndex={subPointIndex}
            onStartService={handleStartService}
            onEndService={handleEndService}
            onAddSubPoint={handleAddSubPoint}
            onNextSubPoint={handleNextSubPoint}
          />

          <section className="op-panel op-panel-verse">
            <h2 className="op-panel-heading">Currently Displayed</h2>
            {displayedVerse ? (
              <div className="verse-display">
                <div className="verse-display-ref">{displayedVerse.reference}</div>
                <p className="verse-display-text">
                  {displayedVerse.text || '(text not yet loaded)'}
                </p>
                <span className="verse-display-trans">{displayedVerse.translation}</span>
              </div>
            ) : (
              <p className="verse-display-empty">Nothing on screen</p>
            )}
          </section>

          <div className="verse-nav-row">
            <button
              className="btn btn-secondary"
              disabled={!displayedVerse}
              onClick={handlePrevVerse}
              title="Previous verse — Keyboard: ←"
            >
              ← Prev
              <kbd className="key-hint">←</kbd>
            </button>
            <button
              className="btn btn-secondary"
              disabled={!displayedVerse}
              onClick={handleNextVerse}
              title="Next verse — Keyboard: →"
            >
              Next →<kbd className="key-hint">→</kbd>
            </button>
          </div>

          <button
            className="btn-discard"
            disabled={!displayedVerse}
            onClick={handleDiscard}
            title="Keyboard: Space"
          >
            Discard
            <kbd className="key-hint">Space</kbd>
          </button>

          <ManualOverride onSubmit={handleManualOverride} />

          {showUndo && (
            <button className="btn-undo" onClick={handleUndo} title="Keyboard: Ctrl+Z">
              ↩ Undo discard
              <span className="undo-timer">{undoSecsLeft}s</span>
              <kbd className="key-hint">Ctrl+Z</kbd>
            </button>
          )}
        </div>
      </main>

      {/* ── Status Bar ── */}
      <StatusBar
        sessionActive={sessionActive}
        audio={audio}
        internet={internet}
        ai={ai}
        storage={storage}
        totalScreens={totalScreens}
        hasSecondary={hasSecondary}
      />
    </div>
  );
}

// ── helpers ───────────────────────────────────────────────────────────────────

function formatRef(book: string, chapter: number, verse: number | null | undefined): string {
  return verse != null ? `${book} ${chapter}:${verse}` : `${book} ${chapter}`;
}
