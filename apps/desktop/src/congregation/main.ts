import { listen } from '@tauri-apps/api/event';
import type { AppEvent } from '@companion-bible/types';

// ─── element references ───────────────────────────────────────────────────────

const stateIdle = document.getElementById('state-idle') as HTMLDivElement;
const stateVerse = document.getElementById('state-verse') as HTMLDivElement;
const stateTitle = document.getElementById('state-title') as HTMLDivElement;
const stateSubpoint = document.getElementById('state-subpoint') as HTMLDivElement;
const stateBlank = document.getElementById('state-blank') as HTMLDivElement;
const verseReference = document.getElementById('verse-reference') as HTMLDivElement;
const verseText = document.getElementById('verse-text') as HTMLDivElement;
const verseTranslation = document.getElementById('verse-translation') as HTMLDivElement;
const titleText = document.getElementById('title-text') as HTMLDivElement;
const subpointText = document.getElementById('subpoint-text') as HTMLDivElement;

// ─── state machine ────────────────────────────────────────────────────────────

type DisplayState = 'idle' | 'blank' | 'verse' | 'title' | 'subpoint';

function showState(next: DisplayState): void {
  stateIdle.hidden = next !== 'idle';
  stateBlank.hidden = next !== 'blank';
  stateVerse.hidden = next !== 'verse';
  stateTitle.hidden = next !== 'title';
  stateSubpoint.hidden = next !== 'subpoint';
}

// ─── display helpers ──────────────────────────────────────────────────────────

function showVerse(reference: string, text: string, translation: string): void {
  verseReference.textContent = reference;
  verseText.textContent = text;
  verseTranslation.textContent = translation;
  showState('verse');
}

function showSermonTitle(title: string): void {
  titleText.textContent = title;
  showState('title');
}

function showSubPoint(text: string): void {
  subpointText.textContent = text;
  showState('subpoint');
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

    case 'SERMON_TITLE_SHOWN':
      showSermonTitle(payload.title);
      break;

    case 'SUB_POINT_SHOWN':
      showSubPoint(payload.text);
      break;

    case 'DISPLAY_CLEARED':
      showState('idle');
      break;
  }
});
