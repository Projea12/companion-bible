import { useRef } from 'react';
import type { TranscriptLine } from './useTranscript';

export type ScreenMode =
  | 'idle'
  | 'blank'
  | 'verse'
  | 'title'
  | 'subpoint'
  | 'hymn'
  | 'announcement';

export interface CongregationPreviewProps {
  screenMode: ScreenMode;
  congregationVisible: boolean;
  verse: { reference: string; text: string; translation: string } | null;
  sermonTitle: string | null;
  subPoint: string | null;
  hymn: { number: number; title: string } | null;
  hymnSection: { stanzaNumber: number | null; isChorus: boolean; lines: string[] } | null;
  announcementBody: string | null;
  transcriptLines: TranscriptLine[];
  sessionActive: boolean;
}

export function CongregationPreview(props: CongregationPreviewProps) {
  const {
    screenMode,
    congregationVisible,
    verse,
    sermonTitle,
    subPoint,
    hymn,
    hymnSection,
    announcementBody,
    transcriptLines,
    sessionActive,
  } = props;

  const verseScrollRef = useRef<HTMLDivElement>(null);

  // ── preview inner content ────────────────────────────────────────────────

  function renderInner() {
    switch (screenMode) {
      case 'blank':
        return null;

      case 'verse':
        return (
          <div className="cpv-verse-scroll" ref={verseScrollRef}>
            <div className="cpv-verse-card">
              <img src="/deeper_life_logo.png" className="cpv-verse-card-logo" aria-hidden="true" />
              <p className="cpv-verse-text">{verse?.text ?? ''}</p>
              <hr className="cpv-verse-rule" />
              <p className="cpv-verse-ref">{verse?.reference ?? ''}</p>
              <p className="cpv-verse-trans">{verse?.translation ?? ''}</p>
            </div>
          </div>
        );

      case 'title':
        return (
          <div className="cpv-centered">
            <div className="cpv-title-card">
              <p className="cpv-title-eyebrow">Sermon</p>
              <p className="cpv-title-text">{sermonTitle ?? ''}</p>
            </div>
          </div>
        );

      case 'subpoint':
        return (
          <div className="cpv-centered">
            <div className="cpv-subpoint-card">
              <p className="cpv-subpoint-text">{subPoint ?? ''}</p>
            </div>
          </div>
        );

      case 'hymn': {
        const sectionLabel = hymnSection?.isChorus
          ? 'Chorus'
          : `Stanza ${hymnSection?.stanzaNumber ?? ''}`;
        return (
          <div className="cpv-centered">
            <div className="cpv-hymn-card">
              <div className="cpv-hymn-eyebrow">
                <span>{hymn ? `GHS ${hymn.number}` : ''}</span>
                <span className="cpv-hymn-sep">·</span>
                <span>{sectionLabel}</span>
              </div>
              {hymn && <p className="cpv-hymn-title">{hymn.title}</p>}
              <div className="cpv-hymn-lines">
                {hymnSection?.lines.map((line, i) => (
                  <p key={i} className="cpv-hymn-line">
                    {line}
                  </p>
                ))}
              </div>
            </div>
          </div>
        );
      }

      case 'announcement':
        return (
          <div className="cpv-announcement">
            <div className="cpv-ann-logo-area">
              <img src="/deeper_life_logo.png" alt="Deeper Life" className="cpv-ann-logo" />
              <p className="cpv-ann-church">Blessed Group&nbsp;&nbsp;Poka</p>
            </div>
            <div className="cpv-ann-body-wrap">
              <p className="cpv-ann-body">{announcementBody ?? ''}</p>
            </div>
          </div>
        );

      case 'idle':
      default:
        return (
          <div className="cpv-centered">
            <span className="cpv-idle-mark">✦</span>
          </div>
        );
    }
  }

  // ── now hearing ──────────────────────────────────────────────────────────

  const lastLine = transcriptLines.length > 0 ? transcriptLines[transcriptLines.length - 1] : null;

  function renderNowHearing() {
    if (!sessionActive) {
      return (
        <span className="cong-now-hearing-text cong-now-hearing-text--muted">Start a session</span>
      );
    }
    if (!lastLine) {
      return (
        <span className="cong-now-hearing-text cong-now-hearing-text--muted">
          Waiting for audio…
        </span>
      );
    }
    return (
      <span className="cong-now-hearing-text">
        {lastLine.text}
        {lastLine.detectedRef && (
          <span className="cong-now-hearing-badge">{lastLine.detectedRef}</span>
        )}
      </span>
    );
  }

  const liveClass = `cong-preview-live cong-preview-live--${congregationVisible ? 'on' : 'off'}`;

  return (
    <div className="cong-preview-wrap">
      <div className="cong-preview-box">{renderInner()}</div>

      <div className="cong-preview-status-row">
        <span className={liveClass}>
          {congregationVisible ? '●' : '○'}&nbsp;
          {congregationVisible ? 'LIVE' : 'Hidden'}
        </span>
        <span className="cong-preview-mode-label">{screenMode}</span>
      </div>

      <div className="cong-now-hearing">
        <span className="cong-now-hearing-label">Now Hearing</span>
        {renderNowHearing()}
      </div>
    </div>
  );
}
