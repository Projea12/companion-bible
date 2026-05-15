import { describe, it, expect } from 'vitest';
import { BOOK_ALIASES, resolveBook, normalizeBookInput } from './aliases.js';

// ─── Every alias in the map ───────────────────────────────────────────────────

describe('resolveBook — every alias in the map', () => {
  for (const { canonical, aliases } of BOOK_ALIASES) {
    describe(canonical, () => {
      it('resolves the canonical name itself', () => {
        expect(resolveBook(canonical)).toBe(canonical);
      });

      for (const alias of aliases) {
        it(`"${alias}" → "${canonical}"`, () => {
          expect(resolveBook(alias)).toBe(canonical);
        });
      }
    });
  }
});

// ─── Case-insensitive matching ────────────────────────────────────────────────

describe('resolveBook — case-insensitive matching', () => {
  it('all uppercase canonical name', () => {
    expect(resolveBook('GENESIS')).toBe('Genesis');
  });

  it('all lowercase canonical name', () => {
    expect(resolveBook('revelation')).toBe('Revelation');
  });

  it('mixed case canonical name', () => {
    expect(resolveBook('gEnEsIs')).toBe('Genesis');
  });

  it('uppercase abbreviation', () => {
    expect(resolveBook('GEN')).toBe('Genesis');
  });

  it('lowercase abbreviation', () => {
    expect(resolveBook('gen')).toBe('Genesis');
  });

  it('uppercase multi-word', () => {
    expect(resolveBook('SONG OF SOLOMON')).toBe('Song of Solomon');
  });

  it('lowercase multi-word', () => {
    expect(resolveBook('song of solomon')).toBe('Song of Solomon');
  });

  it('uppercase Nigerian form', () => {
    expect(resolveBook('REVELATIONS')).toBe('Revelation');
  });

  it('mixed case Nigerian form', () => {
    expect(resolveBook('Habakuk')).toBe('Habakkuk');
  });

  it('uppercase ordinal prefix', () => {
    expect(resolveBook('FIRST CORINTHIANS')).toBe('1 Corinthians');
  });

  it('lowercase ordinal prefix', () => {
    expect(resolveBook('second samuel')).toBe('2 Samuel');
  });

  it('mixed case ordinal prefix', () => {
    expect(resolveBook('Third John')).toBe('3 John');
  });

  it('roman numeral uppercase', () => {
    expect(resolveBook('II KINGS')).toBe('2 Kings');
  });

  it('roman numeral lowercase', () => {
    expect(resolveBook('iii john')).toBe('3 John');
  });

  it('uppercase abbreviation with dot', () => {
    expect(resolveBook('REV.')).toBe('Revelation');
  });
});

// ─── Whitespace tolerance ─────────────────────────────────────────────────────

describe('resolveBook — whitespace tolerance', () => {
  it('leading spaces', () => {
    expect(resolveBook('   Genesis')).toBe('Genesis');
  });

  it('trailing spaces', () => {
    expect(resolveBook('Genesis   ')).toBe('Genesis');
  });

  it('leading and trailing spaces', () => {
    expect(resolveBook('  Revelation  ')).toBe('Revelation');
  });

  it('multiple internal spaces', () => {
    expect(resolveBook('Song  of  Solomon')).toBe('Song of Solomon');
  });

  it('multiple internal spaces with ordinal', () => {
    expect(resolveBook('1  Corinthians')).toBe('1 Corinthians');
  });

  it('leading spaces with ordinal prefix', () => {
    expect(resolveBook('  First  Timothy  ')).toBe('1 Timothy');
  });

  it('tab character treated as whitespace', () => {
    expect(resolveBook('\tGenesis')).toBe('Genesis');
  });

  it('mixed tabs and spaces', () => {
    expect(resolveBook(' \t Psalms \t ')).toBe('Psalms');
  });

  it('spaces around abbreviation', () => {
    expect(resolveBook('  Gen  ')).toBe('Genesis');
  });

  it('spaces around dot abbreviation', () => {
    expect(resolveBook('  Gen.  ')).toBe('Genesis');
  });

  it('spaces combine with case insensitivity', () => {
    expect(resolveBook('  REVELATION  ')).toBe('Revelation');
  });

  it('spaces in Nigerian form', () => {
    expect(resolveBook('  Revelations  ')).toBe('Revelation');
  });
});

// ─── Dot stripping ────────────────────────────────────────────────────────────

describe('resolveBook — abbreviation dot stripping', () => {
  it('strips single trailing dot', () => {
    expect(resolveBook('Gen.')).toBe('Genesis');
  });

  it('strips dot from NT abbreviation', () => {
    expect(resolveBook('Rev.')).toBe('Revelation');
  });

  it('strips dot from numbered book abbreviation', () => {
    expect(resolveBook('1 Sam.')).toBe('1 Samuel');
  });

  it('strips dot from multi-part abbreviation', () => {
    expect(resolveBook('1 Cor.')).toBe('1 Corinthians');
  });
});

// ─── Negative cases ───────────────────────────────────────────────────────────

describe('resolveBook — unknown inputs return undefined', () => {
  it('empty string', () => {
    expect(resolveBook('')).toBeUndefined();
  });

  it('random word', () => {
    expect(resolveBook('Foobar')).toBeUndefined();
  });

  it('number only', () => {
    expect(resolveBook('42')).toBeUndefined();
  });

  it('partial book name', () => {
    expect(resolveBook('Genesi')).toBeUndefined();
  });
});

// ─── normalizeBookInput unit tests ───────────────────────────────────────────

describe('normalizeBookInput', () => {
  it('lowercases input', () => {
    expect(normalizeBookInput('GENESIS')).toBe('genesis');
  });

  it('trims leading and trailing whitespace', () => {
    expect(normalizeBookInput('  genesis  ')).toBe('genesis');
  });

  it('collapses multiple spaces', () => {
    expect(normalizeBookInput('song  of  solomon')).toBe('song of solomon');
  });

  it('strips dots', () => {
    expect(normalizeBookInput('Gen.')).toBe('gen');
  });

  it('handles tabs as whitespace', () => {
    expect(normalizeBookInput('\tGenesis\t')).toBe('genesis');
  });

  it('combined transformations', () => {
    expect(normalizeBookInput('  1  Sam.  ')).toBe('1 sam');
  });
});
