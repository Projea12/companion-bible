import { listen } from '@tauri-apps/api/event';
import type { AppEvent } from '@companion-bible/types';
import { createStateMachine } from './state-machine';

// ─── element references ───────────────────────────────────────────────────────

const stateIdle = document.getElementById('state-idle') as HTMLDivElement;
const stateVerse = document.getElementById('state-verse') as HTMLDivElement;
const stateTitle = document.getElementById('state-title') as HTMLDivElement;
const stateSubpoint = document.getElementById('state-subpoint') as HTMLDivElement;
const stateBlank = document.getElementById('state-blank') as HTMLDivElement;

const verseReference = document.getElementById('verse-reference') as HTMLElement;
const verseText = document.getElementById('verse-text') as HTMLElement;
const verseTranslation = document.getElementById('verse-translation') as HTMLElement;
const titleText = document.getElementById('title-text') as HTMLElement;
const subpointText = document.getElementById('subpoint-text') as HTMLElement;
const stateHymn = document.getElementById('state-hymn') as HTMLDivElement;
const hymnNumberLabel = document.getElementById('hymn-number-label') as HTMLElement;
const hymnSectionLabel = document.getElementById('hymn-section-label') as HTMLElement;
const hymnTitle = document.getElementById('hymn-title') as HTMLElement;
const hymnLines = document.getElementById('hymn-lines') as HTMLElement;

// ─── state machine ────────────────────────────────────────────────────────────

const { showState } = createStateMachine({
  idle: stateIdle,
  blank: stateBlank,
  verse: stateVerse,
  title: stateTitle,
  subpoint: stateSubpoint,
  hymn: stateHymn,
});

// ─── display helpers ──────────────────────────────────────────────────────────

function showVerse(reference: string, text: string, translation: string): void {
  showState('verse', () => {
    verseReference.textContent = reference;
    verseText.textContent = text;
    verseTranslation.textContent = translation;
  });
}

function showSermonTitle(title: string): void {
  showState('title', () => {
    titleText.textContent = title;
  });
}

function showSubPoint(text: string): void {
  showState('subpoint', () => {
    subpointText.textContent = text;
  });
}

let activeHymnTitle = '';

function showHymnSection(
  number: number,
  stanzaNumber: number | null,
  isChorus: boolean,
  lines: string[],
): void {
  showState('hymn', () => {
    hymnNumberLabel.textContent = `GHS ${String(number)}`;
    hymnSectionLabel.textContent = isChorus ? 'Chorus' : `Stanza ${String(stanzaNumber ?? '')}`;
    hymnTitle.textContent = activeHymnTitle;
    hymnLines.innerHTML = '';
    for (const line of lines) {
      const p = document.createElement('p');
      p.className = 'hymn-line';
      p.textContent = line;
      hymnLines.appendChild(p);
    }
  });
}

// ─── backend event listeners ──────────────────────────────────────────────────

void listen<AppEvent>('app-event', ({ payload }) => {
  switch (payload.type) {
    case 'VERSE_LOADED': {
      const ref = payload.reference;
      const label = `${ref.book} ${ref.chapter}${ref.verse != null ? ':' + String(ref.verse) : ''}`;
      showVerse(label, payload.text, payload.translation);
      break;
    }

    case 'SERMON_TITLE_SHOWN':
      showSermonTitle(payload.title);
      break;

    case 'SUB_POINT_SHOWN':
      showSubPoint(payload.text);
      break;

    case 'DISPLAY_BLANKED':
      showState('blank');
      break;

    case 'DISPLAY_CLEARED':
      showState('idle');
      break;

    case 'HYMN_DETECTED':
      activeHymnTitle = payload.title;
      break;

    case 'HYMN_SECTION_ADVANCED':
      showHymnSection(payload.number, payload.stanza_number, payload.is_chorus, payload.lines);
      break;

    case 'HYMN_COMPLETED':
      showState('idle');
      break;
  }
});
