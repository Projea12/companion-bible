import { useCallback, useEffect, useState } from 'react';
import { invoke } from '@tauri-apps/api/core';
import { listen } from '@tauri-apps/api/event';
import type { AppEvent, AppState } from '@companion-bible/types';
import { TranscriptPanel } from './TranscriptPanel';
import { VerseQueuePanel } from './VerseQueuePanel';
import { StatusBar } from './StatusBar';
import type { AudioStatus, InternetStatus, AiStatus, StorageStatus } from './StatusBar';
import { useTranscript } from './useTranscript';
import { useVerseQueue } from './useVerseQueue';
import { CongregationPreview } from './CongregationPreview';
import type { ScreenMode } from './CongregationPreview';
import { BibleTab } from './BibleTab';
import { SermonTab } from './SermonTab';
import { HymnTab } from './HymnTab';
import { MoreTab } from './MoreTab';

// ── types ─────────────────────────────────────────────────────────────────────

interface DisplayedVerse {
  reference: string;
  text: string;
  translation: string;
}

type ActiveTab = 'bible' | 'sermon' | 'hymn' | 'more';

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

  // sermon display state (for "Now on Screen")
  const [sermonTitle, setSermonTitle] = useState<string | null>(null);
  const [sermonPoint, setSermonPoint] = useState<{ number: number; text: string } | null>(null);
  const [currentSubPoint, setCurrentSubPoint] = useState<string | null>(null);
  const [subPointIndex, setSubPointIndex] = useState<number | null>(null);

  // live transcript + verse queue
  const transcript = useTranscript();
  const queue = useVerseQueue();

  // display
  const [displayedVerse, setDisplayedVerse] = useState<DisplayedVerse | null>(null);
  const [screenMode, setScreenMode] = useState<ScreenMode>('idle');
  const [currentAnnouncementBody, setCurrentAnnouncementBody] = useState<string | null>(null);

  // hymn
  const [activeHymn, setActiveHymn] = useState<{ number: number; title: string } | null>(null);
  const [hymnSection, setHymnSection] = useState<{
    stanzaNumber: number | null;
    isChorus: boolean;
    lines: string[];
  } | null>(null);

  // announcements
  const [announcements, setAnnouncements] = useState<
    { id: number; body: string; durationSecs: number }[]
  >([]);
  const [announcementRunning, setAnnouncementRunning] = useState(false);
  const [announcementIndex, setAnnouncementIndex] = useState<number | null>(null);

  // order of service
  const [serviceItems, setServiceItems] = useState<
    { id: number; label: string; isCurrent: boolean }[]
  >([]);
  const [currentServiceLabel, setCurrentServiceLabel] = useState<string | null>(null);

  // chapter browser
  const [chapterBook, setChapterBook] = useState<string | null>(null);
  const [chapterNum, setChapterNum] = useState<number | null>(null);
  const [chapterActiveVerse, setChapterActiveVerse] = useState<number | null>(null);

  // tabs
  const [activeTab, setActiveTab] = useState<ActiveTab>('bible');

  // undo (5-second window)
  const [undoExpiresAt, setUndoExpiresAt] = useState<number | null>(null);
  const [undoSecsLeft, setUndoSecsLeft] = useState(0);

  // status
  const [transcriptionMode, setTranscriptionMode] = useState<'assemblyai' | 'deepgram' | 'whisper'>(
    'whisper',
  );
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
    void invoke<{ id: number; label: string; is_current: boolean }[]>('get_service_items').then(
      (items) => {
        setServiceItems(items.map((i) => ({ id: i.id, label: i.label, isCurrent: i.is_current })));
        const current = items.find((i) => i.is_current);
        if (current) setCurrentServiceLabel(current.label);
      },
    );
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
          setModelDownloadPercent(payload.percent);
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
          setScreenMode('verse');
          setChapterBook(payload.reference.book);
          setChapterNum(payload.reference.chapter);
          setChapterActiveVerse(payload.reference.verse ?? null);
          break;

        case 'DISPLAY_BLANKED':
          setDisplayedVerse(null);
          setScreenMode('blank');
          break;

        case 'DISPLAY_CLEARED':
          setDisplayedVerse(null);
          setScreenMode('idle');
          break;

        case 'SERMON_TITLE_SHOWN':
          setSermonTitle(payload.title);
          setScreenMode('title');
          break;

        case 'SERMON_POINT_SHOWN':
          setSermonPoint({ number: payload.number, text: payload.text });
          setScreenMode('point');
          break;

        case 'SUB_POINT_SHOWN':
          setCurrentSubPoint(payload.text);
          setSubPointIndex(payload.index);
          setScreenMode('subpoint');
          break;

        case 'INTERNET_CONNECTED':
          setInternet('online');
          break;

        case 'INTERNET_DISCONNECTED':
          setInternet('offline');
          break;

        case 'TRANSCRIPTION_MODE_CHANGED': {
          const mode = payload.mode;
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

        case 'HYMN_DETECTED':
          setActiveHymn({ number: payload.number, title: payload.title });
          break;

        case 'HYMN_SECTION_ADVANCED':
          setHymnSection({
            stanzaNumber: payload.stanza_number,
            isChorus: payload.is_chorus,
            lines: payload.lines,
          });
          setScreenMode('hymn');
          setActiveTab('hymn');
          break;

        case 'HYMN_COMPLETED':
          setHymnSection(null);
          setScreenMode('idle');
          break;

        case 'ANNOUNCEMENT_SHOWN':
          setAnnouncementIndex(payload.index);
          setAnnouncementRunning(true);
          setScreenMode('announcement');
          setCurrentAnnouncementBody(payload.body);
          break;

        case 'ANNOUNCEMENTS_STOPPED':
          setAnnouncementRunning(false);
          setAnnouncementIndex(null);
          setScreenMode('idle');
          setCurrentAnnouncementBody(null);
          break;

        case 'SERVICE_ITEM_CHANGED':
          setCurrentServiceLabel(payload.label);
          setServiceItems((prev) =>
            prev.map((i) => ({
              ...i,
              isCurrent: payload.label !== null && i.label === payload.label,
            })),
          );
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
    if (screenMode === 'idle') return;
    if (screenMode === 'verse' && displayedVerse) {
      void invoke('discard_verse').then(() => {
        setUndoExpiresAt(Date.now() + 5000);
        setUndoSecsLeft(5);
      });
    } else {
      void invoke('clear_congregation_display');
    }
  }, [screenMode, displayedVerse]);

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

  const handleSelectChapterVerse = useCallback((ref: string) => {
    void invoke('show_verse', { reference: ref, text: '' });
  }, []);

  const handleLoadHymn = useCallback((number: number) => {
    void invoke('load_hymn', { number });
  }, []);

  const handleNextStanza = useCallback(() => {
    void invoke('next_hymn_stanza');
  }, []);

  const handleNextVerse = useCallback(() => {
    void invoke('next_verse');
  }, []);

  const handlePrevVerse = useCallback(() => {
    void invoke('previous_verse');
  }, []);

  const handleAddAnnouncement = useCallback((body: string, durationSecs: number) => {
    void invoke<number>('add_announcement', { body, durationSecs }).then((id) => {
      setAnnouncements((prev) => [...prev, { id, body, durationSecs }]);
    });
  }, []);

  const handleRemoveAnnouncement = useCallback((id: number) => {
    void invoke('remove_announcement', { id }).then(() => {
      setAnnouncements((prev) => prev.filter((a) => a.id !== id));
    });
  }, []);

  const handleStartAnnouncements = useCallback(() => {
    void invoke('start_announcements');
  }, []);

  const handleStopAnnouncements = useCallback(() => {
    void invoke('stop_announcements');
  }, []);

  const handleNextAnnouncement = useCallback(() => {
    void invoke('next_announcement');
  }, []);

  const handlePrevAnnouncement = useCallback(() => {
    void invoke('prev_announcement');
  }, []);

  const handleAddServiceItem = useCallback((label: string) => {
    void invoke<number>('add_service_item', { label }).then((id) => {
      setServiceItems((prev) => [...prev, { id, label, isCurrent: false }]);
    });
  }, []);

  const handleRemoveServiceItem = useCallback((id: number) => {
    void invoke('remove_service_item', { id }).then(() => {
      setServiceItems((prev) => prev.filter((i) => i.id !== id));
    });
  }, []);

  const handleSetCurrentServiceItem = useCallback((id: number) => {
    void invoke('set_current_service_item', { id });
  }, []);

  const handleNextServiceItem = useCallback(() => {
    void invoke('next_service_item');
  }, []);

  const handleClearServiceItems = useCallback(() => {
    void invoke('clear_service_items').then(() => {
      setServiceItems([]);
      setCurrentServiceLabel(null);
    });
  }, []);

  const switchTab = useCallback(
    (tab: ActiveTab) => {
      setActiveTab(tab);
      if (tab === 'hymn') {
        void invoke('set_display_mode', { mode: 'hymn' });
      } else if (activeTab === 'hymn') {
        void invoke('set_display_mode', { mode: 'bible' });
      }
    },
    [activeTab],
  );

  // ── keyboard shortcuts ────────────────────────────────────────────────────

  useEffect(() => {
    const onKey = (e: KeyboardEvent) => {
      const tag = (e.target as HTMLElement).tagName;
      const inInput = tag === 'INPUT' || tag === 'TEXTAREA';
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
  const screenIsActive = screenMode !== 'idle' && screenMode !== 'blank';

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
        </div>
      </header>

      {/* ── Main ── */}
      <main className="op-main">
        {/* ── Left: preview + queue + transcript ── */}
        <div className="op-col op-col-left">
          <CongregationPreview
            screenMode={screenMode}
            congregationVisible={congregationVisible}
            verse={displayedVerse}
            sermonTitle={sermonTitle}
            sermonPoint={sermonPoint}
            subPoint={currentSubPoint}
            subPointIndex={subPointIndex}
            hymn={activeHymn}
            hymnSection={hymnSection}
            announcementBody={currentAnnouncementBody}
            transcriptLines={transcript.lines}
            sessionActive={sessionActive}
          />

          <VerseQueuePanel
            items={queue.items}
            sessionActive={sessionActive}
            onConfirm={handleConfirmVerse}
            onReject={handleRejectVerse}
          />

          <section className="op-panel op-panel-transcript">
            <h2 className="op-panel-heading">Live Transcript</h2>
            <TranscriptPanel lines={transcript.lines} sessionActive={sessionActive} />
          </section>
        </div>

        {/* ── Right: pinned Now on Screen + tabs ── */}
        <div className="op-col op-col-right">
          {/* ── Now on Screen — always visible ── */}
          <div className="op-now-on-screen">
            <div className="op-now-header">
              <span className="op-now-label">
                <span className={`verse-live-dot${screenIsActive ? ' verse-live-dot--on' : ''}`} />
                Now on Screen
              </span>
              <span className="op-now-mode">{screenMode}</span>
            </div>

            <div className="op-now-content">
              {screenMode === 'verse' && displayedVerse ? (
                <>
                  <div className="verse-display-ref">{displayedVerse.reference}</div>
                  <p className="verse-display-text">{displayedVerse.text || '(text loading…)'}</p>
                </>
              ) : screenMode === 'title' ? (
                <div className="op-now-text">{sermonTitle ?? '—'}</div>
              ) : screenMode === 'point' && sermonPoint ? (
                <div className="op-now-text">
                  <span className="op-now-eyebrow">Point {sermonPoint.number}</span>
                  {sermonPoint.text}
                </div>
              ) : screenMode === 'subpoint' ? (
                <div className="op-now-text">{currentSubPoint ?? '—'}</div>
              ) : screenMode === 'hymn' && activeHymn ? (
                <div className="op-now-text">
                  GHS {activeHymn.number} · {activeHymn.title}
                  {hymnSection && (
                    <span className="op-now-eyebrow">
                      {hymnSection.isChorus ? 'Chorus' : `Stanza ${hymnSection.stanzaNumber ?? ''}`}
                    </span>
                  )}
                </div>
              ) : screenMode === 'announcement' ? (
                <div className="op-now-text op-now-text--clamp">
                  {currentAnnouncementBody ?? '—'}
                </div>
              ) : (
                <p className="verse-display-empty">
                  {screenMode === 'blank' ? 'Screen is blank' : 'Nothing on screen'}
                </p>
              )}
            </div>

            {/* Verse nav — only when a verse is showing */}
            {screenMode === 'verse' && (
              <>
                <div className="op-now-verse-nav">
                  <button
                    className="btn btn-secondary"
                    onClick={handlePrevVerse}
                    title="Keyboard: ←"
                  >
                    ← Prev<kbd className="key-hint">←</kbd>
                  </button>
                  <button
                    className="btn btn-secondary"
                    onClick={handleNextVerse}
                    title="Keyboard: →"
                  >
                    Next →<kbd className="key-hint">→</kbd>
                  </button>
                </div>
                <div className="op-now-scroll-row">
                  <span className="op-now-scroll-label">Scroll</span>
                  <button
                    className="btn btn-secondary op-now-scroll-btn"
                    onClick={() => void invoke('scroll_congregation', { amount: -250 })}
                    title="Scroll congregation screen up"
                  >
                    ↑ Up
                  </button>
                  <button
                    className="btn btn-secondary op-now-scroll-btn"
                    onClick={() => void invoke('scroll_congregation', { amount: 250 })}
                    title="Scroll congregation screen down"
                  >
                    ↓ Down
                  </button>
                </div>
              </>
            )}

            {/* Clear + Undo */}
            <div className="op-now-actions">
              <button
                className="btn-discard op-now-clear"
                disabled={screenMode === 'idle'}
                onClick={handleDiscard}
                title="Keyboard: Space"
              >
                Clear Screen<kbd className="key-hint">Space</kbd>
              </button>
              {showUndo && (
                <button
                  className="btn-undo op-now-undo"
                  onClick={handleUndo}
                  title="Keyboard: Ctrl+Z"
                >
                  ↩ Undo<span className="undo-timer">{undoSecsLeft}s</span>
                  <kbd className="key-hint">Ctrl+Z</kbd>
                </button>
              )}
            </div>
          </div>

          {/* ── Tab bar ── */}
          <div className="op-tab-bar" role="tablist">
            {(['bible', 'sermon', 'hymn', 'more'] as const).map((tab) => (
              <button
                key={tab}
                role="tab"
                aria-selected={activeTab === tab}
                className={`op-tab-btn${activeTab === tab ? ' op-tab-btn--active' : ''}`}
                onClick={() => switchTab(tab)}
              >
                {tab.charAt(0).toUpperCase() + tab.slice(1)}
              </button>
            ))}
          </div>

          {/* ── Tab content — all tabs stay mounted to preserve state ── */}
          <div className="op-tab-content" role="tabpanel">
            <div hidden={activeTab !== 'bible'}>
              <BibleTab
                chapterBook={chapterBook}
                chapterNum={chapterNum}
                chapterActiveVerse={chapterActiveVerse}
                onSelectVerse={handleSelectChapterVerse}
                onManualSubmit={handleManualOverride}
              />
            </div>
            <div hidden={activeTab !== 'sermon'}>
              <SermonTab />
            </div>
            <div hidden={activeTab !== 'hymn'}>
              <HymnTab
                activeHymn={activeHymn}
                hymnSection={hymnSection}
                onLoadHymn={handleLoadHymn}
                onNextStanza={handleNextStanza}
              />
            </div>
            <div hidden={activeTab !== 'more'}>
              <MoreTab
                serviceItems={serviceItems}
                currentServiceLabel={currentServiceLabel}
                onAddServiceItem={handleAddServiceItem}
                onRemoveServiceItem={handleRemoveServiceItem}
                onSetCurrentServiceItem={handleSetCurrentServiceItem}
                onNextServiceItem={handleNextServiceItem}
                onClearServiceItems={handleClearServiceItems}
                announcements={announcements}
                announcementRunning={announcementRunning}
                announcementIndex={announcementIndex}
                onAddAnnouncement={handleAddAnnouncement}
                onRemoveAnnouncement={handleRemoveAnnouncement}
                onStartAnnouncements={handleStartAnnouncements}
                onStopAnnouncements={handleStopAnnouncements}
                onNextAnnouncement={handleNextAnnouncement}
                onPrevAnnouncement={handlePrevAnnouncement}
                sessionActive={sessionActive}
              />
            </div>
          </div>
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
