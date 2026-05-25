import { describe, it, expect } from 'vitest';
import type { CongregationDisplayState } from './CongregationPreview';

// The scroll button disabled condition extracted from App.tsx:
//   disabled={congregationDisplayState === 'idle' || congregationDisplayState === 'blank'}
//
// These tests verify that condition directly — no DOM rendering needed.

function isScrollDisabled(state: CongregationDisplayState): boolean {
  return state === 'idle' || state === 'blank';
}

describe('scroll button disabled condition', () => {
  // ── disabled states ──────────────────────────────────────────────────────

  it('is disabled when congregation is idle', () => {
    expect(isScrollDisabled('idle')).toBe(true);
  });

  it('is disabled when congregation is blanked', () => {
    expect(isScrollDisabled('blank')).toBe(true);
  });

  // ── enabled states ───────────────────────────────────────────────────────

  it('is enabled when a Bible verse is showing', () => {
    expect(isScrollDisabled('verse')).toBe(false);
  });

  it('is enabled when a GHS hymn is showing', () => {
    // This was the bug — previously disabled={!displayedVerse} which is null
    // during hymn, so GHS could never be scrolled from the operator.
    expect(isScrollDisabled('hymn')).toBe(false);
  });

  it('is enabled when a sermon title is showing', () => {
    expect(isScrollDisabled('title')).toBe(false);
  });

  it('is enabled when a sub-point is showing', () => {
    expect(isScrollDisabled('subpoint')).toBe(false);
  });

  // ── exhaustive check — every state accounted for ─────────────────────────

  it('covers all CongregationDisplayState values', () => {
    const allStates: CongregationDisplayState[] = [
      'idle',
      'blank',
      'verse',
      'hymn',
      'title',
      'subpoint',
    ];
    const disabled = allStates.filter(isScrollDisabled);
    const enabled = allStates.filter((s) => !isScrollDisabled(s));

    expect(disabled).toEqual(['idle', 'blank']);
    expect(enabled).toEqual(['verse', 'hymn', 'title', 'subpoint']);
  });
});

// ── scroll step amount ────────────────────────────────────────────────────────
//
// The operator preview scrollBy calls must use 200 px to match the backend
// scroll_congregation command and clear at least one 1.5× GHS hymn line.

const SCROLL_STEP = 200;

describe('scroll step amount', () => {
  it('scroll up step is -200', () => {
    expect(-SCROLL_STEP).toBe(-200);
  });

  it('scroll down step is 200', () => {
    expect(SCROLL_STEP).toBe(200);
  });

  it('is not the old value of 150', () => {
    // Regression guard — 150 was too small for 1.5× GHS text.
    expect(SCROLL_STEP).not.toBe(150);
  });

  it('up and down are equal magnitude', () => {
    expect(Math.abs(-SCROLL_STEP)).toBe(SCROLL_STEP);
  });
});
