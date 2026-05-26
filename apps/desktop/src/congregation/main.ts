import { listen } from '@tauri-apps/api/event';
import type { AppEvent } from '@companion-bible/types';
import { createStateMachine } from './state-machine';

// ─── element references ───────────────────────────────────────────────────────

const stateIdle = document.getElementById('state-idle') as HTMLDivElement;
const stateVerse = document.getElementById('state-verse') as HTMLDivElement;
const stateTitle = document.getElementById('state-title') as HTMLDivElement;
const stateSubpoint = document.getElementById('state-subpoint') as HTMLDivElement;
const stateBlank = document.getElementById('state-blank') as HTMLDivElement;
const stateHymn = document.getElementById('state-hymn') as HTMLDivElement;
const stateAnnouncement = document.getElementById('state-announcement') as HTMLDivElement;

const verseReference = document.getElementById('verse-reference') as HTMLElement;
const verseText = document.getElementById('verse-text') as HTMLElement;
const verseTranslation = document.getElementById('verse-translation') as HTMLElement;
const titleText = document.getElementById('title-text') as HTMLElement;
const subpointText = document.getElementById('subpoint-text') as HTMLElement;
const hymnNumberLabel = document.getElementById('hymn-number-label') as HTMLElement;
const hymnSectionLabel = document.getElementById('hymn-section-label') as HTMLElement;
const hymnTitle = document.getElementById('hymn-title') as HTMLElement;
const hymnLines = document.getElementById('hymn-lines') as HTMLElement;
const announcementBodyWrap = document.getElementById('announcement-body-wrap') as HTMLDivElement;
const announcementBody = document.getElementById('announcement-body') as HTMLDivElement;
const serviceLabel = document.getElementById('service-label') as HTMLDivElement;
const serviceLabelText = document.getElementById('service-label-text') as HTMLElement;

// ─── state machine ────────────────────────────────────────────────────────────

const { showState, current: currentState } = createStateMachine({
  idle: stateIdle,
  blank: stateBlank,
  verse: stateVerse,
  title: stateTitle,
  subpoint: stateSubpoint,
  hymn: stateHymn,
  announcement: stateAnnouncement,
});

// ─── display helpers ──────────────────────────────────────────────────────────

// ─── verse auto-scroll ────────────────────────────────────────────────────────

let verseScrollRaf = 0;

function stopVerseScroll(): void {
  cancelAnimationFrame(verseScrollRaf);
}

function startVerseScroll(): void {
  stopVerseScroll();

  // 30 px/s — slow enough to read comfortably on a large screen.
  const PX_PER_MS = 30 / 1000;
  let lastTime: number | null = null;

  function step(now: number): void {
    const maxScroll = stateVerse.scrollHeight - stateVerse.clientHeight;
    if (maxScroll <= 0) return; // text fits — nothing to scroll

    if (lastTime !== null) {
      stateVerse.scrollTop = Math.min(
        stateVerse.scrollTop + PX_PER_MS * (now - lastTime),
        maxScroll,
      );
    }
    lastTime = now;

    if (stateVerse.scrollTop < maxScroll) {
      verseScrollRaf = requestAnimationFrame(step);
    }
  }

  verseScrollRaf = requestAnimationFrame(step);
}

function showVerse(reference: string, text: string, translation: string): void {
  stopVerseScroll();
  stateVerse.scrollTop = 0;
  showState('verse', () => {
    verseReference.textContent = reference;
    verseText.textContent = text;
    verseTranslation.textContent = translation;
  });
  // --cb-fade is 300 ms. Wait 700 ms so the panel is fully visible and the
  // browser has painted the final layout before we measure scrollHeight.
  // (transitionend is unreliable here because two properties transition
  // simultaneously — opacity and transform — and { once: true } consumes
  // the listener on the first one fired, which may not be opacity.)
  setTimeout(startVerseScroll, 700);
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

const HYMN_FS_MAX = 144;
const HYMN_FS_MIN = 40;
const HYMN_FS_STEP = 2;

function fitHymnText(): void {
  stateHymn.style.removeProperty('--hymn-fs');
  requestAnimationFrame(() => {
    const card = stateHymn.querySelector('.hymn-card');
    if (!card) return;
    let size = Math.min(HYMN_FS_MAX, Math.round(window.innerHeight * 0.055));
    stateHymn.style.setProperty('--hymn-fs', `${String(size)}px`);
    requestAnimationFrame(function shrink() {
      if (card.scrollHeight > stateHymn.clientHeight && size > HYMN_FS_MIN) {
        size = Math.max(HYMN_FS_MIN, size - HYMN_FS_STEP);
        stateHymn.style.setProperty('--hymn-fs', `${String(size)}px`);
        requestAnimationFrame(shrink);
      }
    });
  });
}

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
  fitHymnText();
}

// Smoothly scroll the announcement body over the slide duration so long text
// behaves like a news broadcast — slow linear scroll from top to bottom.
let announcementScrollRaf = 0;

function startAnnouncementScroll(durationMs: number): void {
  cancelAnimationFrame(announcementScrollRaf);
  announcementBodyWrap.scrollTop = 0;

  const startTime = performance.now();

  function step(now: number): void {
    const maxScroll = announcementBodyWrap.scrollHeight - announcementBodyWrap.clientHeight;
    if (maxScroll <= 0) return;

    const progress = Math.min((now - startTime) / durationMs, 1);
    announcementBodyWrap.scrollTop = maxScroll * progress;

    if (progress < 1) {
      announcementScrollRaf = requestAnimationFrame(step);
    }
  }

  announcementScrollRaf = requestAnimationFrame(step);
}

// Flip the text only — the logo and background never move.
function flipAnnouncementText(body: string, durationSecs: number): void {
  cancelAnimationFrame(announcementScrollRaf);

  announcementBody.classList.remove('ann-flip-in');
  announcementBody.classList.add('ann-flip-out');

  announcementBody.addEventListener(
    'animationend',
    () => {
      announcementBody.classList.remove('ann-flip-out');
      announcementBody.textContent = body;
      announcementBodyWrap.scrollTop = 0;
      announcementBody.classList.add('ann-flip-in');

      announcementBody.addEventListener(
        'animationend',
        () => {
          announcementBody.classList.remove('ann-flip-in');
          startAnnouncementScroll(durationSecs * 1000);
        },
        { once: true },
      );
    },
    { once: true },
  );
}

function showAnnouncement(body: string, durationSecs: number): void {
  if (currentState() === 'announcement') {
    // Already on the announcement screen — only the text changes.
    flipAnnouncementText(body, durationSecs);
    return;
  }
  // First entry into announcement state — transition the whole panel in once.
  showState('announcement', () => {
    announcementBody.textContent = body;
    announcementBodyWrap.scrollTop = 0;
  });
  setTimeout(() => startAnnouncementScroll(durationSecs * 1000), 800);
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
      stopVerseScroll();
      showState('blank');
      break;

    case 'DISPLAY_CLEARED':
      stopVerseScroll();
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

    case 'ANNOUNCEMENT_SHOWN':
      showAnnouncement(payload.body, payload.duration_secs);
      break;

    case 'ANNOUNCEMENTS_STOPPED':
      cancelAnimationFrame(announcementScrollRaf);
      showState('idle');
      break;

    case 'SERVICE_ITEM_CHANGED':
      if (payload.label) {
        serviceLabelText.textContent = payload.label;
        serviceLabel.classList.add('visible');
      } else {
        serviceLabel.classList.remove('visible');
      }
      break;

    case 'CONGREGATION_SCROLL': {
      const panels: Partial<Record<string, HTMLElement>> = {
        verse: stateVerse,
        title: stateTitle,
        subpoint: stateSubpoint,
        hymn: stateHymn,
        announcement: announcementBodyWrap,
      };
      const active = panels[currentState()];
      if (active) active.scrollBy({ top: payload.amount, behavior: 'smooth' });
      break;
    }
  }
});
