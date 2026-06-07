import { useEffect, useRef } from 'react';
import '../congregation/congregation.css';
import type { TranscriptLine } from './useTranscript';

function toRoman(n: number): string {
  const map: [number, string][] = [
    [1000, 'M'],
    [900, 'CM'],
    [500, 'D'],
    [400, 'CD'],
    [100, 'C'],
    [90, 'XC'],
    [50, 'L'],
    [40, 'XL'],
    [10, 'X'],
    [9, 'IX'],
    [5, 'V'],
    [4, 'IV'],
    [1, 'I'],
  ];
  let result = '';
  for (const [val, sym] of map) {
    while (n >= val) {
      result += sym;
      n -= val;
    }
  }
  return result;
}

export type ScreenMode =
  | 'idle'
  | 'blank'
  | 'verse'
  | 'title'
  | 'point'
  | 'subpoint'
  | 'hymn'
  | 'announcement';

export interface CongregationPreviewProps {
  screenMode: ScreenMode;
  congregationVisible: boolean;
  verse: { reference: string; text: string; translation: string } | null;
  sermonTitle: string | null;
  sermonPoint: { number: number; text: string } | null;
  subPoint: string | null;
  subPointIndex: number | null;
  verseScrollSignal: { amount: number; id: number } | null;
  hymn: { number: number; title: string } | null;
  hymnSection: { stanzaNumber: number | null; isChorus: boolean; lines: string[] } | null;
  announcementBody: string | null;
  transcriptLines: TranscriptLine[];
  sessionActive: boolean;
  serviceName: string;
}

export function CongregationPreview(props: CongregationPreviewProps) {
  const {
    screenMode,
    congregationVisible,
    verse,
    sermonTitle,
    sermonPoint,
    subPoint,
    subPointIndex,
    verseScrollSignal,
    hymn,
    hymnSection,
    announcementBody,
    transcriptLines,
    sessionActive,
    serviceName,
  } = props;

  const verseRef = useRef<HTMLDivElement>(null);
  const verseRafRef = useRef(0);
  const verseTimerRef = useRef(0);

  // Mirror main.ts startVerseScroll — restarts whenever the verse changes
  useEffect(() => {
    cancelAnimationFrame(verseRafRef.current);
    clearTimeout(verseTimerRef.current);

    const el = verseRef.current;
    if (!verse || !el) return;

    el.scrollTop = 0;

    const PX_PER_MS = 25 / 1000;
    let lastTime: number | null = null;
    const deadline = performance.now() + 1500;

    function step(now: number) {
      const maxScroll = el!.scrollHeight - el!.clientHeight;
      if (maxScroll <= 0) {
        if (now < deadline) verseRafRef.current = requestAnimationFrame(step);
        return;
      }
      if (lastTime !== null) {
        el!.scrollTop = Math.min(el!.scrollTop + PX_PER_MS * (now - lastTime), maxScroll);
      }
      lastTime = now;
      if (el!.scrollTop < maxScroll) verseRafRef.current = requestAnimationFrame(step);
    }

    verseTimerRef.current = window.setTimeout(() => {
      verseRafRef.current = requestAnimationFrame(step);
    }, 350);

    return () => {
      clearTimeout(verseTimerRef.current);
      cancelAnimationFrame(verseRafRef.current);
    };
  }, [verse]);

  // Manual scroll — stop auto-scroll, apply operator's scroll amount
  useEffect(() => {
    if (!verseScrollSignal || !verseRef.current) return;
    cancelAnimationFrame(verseRafRef.current);
    clearTimeout(verseTimerRef.current);
    verseRef.current.scrollBy({ top: verseScrollSignal.amount, behavior: 'smooth' });
  }, [verseScrollSignal]);

  const hymnSectionLabel = hymnSection?.isChorus
    ? 'Chorus'
    : `Stanza ${hymnSection?.stanzaNumber ?? ''}`;

  const liveClass = `cong-preview-live cong-preview-live--${congregationVisible ? 'on' : 'off'}`;

  const lastLine = transcriptLines.length > 0 ? transcriptLines[transcriptLines.length - 1] : null;

  return (
    <div className="cong-preview-wrap">
      {/*
       * The inner .congregation-app uses the REAL congregation CSS classes.
       * congregation.css uses cqw/cqh (container query units) so everything
       * scales correctly relative to .cong-preview-box — zero duplication,
       * changes to congregation.css auto-reflect here.
       */}
      <div className="cong-preview-box">
        <div className="congregation-app">
          {/* idle */}
          <div className="congregation-state state-idle" hidden={screenMode !== 'idle'}>
            <div className="idle-top-bar">
              <img src="/deeper_life_logo.png" className="idle-logo-bug" aria-hidden="true" />
            </div>
            <div className="idle-body-zone">
              <span className="idle-ghost" aria-hidden="true">
                ✦
              </span>
              <div className="idle-content">
                <img src="/deeper_life_logo.png" className="idle-center-logo" aria-hidden="true" />
                <div className="idle-rule" aria-hidden="true" />
                <div className="idle-church-name">Blessed Group&nbsp;&nbsp;Poka</div>
              </div>
            </div>
            <div className="idle-footer">
              <div className="idle-service-name">{serviceName}</div>
            </div>
          </div>

          {/* verse */}
          <div
            ref={verseRef}
            className="congregation-state state-verse"
            hidden={screenMode !== 'verse'}
          >
            <div className="verse-card">
              <img src="/deeper_life_logo.png" className="verse-card-logo" aria-hidden="true" />
              <div className="verse-text">{verse?.text ?? ''}</div>
              <div className="verse-rule" aria-hidden="true"></div>
              <div className="verse-reference">{verse?.reference ?? ''}</div>
              <div className="verse-translation">{verse?.translation ?? ''}</div>
            </div>
          </div>

          {/* sermon title */}
          <div className="congregation-state state-sermon" hidden={screenMode !== 'title'}>
            <div className="sermon-top-bar">
              <img src="/deeper_life_logo.png" className="sermon-logo-bug" aria-hidden="true" />
            </div>
            <div className="sermon-body-zone">
              <img src="/deeper_life_logo.png" className="sermon-watermark" aria-hidden="true" />
              <span className="sermon-ghost" aria-hidden="true">
                SERMON
              </span>
              <div className="sermon-content">
                <div className="sermon-eyebrow">Sermon</div>
                <div className="title-text">{sermonTitle ?? ''}</div>
              </div>
            </div>
            <div className="sermon-footer">
              <div className="sermon-church-name">Blessed Group&nbsp;&nbsp;Poka</div>
            </div>
          </div>

          {/* main point */}
          <div className="congregation-state state-sermon" hidden={screenMode !== 'point'}>
            <div className="sermon-top-bar">
              <div className="sermon-badge">
                Point&nbsp;&nbsp;{sermonPoint ? sermonPoint.number : ''}
              </div>
              <img src="/deeper_life_logo.png" className="sermon-logo-bug" aria-hidden="true" />
            </div>
            <div className="sermon-body-zone">
              <img src="/deeper_life_logo.png" className="sermon-watermark" aria-hidden="true" />
              <span className="sermon-ghost sermon-ghost--number" aria-hidden="true">
                {sermonPoint ? sermonPoint.number : ''}
              </span>
              <div className="sermon-content">
                <div className="point-text">{sermonPoint?.text ?? ''}</div>
              </div>
            </div>
            <div className="sermon-footer">
              <div className="sermon-church-name">Blessed Group&nbsp;&nbsp;Poka</div>
            </div>
          </div>

          {/* sub-point */}
          <div className="congregation-state state-sermon" hidden={screenMode !== 'subpoint'}>
            <div className="sermon-top-bar">
              <div className="sermon-badge">
                {sermonPoint
                  ? `Point\u00a0\u00a0${sermonPoint.number}${subPointIndex !== null ? `\u00a0·\u00a0${toRoman(subPointIndex + 1)}` : ''}`
                  : ''}
              </div>
              <img src="/deeper_life_logo.png" className="sermon-logo-bug" aria-hidden="true" />
            </div>
            <div className="sermon-body-zone">
              <img src="/deeper_life_logo.png" className="sermon-watermark" aria-hidden="true" />
              <span className="sermon-ghost sermon-ghost--deco" aria-hidden="true">
                ✦
              </span>
              <div className="sermon-content">
                <div className="subpoint-text">{subPoint ?? ''}</div>
              </div>
            </div>
            <div className="sermon-footer">
              <div className="sermon-church-name">Blessed Group&nbsp;&nbsp;Poka</div>
            </div>
          </div>

          {/* hymn */}
          <div className="congregation-state" hidden={screenMode !== 'hymn'}>
            <div className="hymn-card">
              <div className="hymn-eyebrow">
                <span>{hymn ? `GHS ${hymn.number}` : ''}</span>
                <span className="hymn-eyebrow-sep" aria-hidden="true">
                  ·
                </span>
                <span>{hymnSectionLabel}</span>
              </div>
              {hymn && <div className="hymn-title">{hymn.title}</div>}
              <div className="hymn-lines">
                {hymnSection?.lines.map((line, i) => (
                  <p key={i} className="hymn-line">
                    {line}
                  </p>
                ))}
              </div>
            </div>
          </div>

          {/* announcement */}
          <div
            className="congregation-state state-announcement"
            hidden={screenMode !== 'announcement'}
          >
            <div className="ann-top-bar">
              <img src="/deeper_life_logo.png" className="ann-logo-bug" aria-hidden="true" />
            </div>
            <div className="ann-body-zone">
              <img src="/deeper_life_logo.png" className="ann-watermark" aria-hidden="true" />
              <div className="ann-text">{announcementBody ?? ''}</div>
            </div>
            <div className="ann-footer">
              <div className="ann-church-name">Blessed Group&nbsp;&nbsp;Poka</div>
            </div>
          </div>

          {/* blank */}
          <div
            className="congregation-state congregation-blank"
            hidden={screenMode !== 'blank'}
          ></div>
        </div>
      </div>

      <div className="cong-preview-status-row">
        <span className={liveClass}>
          {congregationVisible ? '●' : '○'}&nbsp;
          {congregationVisible ? 'LIVE' : 'Hidden'}
        </span>
        <span className="cong-preview-mode-label">{screenMode}</span>
      </div>

      <div className="cong-now-hearing">
        <span className="cong-now-hearing-label">Now Hearing</span>
        {sessionActive ? (
          lastLine ? (
            <span className="cong-now-hearing-text">
              {lastLine.text}
              {lastLine.detectedRef && (
                <span className="cong-now-hearing-badge">{lastLine.detectedRef}</span>
              )}
            </span>
          ) : (
            <span className="cong-now-hearing-text cong-now-hearing-text--muted">
              Waiting for audio…
            </span>
          )
        ) : (
          <span className="cong-now-hearing-text cong-now-hearing-text--muted">
            Start a session
          </span>
        )}
      </div>
    </div>
  );
}
