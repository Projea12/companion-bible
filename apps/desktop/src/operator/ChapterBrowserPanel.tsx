import { useEffect, useRef, useState } from 'react';
import { invoke } from '@tauri-apps/api/core';

interface ChapterVerse {
  verse: number;
  text: string;
}

interface ChapterBrowserPanelProps {
  book: string | null;
  chapter: number | null;
  activeVerse: number | null;
  onSelectVerse: (reference: string) => void;
}

export function ChapterBrowserPanel({
  book,
  chapter,
  activeVerse,
  onSelectVerse,
}: ChapterBrowserPanelProps) {
  const [verses, setVerses] = useState<ChapterVerse[]>([]);
  const [loading, setLoading] = useState(false);
  const [hoverVerse, setHoverVerse] = useState<ChapterVerse | null>(null);
  const activeRef = useRef<HTMLButtonElement>(null);

  useEffect(() => {
    if (!book || !chapter) {
      setVerses([]);
      setLoading(false);
      return;
    }
    setLoading(true);
    invoke<ChapterVerse[]>('get_chapter_verses', { book, chapter })
      .then(setVerses)
      .catch(() => setVerses([]))
      .finally(() => setLoading(false));
  }, [book, chapter]);

  useEffect(() => {
    activeRef.current?.scrollIntoView({ block: 'nearest', behavior: 'smooth' });
  }, [activeVerse, verses]);

  const activeVerseData = verses.find((v) => v.verse === activeVerse) ?? null;
  const previewVerse = hoverVerse ?? activeVerseData;

  return (
    <section className="op-panel op-panel-chapter-browser" aria-label="Chapter browser">
      <div className="op-panel-header">
        <h2 className="op-panel-heading">
          {book && chapter ? `${book} ${chapter}` : 'Chapter Browser'}
        </h2>
        {verses.length > 0 && <span className="chapter-browser-count">{verses.length} verses</span>}
      </div>

      {/* Preview bar — shows hovered verse, falls back to active, then hint */}
      <div className="chapter-verse-preview">
        {!book || !chapter ? (
          <span className="chapter-verse-preview-hint">Confirm a verse to browse the chapter</span>
        ) : previewVerse ? (
          <>
            <span className="chapter-verse-preview-ref">
              {book}&nbsp;{chapter}:{previewVerse.verse}
            </span>
            <span className="chapter-verse-preview-text">{previewVerse.text}</span>
          </>
        ) : (
          <span className="chapter-verse-preview-hint">Hover a number to preview</span>
        )}
      </div>

      {/* Number grid */}
      {book && chapter && (
        <>
          {loading ? (
            <p className="chapter-browser-loading">Loading…</p>
          ) : (
            <div className="chapter-verse-grid" aria-label={`${book} chapter ${chapter}`}>
              {verses.map((v) => {
                const isActive = v.verse === activeVerse;
                return (
                  <button
                    key={v.verse}
                    ref={isActive ? activeRef : null}
                    className={`chapter-verse-num-btn${isActive ? ' chapter-verse-num-btn--active' : ''}`}
                    onClick={() => onSelectVerse(`${book} ${chapter}:${v.verse}`)}
                    onMouseEnter={() => setHoverVerse(v)}
                    onMouseLeave={() => setHoverVerse(null)}
                    aria-label={`Show ${book} ${chapter}:${v.verse} on screen`}
                    aria-current={isActive ? 'true' : undefined}
                  >
                    {v.verse}
                  </button>
                );
              })}
            </div>
          )}
        </>
      )}
    </section>
  );
}
