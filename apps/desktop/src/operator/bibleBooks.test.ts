import { describe, it, expect } from 'vitest';
import { suggestBooks, validateReference } from './bibleBooks';

// ── validateReference ─────────────────────────────────────────────────────────

describe('validateReference', () => {
  it('returns empty for empty string', () => {
    expect(validateReference('')).toBe('empty');
  });

  it('returns empty for whitespace-only input', () => {
    expect(validateReference('   ')).toBe('empty');
  });

  it('returns valid for chapter + verse', () => {
    expect(validateReference('John 3:16')).toBe('valid');
  });

  it('returns valid for chapter only', () => {
    expect(validateReference('John 3')).toBe('valid');
  });

  it('returns valid for numbered book with chapter:verse', () => {
    expect(validateReference('1 Corinthians 13:4')).toBe('valid');
  });

  it('returns valid for multi-word book name', () => {
    expect(validateReference('Song of Solomon 1:1')).toBe('valid');
  });

  it('is case-insensitive for book names', () => {
    expect(validateReference('jOHN 3:16')).toBe('valid');
    expect(validateReference('GENESIS 1:1')).toBe('valid');
  });

  it('returns invalid when book is unknown', () => {
    expect(validateReference('Jhn 3:16')).toBe('invalid');
    expect(validateReference('Corinthians 13:4')).toBe('invalid');
  });

  it('returns invalid when no chapter is given', () => {
    expect(validateReference('John')).toBe('invalid');
  });

  it('returns invalid when no book is given', () => {
    expect(validateReference('3:16')).toBe('invalid');
  });

  it('returns invalid for chapter 0', () => {
    expect(validateReference('John 0:1')).toBe('invalid');
  });

  it('returns invalid for verse 0', () => {
    expect(validateReference('John 3:0')).toBe('invalid');
  });

  it('returns valid for large chapter/verse numbers', () => {
    expect(validateReference('Psalms 119:176')).toBe('valid');
  });
});

// ── suggestBooks ──────────────────────────────────────────────────────────────

describe('suggestBooks', () => {
  it('returns empty array for empty input', () => {
    expect(suggestBooks('')).toEqual([]);
  });

  it('returns empty array for whitespace-only input', () => {
    expect(suggestBooks('   ')).toEqual([]);
  });

  it('returns matching books for a prefix', () => {
    const result = suggestBooks('Gen');
    expect(result).toContain('Genesis');
  });

  it('is case-insensitive', () => {
    expect(suggestBooks('gen')).toContain('Genesis');
    expect(suggestBooks('GEN')).toContain('Genesis');
  });

  it('returns multiple matches for a shared prefix', () => {
    const result = suggestBooks('Jo');
    expect(result).toContain('Job');
    expect(result).toContain('Joel');
    expect(result).toContain('Jonah');
    expect(result).toContain('Joshua');
    expect(result).toContain('John');
  });

  it('returns at most 6 suggestions', () => {
    expect(suggestBooks('1').length).toBeLessThanOrEqual(6);
  });

  it('returns empty when no books match', () => {
    expect(suggestBooks('xyz')).toEqual([]);
  });

  it('returns empty when user has typed a valid book + chapter', () => {
    expect(suggestBooks('John 3')).toEqual([]);
  });

  it('returns empty when user has typed numbered book + chapter', () => {
    expect(suggestBooks('1 Corinthians 13')).toEqual([]);
  });

  it('still suggests when user has typed a book name but no chapter yet', () => {
    const result = suggestBooks('John');
    expect(result).toContain('John');
  });

  it('suggests numbered books correctly', () => {
    const result = suggestBooks('1 Co');
    expect(result).toContain('1 Corinthians');
  });

  it('returns empty for a space followed only by digits', () => {
    expect(suggestBooks('John 3:16')).toEqual([]);
  });

  it('returns empty when input is an exact book name followed by a space', () => {
    expect(suggestBooks('John ')).toEqual([]);
  });
});
