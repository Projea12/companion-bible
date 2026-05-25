import { useEffect, useRef } from 'react';
import type { RefObject } from 'react';

export type CongregationDisplayState = 'idle' | 'verse' | 'blank' | 'title' | 'subpoint' | 'hymn';

interface DisplayedVerse {
  reference: string;
  text: string;
  translation: string;
}

interface Props {
  displayState: CongregationDisplayState;
  verse: DisplayedVerse | null;
  sermonTitle: string | null;
  subPoint: string | null;
  hymn: { number: number; title: string } | null;
  hymnSection: { stanzaNumber: number | null; isChorus: boolean; lines: string[] } | null;
  contentRef: RefObject<HTMLDivElement>;
}

// Mirrors the fitHymnText() logic from congregation/main.ts, scaled to the
// preview container's pixel height instead of the full viewport height.
function fitHymn(container: HTMLElement) {
  container.style.removeProperty('--cp-hymn-fs');
  requestAnimationFrame(() => {
    const card = container.querySelector('.cp-hymn-card');
    if (!card) return;

    const maxFs = Math.round(container.clientHeight * 0.055);
    const minFs = Math.max(6, Math.round(container.clientHeight * 0.028));
    let size = maxFs;
    container.style.setProperty('--cp-hymn-fs', `${size}px`);

    requestAnimationFrame(function shrink() {
      if (card.scrollHeight > container.clientHeight && size > minFs) {
        size = Math.max(minFs, size - 1);
        container.style.setProperty('--cp-hymn-fs', `${size}px`);
        requestAnimationFrame(shrink);
      }
    });
  });
}

export function CongregationPreview({
  displayState,
  verse,
  sermonTitle,
  subPoint,
  hymn,
  hymnSection,
  contentRef,
}: Props) {
  const hymnStateRef = useRef<HTMLDivElement>(null);

  // Re-fit hymn text whenever the section changes or hymn state becomes active.
  useEffect(() => {
    if (displayState !== 'hymn' || !hymnStateRef.current) return;
    fitHymn(hymnStateRef.current);
  }, [displayState, hymnSection]);

  return (
    <div className="cong-preview">
      <p className="cong-preview-label">Congregation Preview</p>

      <div className="cong-preview-screen">
        {/* Idle */}
        <div
          className="cp-state cp-state-idle"
          style={{ display: displayState === 'idle' ? 'flex' : 'none' }}
        >
          <div className="cp-idle-mark" aria-hidden="true">
            ✦
          </div>
        </div>

        {/* Blank */}
        <div
          className="cp-state cp-state-blank"
          style={{ display: displayState === 'blank' ? 'flex' : 'none' }}
        />

        {/* Verse */}
        <div
          ref={contentRef}
          className="cp-state cp-state-verse"
          style={{ display: displayState === 'verse' ? 'flex' : 'none' }}
        >
          {verse && (
            <div className="cp-verse-card">
              <div className="cp-verse-text">{verse.text}</div>
              <div className="cp-verse-rule" aria-hidden="true" />
              <div className="cp-verse-reference">{verse.reference}</div>
              <div className="cp-verse-translation">{verse.translation}</div>
            </div>
          )}
        </div>

        {/* Sermon title */}
        <div
          className="cp-state cp-state-title"
          style={{ display: displayState === 'title' ? 'flex' : 'none' }}
        >
          <div className="cp-title-card">
            <div className="cp-title-eyebrow">Sermon</div>
            <div className="cp-title-text">{sermonTitle}</div>
          </div>
        </div>

        {/* Sub-point */}
        <div
          className="cp-state cp-state-subpoint"
          style={{ display: displayState === 'subpoint' ? 'flex' : 'none' }}
        >
          <div className="cp-subpoint-card">
            <div className="cp-subpoint-text">{subPoint}</div>
          </div>
        </div>

        {/* Hymn */}
        <div
          ref={hymnStateRef}
          className="cp-state cp-state-hymn"
          style={{ display: displayState === 'hymn' ? 'flex' : 'none' }}
        >
          {hymn && hymnSection && (
            <div className="cp-hymn-card">
              <div className="cp-hymn-eyebrow">
                <span>GHS {hymn.number}</span>
                <span className="cp-hymn-eyebrow-sep" aria-hidden="true">
                  ·
                </span>
                <span>
                  {hymnSection.isChorus ? 'Chorus' : `Stanza ${hymnSection.stanzaNumber ?? ''}`}
                </span>
              </div>
              <div className="cp-hymn-title">{hymn.title}</div>
              <div className="cp-hymn-lines">
                {hymnSection.lines.map((line, i) => (
                  <p key={i} className="cp-hymn-line">
                    {line}
                  </p>
                ))}
              </div>
            </div>
          )}
        </div>
      </div>
    </div>
  );
}
