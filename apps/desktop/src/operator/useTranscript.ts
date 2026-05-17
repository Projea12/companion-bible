import { useCallback, useRef, useState } from 'react';

// ── types ─────────────────────────────────────────────────────────────────────

export interface TranscriptLine {
  id: number;
  chunkId: number;
  text: string;
  /** Reference label to highlight inside this line, or null. */
  detectedRef: string | null;
}

// ── constants ─────────────────────────────────────────────────────────────────

export const MAX_TRANSCRIPT_LINES = 8;

// ── hook ──────────────────────────────────────────────────────────────────────

export interface UseTranscript {
  lines: TranscriptLine[];
  addLine(chunkId: number, text: string): void;
  /** Find the line matching sourceText and annotate it with refLabel. */
  markDetection(sourceText: string, refLabel: string): void;
  clear(): void;
}

export function useTranscript(): UseTranscript {
  const [lines, setLines] = useState<TranscriptLine[]>([]);
  const nextId = useRef(0);

  const addLine = useCallback((chunkId: number, text: string) => {
    setLines((prev) => {
      const line: TranscriptLine = { id: ++nextId.current, chunkId, text, detectedRef: null };
      return [...prev, line].slice(-MAX_TRANSCRIPT_LINES);
    });
  }, []);

  const markDetection = useCallback((sourceText: string, refLabel: string) => {
    setLines((prev) =>
      prev.map((line) =>
        matchesSource(line.text, sourceText) ? { ...line, detectedRef: refLabel } : line,
      ),
    );
  }, []);

  const clear = useCallback(() => setLines([]), []);

  return { lines, addLine, markDetection, clear };
}

// ── helpers ───────────────────────────────────────────────────────────────────

/**
 * Returns true when a transcript line's text corresponds to the given
 * source_text from a detection event. Handles exact match and the common
 * case where the detection window spans a slightly wider or narrower
 * text segment than a single transcription chunk.
 */
export function matchesSource(lineText: string, sourceText: string): boolean {
  if (!lineText || !sourceText) return false;
  const a = lineText.trim();
  const b = sourceText.trim();
  return a === b || a.includes(b) || b.includes(a);
}
