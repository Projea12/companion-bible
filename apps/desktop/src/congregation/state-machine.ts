export type DisplayState = 'idle' | 'blank' | 'verse' | 'title' | 'subpoint';

export interface StatePanels {
  idle: HTMLElement;
  blank: HTMLElement;
  verse: HTMLElement;
  title: HTMLElement;
  subpoint: HTMLElement;
}

export interface StateMachine {
  showState(this: void, next: DisplayState, update?: () => void): void;
  current(this: void): DisplayState;
}

export function createStateMachine(panels: StatePanels): StateMachine {
  let currentState: DisplayState = 'idle';

  function showState(next: DisplayState, update?: () => void): void {
    if (next === currentState && update) {
      // Same panel, new content: hide → wait for fade-out → swap content → show.
      // Content is never visible while being changed.
      const panel = panels[next];
      panel.hidden = true;
      const handler = (e: TransitionEvent) => {
        if (e.propertyName !== 'opacity') return;
        panel.removeEventListener('transitionend', handler);
        update();
        panel.hidden = false;
      };
      panel.addEventListener('transitionend', handler);
      return;
    }

    // Different state: update content while target panel is at opacity 0, then reveal.
    update?.();
    currentState = next;
    for (const [state, panel] of Object.entries(panels) as [DisplayState, HTMLElement][]) {
      panel.hidden = state !== next;
    }
  }

  return {
    showState,
    current: () => currentState,
  };
}
