import { listen } from '@tauri-apps/api/event';
import type { AppEvent } from '@companion-bible/types';

// ─── element references ───────────────────────────────────────────────────────

const stateIdle = document.getElementById('state-idle') as HTMLDivElement;
const stateVerse = document.getElementById('state-verse') as HTMLDivElement;
const verseReference = document.getElementById('verse-reference') as HTMLDivElement;
const verseText = document.getElementById('verse-text') as HTMLDivElement;
const verseTranslation = document.getElementById('verse-translation') as HTMLDivElement;

// ─── display helpers ──────────────────────────────────────────────────────────

function showVerse(reference: string, text: string, translation: string): void {
  verseReference.textContent = reference;
  verseText.textContent = text;
  verseTranslation.textContent = translation;

  stateIdle.hidden = true;
  stateVerse.hidden = false;
}

function clearDisplay(): void {
  stateVerse.hidden = true;
  stateIdle.hidden = false;
}

// ─── backend event listeners ──────────────────────────────────────────────────

void listen<AppEvent>('app-event', ({ payload }) => {
  switch (payload.type) {
    case 'VERSE_LOADED': {
      const ref = payload.reference;
      const label = `${ref.book} ${ref.chapter}${ref.verse != null ? ':' + ref.verse : ''}`;
      showVerse(label, payload.text, payload.translation);
      break;
    }

    case 'DISPLAY_CLEARED':
      clearDisplay();
      break;
  }
});
