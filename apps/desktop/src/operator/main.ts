import { invoke } from '@tauri-apps/api/core';
import { listen } from '@tauri-apps/api/event';
import type { AppEvent, AppState } from '@companion-bible/types';

// ─── element references ───────────────────────────────────────────────────────

const btnStart = document.getElementById('btn-start-session') as HTMLButtonElement;
const btnStop = document.getElementById('btn-stop-session') as HTMLButtonElement;
const btnToggleCongregation = document.getElementById(
  'btn-toggle-congregation',
) as HTMLButtonElement;
const btnApprove = document.getElementById('btn-approve') as HTMLButtonElement;
const btnReject = document.getElementById('btn-reject') as HTMLButtonElement;
const btnClear = document.getElementById('btn-clear') as HTMLButtonElement;
const detectionStatus = document.getElementById('detection-status') as HTMLDivElement;
const currentReference = document.getElementById('current-reference') as HTMLDivElement;
const referenceText = document.getElementById('reference-text') as HTMLDivElement;
const historyList = document.getElementById('detection-history') as HTMLUListElement;
const connectionStatus = document.getElementById('connection-status') as HTMLDivElement;
const screenStatus = document.getElementById('screen-status') as HTMLDivElement;

// ─── state ────────────────────────────────────────────────────────────────────

let congregationVisible = false;
let pendingReference: string | null = null;

// ─── startup sync ─────────────────────────────────────────────────────────────

function applyAppState(s: AppState): void {
  const hasSecondary = s.hasSecondaryScreen;
  btnToggleCongregation.disabled = !hasSecondary;
  screenStatus.textContent = `${s.totalScreens} screen${s.totalScreens === 1 ? '' : 's'}`;
  screenStatus.className = `screen-status ${hasSecondary ? 'screen-dual' : 'screen-single'}`;

  congregationVisible = s.congregationVisible;
  btnToggleCongregation.textContent = congregationVisible
    ? 'Hide Congregation Window'
    : 'Show Congregation Window';

  if (s.sessionActive) {
    btnStart.hidden = true;
    btnStop.hidden = false;
    setDetectionStatus('active', 'Listening…');
  }
}

void invoke('get_app_state').then((raw) => applyAppState(raw as AppState));

// ─── session controls ─────────────────────────────────────────────────────────

btnStart.addEventListener('click', () => {
  void invoke('start_session').then(() => {
    btnStart.hidden = true;
    btnStop.hidden = false;
    setDetectionStatus('active', 'Listening…');
    setActionButtons(false);
  });
});

btnStop.addEventListener('click', () => {
  void invoke('stop_session').then(() => {
    btnStart.hidden = false;
    btnStop.hidden = true;
    setDetectionStatus('idle', 'Idle');
    setActionButtons(false, true);
  });
});

// ─── congregation window ──────────────────────────────────────────────────────

btnToggleCongregation.addEventListener('click', () => {
  if (congregationVisible) {
    void invoke('hide_congregation_window').then(() => {
      congregationVisible = false;
      btnToggleCongregation.textContent = 'Show Congregation Window';
    });
  } else {
    void invoke('show_congregation_window').then(() => {
      congregationVisible = true;
      btnToggleCongregation.textContent = 'Hide Congregation Window';
    });
  }
});

// ─── operator actions ─────────────────────────────────────────────────────────

btnApprove.addEventListener('click', () => {
  if (!pendingReference) return;
  const ref = pendingReference;
  // show_verse fetches and emits VERSE_LOADED + VERSE_DISPLAYED; text lookup
  // will be filled by the Bible package in a later task.
  void invoke('show_verse', { reference: ref, text: '' }).then(() => {
    addHistoryItem(ref, 'approved');
    pendingReference = null;
    setActionButtons(false, true);
    currentReference.hidden = true;
  });
});

btnReject.addEventListener('click', () => {
  if (!pendingReference) return;
  const ref = pendingReference;
  void invoke('discard_verse').then(() => {
    addHistoryItem(ref, 'rejected');
    pendingReference = null;
    setActionButtons(false, true);
    currentReference.hidden = true;
    setDetectionStatus('active', 'Listening…');
  });
});

btnClear.addEventListener('click', () => {
  void invoke('discard_verse');
});

// ─── backend event listeners ──────────────────────────────────────────────────

void listen<AppEvent>('app-event', ({ payload }) => {
  switch (payload.type) {
    case 'SECONDARY_SCREEN_CONNECTED':
    case 'SECONDARY_SCREEN_DISCONNECTED':
      void invoke('get_app_state').then((raw) => applyAppState(raw as AppState));
      break;

    case 'SCRIPTURE_REFERENCE_DETECTED': {
      const ref = payload.references[0];
      if (!ref) break;
      const label = `${ref.book} ${ref.chapter}${ref.verse != null ? ':' + ref.verse : ''}`;
      pendingReference = label;
      referenceText.textContent = label;
      setDetectionStatus('pending', 'Pending Review');
      currentReference.hidden = false;
      setActionButtons(true);
      break;
    }

    case 'VERSE_DISPLAYED': {
      const ref = payload.reference;
      const label = `${ref.book} ${ref.chapter}${ref.verse != null ? ':' + ref.verse : ''}`;
      setDetectionStatus('active', `Displaying: ${label}`);
      btnClear.disabled = false;
      break;
    }

    case 'DISPLAY_CLEARED':
      btnClear.disabled = true;
      setDetectionStatus('active', 'Listening…');
      break;

    case 'INTERNET_CONNECTED':
      connectionStatus.textContent = 'Online';
      connectionStatus.classList.add('online');
      break;

    case 'INTERNET_DISCONNECTED':
      connectionStatus.textContent = 'Offline';
      connectionStatus.classList.remove('online');
      break;
  }
});

// ─── helpers ──────────────────────────────────────────────────────────────────

function setDetectionStatus(state: 'idle' | 'active' | 'pending', label: string): void {
  detectionStatus.className = `detection-status status-${state}`;
  detectionStatus.querySelector('.status-label')!.textContent = label;
}

function setActionButtons(enabled: boolean, disableAll = false): void {
  btnApprove.disabled = disableAll || !enabled;
  btnReject.disabled = disableAll || !enabled;
  btnClear.disabled = disableAll;
}

function addHistoryItem(reference: string, outcome: 'approved' | 'rejected'): void {
  const empty = historyList.querySelector('.history-empty');
  if (empty) empty.remove();

  const li = document.createElement('li');
  li.className = `history-item ${outcome}`;
  li.innerHTML = `
    <span class="ref">${reference}</span>
    <span class="conf">${outcome === 'approved' ? '✓ Approved' : '✗ Rejected'}</span>
  `;
  historyList.prepend(li);
}
