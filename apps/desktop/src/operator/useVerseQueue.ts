import { useCallback, useEffect, useRef, useState } from 'react';

// ── constants ─────────────────────────────────────────────────────────────────

export const QUEUE_EXPIRY_MS = 30_000;

// ── types ─────────────────────────────────────────────────────────────────────

export interface QueuedVerse {
  id: number;
  label: string;
  confidence: number;
  expiresAt: number;
}

export type ConfidenceLevel = 'high' | 'medium' | 'low';

// ── helpers ───────────────────────────────────────────────────────────────────

/** Classifies a 0-100 confidence score into a display level. */
export function confidenceLevel(confidence: number): ConfidenceLevel {
  if (confidence >= 75) return 'high';
  if (confidence >= 40) return 'medium';
  return 'low';
}

// ── hook ──────────────────────────────────────────────────────────────────────

export interface UseVerseQueue {
  items: QueuedVerse[];
  /** Add a verse to the queue. Ignores duplicates by label. */
  enqueue(label: string, confidence: number, now?: number): void;
  /** Remove a single item by id (after confirm or reject). */
  remove(id: number): void;
  /** Drop all items whose expiresAt ≤ now. */
  expireOld(now?: number): void;
  /** Remove everything — call on session end. */
  clear(): void;
}

export function useVerseQueue(): UseVerseQueue {
  const [items, setItems] = useState<QueuedVerse[]>([]);
  const nextId = useRef(0);

  const enqueue = useCallback((label: string, confidence: number, now = Date.now()) => {
    setItems((prev) => {
      if (prev.some((v) => v.label === label)) return prev;
      const id = ++nextId.current;
      return [...prev, { id, label, confidence, expiresAt: now + QUEUE_EXPIRY_MS }];
    });
  }, []);

  const remove = useCallback((id: number) => {
    setItems((prev) => prev.filter((v) => v.id !== id));
  }, []);

  const expireOld = useCallback((now = Date.now()) => {
    setItems((prev) => prev.filter((v) => v.expiresAt > now));
  }, []);

  const clear = useCallback(() => setItems([]), []);

  // Automatically purge expired entries every second.
  useEffect(() => {
    const id = setInterval(() => expireOld(Date.now()), 1000);
    return () => clearInterval(id);
  }, [expireOld]);

  return { items, enqueue, remove, expireOld, clear };
}
