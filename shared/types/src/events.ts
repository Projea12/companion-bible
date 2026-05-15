import type { BibleReference } from './bible.js';

// Mirrors the Rust AppEvent enum. The `type` field uses SCREAMING_SNAKE_CASE.

export type AppEvent =
  // ── Audio ──────────────────────────────────────────────────────────────
  | { type: 'AUDIO_CAPTURE_STARTED'; device_id: string }
  | { type: 'AUDIO_CAPTURE_STOPPED' }
  | { type: 'AUDIO_CHUNK_CAPTURED'; chunk_id: number; duration_ms: number }

  // ── Transcription ──────────────────────────────────────────────────────
  | { type: 'TRANSCRIPTION_STARTED'; chunk_id: number }
  | { type: 'TRANSCRIPTION_COMPLETED'; chunk_id: number; text: string; duration_ms: number }
  | { type: 'TRANSCRIPTION_FAILED'; chunk_id: number; reason: string }

  // ── Detection ──────────────────────────────────────────────────────────
  | { type: 'SCRIPTURE_REFERENCE_DETECTED'; references: BibleReference[]; source_text: string }
  | { type: 'NO_REFERENCE_FOUND'; source_text: string }

  // ── Bible ──────────────────────────────────────────────────────────────
  | { type: 'VERSE_LOADED'; reference: BibleReference; text: string; translation: string }
  | { type: 'VERSE_LOAD_FAILED'; reference: BibleReference; reason: string }

  // ── AI ─────────────────────────────────────────────────────────────────
  | { type: 'AI_QUERY_STARTED'; query_id: number }
  | { type: 'AI_RESPONSE_RECEIVED'; query_id: number; response: string }
  | { type: 'AI_QUERY_FAILED'; query_id: number; reason: string }

  // ── Display ────────────────────────────────────────────────────────────
  | { type: 'VERSE_DISPLAYED'; reference: BibleReference }
  | { type: 'DISPLAY_CLEARED' }

  // ── System ─────────────────────────────────────────────────────────────
  | { type: 'APP_STARTED'; version: string }
  | { type: 'APP_SHUTDOWN' }
  | { type: 'UPDATE_AVAILABLE'; version: string; release_notes?: string }
  | { type: 'UPDATE_DOWNLOADED'; version: string }
  | { type: 'UPDATE_INSTALLED'; version: string }
  | { type: 'ONBOARDING_COMPLETED' }

  // ── Watchdog ───────────────────────────────────────────────────────────
  | { type: 'HEALTH_CHECK_PASSED'; component: string }
  | { type: 'HEALTH_CHECK_FAILED'; component: string; reason: string }
  | { type: 'PROCESS_RESTARTED'; component: string; restart_count: number }

  // ── Database ───────────────────────────────────────────────────────────
  | { type: 'DATABASE_READY' }
  | { type: 'DATABASE_MIGRATED'; from_version: number; to_version: number }

  // ── Config ─────────────────────────────────────────────────────────────
  | { type: 'CONFIG_LOADED' }
  | { type: 'CONFIG_UPDATED'; key: string };

export type AppEventType = AppEvent['type'];

export function isAppEvent(value: unknown): value is AppEvent {
  return (
    typeof value === 'object' &&
    value !== null &&
    'type' in value &&
    typeof (value as Record<string, unknown>)['type'] === 'string'
  );
}
