import { describe, it, expect, vi } from 'vitest';
import { createStateMachine, type DisplayState, type StatePanels } from './state-machine';

// ── helpers ───────────────────────────────────────────────────────────────────

function makePanel(hidden = true): HTMLElement {
  const el = document.createElement('div');
  el.hidden = hidden;
  return el;
}

function makePanels(): StatePanels {
  return {
    idle: makePanel(false), // idle starts visible
    blank: makePanel(),
    verse: makePanel(),
    title: makePanel(),
    subpoint: makePanel(),
    hymn: makePanel(),
    announcement: makePanel(),
  };
}

function fireTransitionEnd(el: HTMLElement, propertyName = 'opacity'): void {
  el.dispatchEvent(new TransitionEvent('transitionend', { bubbles: true, propertyName }));
}

// ── initial state ─────────────────────────────────────────────────────────────

describe('initial state', () => {
  it('reports idle as current state', () => {
    const sm = createStateMachine(makePanels());
    expect(sm.current()).toBe('idle');
  });
});

// ── cross-state transitions ───────────────────────────────────────────────────

describe('cross-state transitions', () => {
  const transitions: [DisplayState, DisplayState][] = [
    ['idle', 'verse'],
    ['idle', 'title'],
    ['idle', 'subpoint'],
    ['idle', 'blank'],
    ['verse', 'idle'],
    ['verse', 'title'],
    ['verse', 'subpoint'],
    ['verse', 'blank'],
    ['title', 'verse'],
    ['title', 'subpoint'],
    ['title', 'blank'],
    ['title', 'idle'],
    ['subpoint', 'verse'],
    ['subpoint', 'title'],
    ['subpoint', 'blank'],
    ['subpoint', 'idle'],
    ['blank', 'verse'],
    ['blank', 'title'],
    ['blank', 'subpoint'],
    ['blank', 'idle'],
  ];

  it.each(transitions)('%s → %s: only target panel is visible', (from, to) => {
    const panels = makePanels();
    const sm = createStateMachine(panels);

    // Arrive at the `from` state first.
    if (from !== 'idle') sm.showState(from);
    sm.showState(to);

    // Only the target panel must be un-hidden.
    for (const [state, panel] of Object.entries(panels) as [DisplayState, HTMLElement][]) {
      expect(panel.hidden).toBe(state !== to);
    }
  });

  it.each(transitions)('%s → %s: current() updates', (from, to) => {
    const sm = createStateMachine(makePanels());
    if (from !== 'idle') sm.showState(from);
    sm.showState(to);
    expect(sm.current()).toBe(to);
  });
});

// ── no partial text rendering ─────────────────────────────────────────────────

describe('no partial text rendering', () => {
  it('update callback fires before the target panel is un-hidden', () => {
    const panels = makePanels();
    const sm = createStateMachine(panels);
    const calls: string[] = [];

    sm.showState('verse', () => {
      // At the moment update() is called, verse must still be hidden.
      expect(panels.verse.hidden).toBe(true);
      calls.push('update');
    });

    expect(calls).toEqual(['update']);
    // After showState returns, verse is now visible.
    expect(panels.verse.hidden).toBe(false);
  });

  it('update fires before panel reveals for all content states', () => {
    const states: DisplayState[] = ['verse', 'title', 'subpoint'];
    for (const state of states) {
      const panels = makePanels();
      const sm = createStateMachine(panels);
      let updateFiredWhileHidden = false;

      sm.showState(state, () => {
        updateFiredWhileHidden = panels[state].hidden;
      });

      expect(updateFiredWhileHidden).toBe(true);
    }
  });

  it('showState without update does not crash', () => {
    const sm = createStateMachine(makePanels());
    expect(() => sm.showState('blank')).not.toThrow();
  });
});

// ── same-state content swap (verse → verse, title → title, etc.) ──────────────

describe('same-state content swap', () => {
  it('hides the panel immediately when swapping same-state content', () => {
    const panels = makePanels();
    const sm = createStateMachine(panels);

    sm.showState('verse'); // arrive at verse
    expect(panels.verse.hidden).toBe(false);

    sm.showState('verse', () => {
      /* new content */
    });

    // Panel must be hidden immediately so content swap is invisible.
    expect(panels.verse.hidden).toBe(true);
  });

  it('update fires only after the opacity transitionend fires', () => {
    const panels = makePanels();
    const sm = createStateMachine(panels);
    sm.showState('verse');

    const updateSpy = vi.fn();
    sm.showState('verse', updateSpy);

    // Before transitionend: update must not have been called.
    expect(updateSpy).not.toHaveBeenCalled();

    fireTransitionEnd(panels.verse, 'opacity');
    expect(updateSpy).toHaveBeenCalledOnce();
  });

  it('panel becomes visible again after transitionend + update', () => {
    const panels = makePanels();
    const sm = createStateMachine(panels);
    sm.showState('verse');

    sm.showState('verse', () => {
      /* update content */
    });
    expect(panels.verse.hidden).toBe(true);

    fireTransitionEnd(panels.verse, 'opacity');
    expect(panels.verse.hidden).toBe(false);
  });

  it('ignores transitionend events for non-opacity properties', () => {
    const panels = makePanels();
    const sm = createStateMachine(panels);
    sm.showState('verse');

    const updateSpy = vi.fn();
    sm.showState('verse', updateSpy);

    // transform transitionend fires first — must be ignored.
    fireTransitionEnd(panels.verse, 'transform');
    expect(updateSpy).not.toHaveBeenCalled();

    // opacity transitionend fires — triggers the swap.
    fireTransitionEnd(panels.verse, 'opacity');
    expect(updateSpy).toHaveBeenCalledOnce();
  });

  it('same-state swap works for title panel', () => {
    const panels = makePanels();
    const sm = createStateMachine(panels);
    sm.showState('title');

    const updateSpy = vi.fn();
    sm.showState('title', updateSpy);
    expect(panels.title.hidden).toBe(true);
    expect(updateSpy).not.toHaveBeenCalled();

    fireTransitionEnd(panels.title, 'opacity');
    expect(updateSpy).toHaveBeenCalledOnce();
    expect(panels.title.hidden).toBe(false);
  });

  it('same-state swap works for subpoint panel', () => {
    const panels = makePanels();
    const sm = createStateMachine(panels);
    sm.showState('subpoint');

    const updateSpy = vi.fn();
    sm.showState('subpoint', updateSpy);
    expect(panels.subpoint.hidden).toBe(true);

    fireTransitionEnd(panels.subpoint, 'opacity');
    expect(updateSpy).toHaveBeenCalledOnce();
    expect(panels.subpoint.hidden).toBe(false);
  });
});

// ── mutual exclusivity ────────────────────────────────────────────────────────

describe('mutual exclusivity', () => {
  it('only one panel is visible at any time', () => {
    const panels = makePanels();
    const sm = createStateMachine(panels);
    const sequence: DisplayState[] = ['verse', 'title', 'subpoint', 'blank', 'idle', 'verse'];

    for (const state of sequence) {
      sm.showState(state);
      const visibleCount = (Object.values(panels) as HTMLElement[]).filter((p) => !p.hidden).length;
      expect(visibleCount).toBe(1);
    }
  });

  it('blank state hides all content panels', () => {
    const panels = makePanels();
    const sm = createStateMachine(panels);

    sm.showState('verse');
    sm.showState('blank');

    expect(panels.verse.hidden).toBe(true);
    expect(panels.title.hidden).toBe(true);
    expect(panels.subpoint.hidden).toBe(true);
    expect(panels.idle.hidden).toBe(true);
    expect(panels.blank.hidden).toBe(false);
  });
});

// ── full service flow ─────────────────────────────────────────────────────────

describe('full service flow', () => {
  it('idle → title → verse → subpoint → blank → idle', () => {
    const panels = makePanels();
    const sm = createStateMachine(panels);

    expect(sm.current()).toBe('idle');

    sm.showState('title', () => {
      /* sermon title set */
    });
    expect(sm.current()).toBe('title');
    expect(panels.title.hidden).toBe(false);

    sm.showState('verse', () => {
      /* verse set */
    });
    expect(sm.current()).toBe('verse');
    expect(panels.verse.hidden).toBe(false);
    expect(panels.title.hidden).toBe(true);

    sm.showState('subpoint', () => {
      /* subpoint set */
    });
    expect(sm.current()).toBe('subpoint');

    sm.showState('blank');
    expect(sm.current()).toBe('blank');
    expect(panels.blank.hidden).toBe(false);

    sm.showState('idle');
    expect(sm.current()).toBe('idle');
    expect(panels.idle.hidden).toBe(false);
    expect(panels.blank.hidden).toBe(true);
  });
});
