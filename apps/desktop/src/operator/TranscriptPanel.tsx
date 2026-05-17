import { useEffect, useRef } from 'react';
import type { TranscriptLine } from './useTranscript';

// ── component ─────────────────────────────────────────────────────────────────

export interface TranscriptPanelProps {
  lines: TranscriptLine[];
  sessionActive: boolean;
}

export function TranscriptPanel({ lines, sessionActive }: TranscriptPanelProps) {
  const endRef = useRef<HTMLDivElement>(null);

  useEffect(() => {
    endRef.current?.scrollIntoView({ behavior: 'smooth' });
  }, [lines]);

  return (
    <div className="transcript-scroll" role="log" aria-live="polite" aria-label="Live transcript">
      {lines.length === 0 ? (
        <p className="transcript-empty">
          {sessionActive ? 'Waiting for audio…' : 'Start a session to see transcript'}
        </p>
      ) : (
        lines.map((line) => (
          <p
            key={line.id}
            className={`transcript-line${line.detectedRef ? ' transcript-line--detected' : ''}`}
          >
            <HighlightedText text={line.text} highlight={line.detectedRef} />
            {line.detectedRef && <span className="transcript-ref-badge">{line.detectedRef}</span>}
          </p>
        ))
      )}
      <div ref={endRef} aria-hidden="true" />
    </div>
  );
}

// ── highlighted text ──────────────────────────────────────────────────────────

interface HighlightedTextProps {
  text: string;
  highlight: string | null;
}

export function HighlightedText({ text, highlight }: HighlightedTextProps) {
  if (!highlight) return <>{text}</>;

  const idx = text.toLowerCase().indexOf(highlight.toLowerCase());
  if (idx === -1) return <>{text}</>;

  return (
    <>
      {text.slice(0, idx)}
      <mark className="transcript-highlight">{text.slice(idx, idx + highlight.length)}</mark>
      {text.slice(idx + highlight.length)}
    </>
  );
}
