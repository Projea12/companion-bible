import { useCallback, useEffect, useState } from 'react';
import { invoke } from '@tauri-apps/api/core';
import { listen } from '@tauri-apps/api/event';
import type { AppEvent, AppState } from '@companion-bible/types';
import { TranscriptPanel } from './TranscriptPanel';
import { useTranscript } from './useTranscript';

// ── types ─────────────────────────────────────────────────────────────────────

interface PendingDetection {
  label: string;
  confidence: number;
}

interface DisplayedVerse {
  reference: string;
  text: string;
  translation: string;
}

type HealthStatus = 'idle' | 'active' | 'error';

// ── root component ────────────────────────────────────────────────────────────

export function App() {
  // session
  const [sessionActive, setSessionActive] = useState(false);
  const [congregationVisible, setCongregationVisible] = useState(false);
  const [totalScreens, setTotalScreens] = useState(1);
  const [hasSecondary, setHasSecondary] = useState(false);

  // sermon context
  const [sermonTitle, setSermonTitle] = useState<string | null>(null);
  const [currentSubPoint, setCurrentSubPoint] = useState<string | null>(null);

  // live transcript
  const transcript = useTranscript();

  // detection
  const [pendingDetection, setPendingDetection] = useState<PendingDetection | null>(null);

  // display
  const [displayedVerse, setDisplayedVerse] = useState<DisplayedVerse | null>(null);

  // undo (5-second window enforced on the frontend)
  const [undoExpiresAt, setUndoExpiresAt] = useState<number | null>(null);
  const [undoSecsLeft, setUndoSecsLeft] = useState(0);

  // status
  const [internet, setInternet] = useState<'online' | 'offline'>('offline');
  const [audioStatus, setAudioStatus] = useState<HealthStatus>('idle');
  const [aiStatus, setAiStatus] = useState<HealthStatus>('idle');

  // manual override
  const [overrideInput, setOverrideInput] = useState('');

  // ── startup ────────────────────────────────────────────────────────────────

  useEffect(() => {
    void invoke<AppState>('get_app_state').then((s) => {
      setSessionActive(s.sessionActive);
      setCongregationVisible(s.congregationVisible);
      setTotalScreens(s.totalScreens);
      setHasSecondary(s.hasSecondaryScreen);
      if (s.sessionActive) setAudioStatus('active');
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
          setPendingDetection({ label, confidence: 85 });
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

        case 'VERSE_DISPLAYED':
          setPendingDetection(null);
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

        case 'INTERNET_CONNECTED':
          setInternet('online');
          break;

        case 'INTERNET_DISCONNECTED':
          setInternet('offline');
          break;

        case 'AUDIO_CAPTURE_STARTED':
          setAudioStatus('active');
          break;

        case 'AUDIO_CAPTURE_STOPPED':
          setAudioStatus('idle');
          transcript.clear();
          break;

        case 'AI_QUERY_STARTED':
        case 'AI_RESPONSE_RECEIVED':
          setAiStatus('active');
          break;

        case 'AI_QUERY_FAILED':
          setAiStatus('error');
          break;

        case 'HEALTH_CHECK_PASSED':
          if (payload.component === 'ai') setAiStatus('active');
          break;

        case 'HEALTH_CHECK_FAILED':
          if (payload.component === 'ai') setAiStatus('error');
          if (payload.component === 'audio') setAudioStatus('error');
          break;
      }
    });

    return () => {
      void unlistenPromise.then((fn) => fn());
    };
  }, [transcript]);

  // ── actions ───────────────────────────────────────────────────────────────

  const handleStartSession = useCallback(() => {
    void invoke('start_session').then(() => {
      setSessionActive(true);
      setAudioStatus('active');
    });
  }, []);

  const handleStopSession = useCallback(() => {
    void invoke('stop_session').then(() => {
      setSessionActive(false);
      setAudioStatus('idle');
      setPendingDetection(null);
      transcript.clear();
    });
  }, [transcript]);

  const handleToggleCongregation = useCallback(() => {
    const cmd = congregationVisible ? 'hide_congregation_window' : 'show_congregation_window';
    void invoke(cmd).then(() => setCongregationVisible((v) => !v));
  }, [congregationVisible]);

  const handleApprove = useCallback(() => {
    if (!pendingDetection) return;
    void invoke('show_verse', { reference: pendingDetection.label, text: '' });
  }, [pendingDetection]);

  const handleReject = useCallback(() => {
    if (!pendingDetection) return;
    void invoke('reject_detection', { reference: pendingDetection.label }).then(() => {
      setPendingDetection(null);
    });
  }, [pendingDetection]);

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

  const handleManualOverride = useCallback(() => {
    const ref = overrideInput.trim();
    if (!ref) return;
    void invoke('show_verse', { reference: ref, text: '' }).then(() => {
      setOverrideInput('');
    });
  }, [overrideInput]);

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
    };
    window.addEventListener('keydown', onKey);
    return () => window.removeEventListener('keydown', onKey);
  }, [handleDiscard, handleUndo]);

  // ── render ────────────────────────────────────────────────────────────────

  const showUndo = undoExpiresAt !== null && undoSecsLeft > 0;

  return (
    <div className="op-layout">
      {/* ── Header ── */}
      <header className="op-header">
        <span className="op-brand">Companion Bible</span>
        <div className="op-header-controls">
          {sessionActive ? (
            <button className="btn btn-danger" onClick={handleStopSession}>
              Stop Session
            </button>
          ) : (
            <button className="btn btn-primary" onClick={handleStartSession}>
              Start Session
            </button>
          )}
          <button
            className="btn btn-secondary"
            disabled={!hasSecondary}
            onClick={handleToggleCongregation}
          >
            {congregationVisible ? 'Hide Congregation' : 'Show Congregation'}
          </button>
        </div>
      </header>

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
        {/* ── Left: transcript + detection ── */}
        <div className="op-col op-col-left">
          <section className="op-panel op-panel-transcript">
            <h2 className="op-panel-heading">Live Transcript</h2>
            <TranscriptPanel lines={transcript.lines} sessionActive={sessionActive} />
          </section>

          <section className="op-panel op-panel-detection">
            <h2 className="op-panel-heading">Detected Reference</h2>
            {pendingDetection ? (
              <div className="detection-card">
                <div className="detection-ref">{pendingDetection.label}</div>
                <div className="confidence-row">
                  <div className="confidence-track">
                    <div
                      className="confidence-fill"
                      style={{ width: `${pendingDetection.confidence}%` }}
                    />
                  </div>
                  <span className="confidence-pct">{pendingDetection.confidence}%</span>
                </div>
                <div className="detection-actions">
                  <button className="btn btn-approve" onClick={handleApprove}>
                    ✓ Approve &amp; Display
                  </button>
                  <button className="btn btn-reject" onClick={handleReject}>
                    ✗ Reject
                  </button>
                </div>
              </div>
            ) : (
              <p className="detection-empty">
                {sessionActive ? 'Listening for scripture references…' : 'Start a session to begin'}
              </p>
            )}
          </section>
        </div>

        {/* ── Right: displayed verse + discard + override + undo ── */}
        <div className="op-col op-col-right">
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

          <button
            className="btn-discard"
            disabled={!displayedVerse}
            onClick={handleDiscard}
            title="Keyboard: Space"
          >
            Discard
            <kbd className="key-hint">Space</kbd>
          </button>

          <section className="op-panel op-panel-override">
            <h2 className="op-panel-heading">Manual Override</h2>
            <div className="override-row">
              <input
                className="override-input"
                placeholder="e.g. John 3:16"
                value={overrideInput}
                onChange={(e) => setOverrideInput(e.target.value)}
                onKeyDown={(e) => {
                  if (e.key === 'Enter') {
                    e.preventDefault();
                    handleManualOverride();
                  }
                }}
                aria-label="Manual reference override"
              />
              <button
                className="btn btn-primary"
                disabled={!overrideInput.trim()}
                onClick={handleManualOverride}
                title="Keyboard: Enter"
              >
                Display
              </button>
            </div>
          </section>

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
      <footer className="op-statusbar">
        <div className="statusbar-left">
          <StatusBadge label="Audio" status={sessionActive ? audioStatus : 'idle'} />
          <StatusBadge label="AI" status={aiStatus} />
        </div>
        <div className="statusbar-center">
          <span className="status-screens" data-dual={String(hasSecondary)}>
            {totalScreens} screen{totalScreens !== 1 ? 's' : ''}
            {hasSecondary ? ' ✓' : ''}
          </span>
          <span className="status-internet" data-online={String(internet === 'online')}>
            {internet === 'online' ? 'Online' : 'Offline'}
          </span>
        </div>
        <div className="statusbar-right">
          <kbd>Space</kbd>
          <span>Discard</span>
          <kbd>Ctrl+Z</kbd>
          <span>Undo</span>
          <kbd>Enter</kbd>
          <span>Override</span>
        </div>
      </footer>
    </div>
  );
}

// ── helpers ───────────────────────────────────────────────────────────────────

function formatRef(book: string, chapter: number, verse: number | null | undefined): string {
  return verse != null ? `${book} ${chapter}:${verse}` : `${book} ${chapter}`;
}

function StatusBadge({ label, status }: { label: string; status: HealthStatus }) {
  return (
    <div className="status-badge" data-status={status}>
      <span className="status-dot" />
      <span className="status-label">{label}</span>
    </div>
  );
}
