import { listen } from '@tauri-apps/api/event';
import type { AppEvent } from '@companion-bible/types';

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

// ─── state machine ────────────────────────────────────────────────────────────

type DisplayState = 'idle' | 'blank' | 'verse' | 'title' | 'subpoint';

const panels: Record<DisplayState, HTMLDivElement> = {
  idle: stateIdle,
  blank: stateBlank,
  verse: stateVerse,
  title: stateTitle,
  subpoint: stateSubpoint,
};

let currentState: DisplayState = 'idle';

/**
 * Cross-fade to a new display state.
 *
 * Content MUST be written to the panel's DOM nodes before calling this
 * function so that the panel is never visible mid-update (the panel is at
 * opacity 0 while hidden).  When the next state equals the current one
 * (same-type content swap), the panel fades out, a `transitionend` callback
 * fires the reveal so the content swap is never seen.
 */
function showState(next: DisplayState, update?: () => void): void {
  if (next === currentState && update) {
    // Same panel, new content — hide it, then apply update + reveal once
    // the fade-out has completed.
    const panel = panels[next];
    panel.hidden = true;
    panel.addEventListener(
      'transitionend',
      (e: TransitionEvent) => {
        if (e.propertyName !== 'opacity') return;
        update();
        panel.hidden = false;
      },
      { once: true },
    );
    return;
  }

  // Different state — update content first (panel is at opacity 0), then reveal.
  update?.();
  currentState = next;
  for (const [state, panel] of Object.entries(panels) as [DisplayState, HTMLDivElement][]) {
    panel.hidden = state !== next;
  }
}

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
  }
});
