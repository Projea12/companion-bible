// @vitest-environment node
// CSS tests read from disk — no DOM needed.

/**
 * CSS specification tests — parsed from the actual source files.
 *
 * These tests lock down the design contract: background colour, font-family
 * declarations, transition properties, and the font-size clamp() bounds that
 * keep text legible at 10 metres.
 *
 * 10-metre legibility rule of thumb: at 1080p projected to 10 m, a glyph must
 * be ≥ 28 px on-screen to be readable without effort.  Maximum values cap at
 * 100 px so text never overflows a single line on wide displays.
 */

/// <reference types="node" />
import { describe, it, expect } from 'vitest';
import { readFileSync } from 'node:fs';
import { resolve } from 'node:path';
import { fileURLToPath } from 'node:url';

const __dirname = fileURLToPath(new URL('.', import.meta.url));
const ROOT = resolve(__dirname, '../..');

function css(file: string): string {
  return readFileSync(resolve(ROOT, file), 'utf-8');
}

const congregationCss = css('src/congregation/congregation.css');
const fontsCss = css('src/shared/fonts.css');

// ── helpers ───────────────────────────────────────────────────────────────────

/**
 * Extract all clamp(min, pref, max) triples for a given CSS property name.
 * Returns parsed numbers [minPx, maxPx].
 */
function extractClampBounds(source: string, propName: string): [number, number][] {
  const re = new RegExp(`${propName}\\s*:[^;]*clamp\\(([^)]+)\\)`, 'g');
  const results: [number, number][] = [];
  let m: RegExpExecArray | null;
  while ((m = re.exec(source)) !== null) {
    const [minStr, , maxStr] = m[1].split(',').map((s) => s.trim());
    const minPx = parseFloat(minStr);
    const maxPx = parseFloat(maxStr);
    results.push([minPx, maxPx]);
  }
  return results;
}

// ── background & FOUC guard ───────────────────────────────────────────────────

describe('background colour', () => {
  it('html element gets the midnight navy background', () => {
    // CSS uses the token var(--cb-bg); the token itself resolves to #080d1a.
    expect(congregationCss).toMatch(/html\s*\{[^}]*background\s*:\s*var\(--cb-bg\)/s);
    expect(congregationCss).toContain('--cb-bg: #080d1a');
  });

  it('congregation-body uses the same background variable', () => {
    expect(congregationCss).toMatch(
      /\.congregation-body\s*\{[^}]*background\s*:\s*var\(--cb-bg\)/s,
    );
  });

  it('--cb-bg token resolves to #080d1a', () => {
    expect(congregationCss).toContain('--cb-bg: #080d1a');
  });

  it('congregation-blank uses --cb-bg so it matches the body background', () => {
    expect(congregationCss).toMatch(
      /\.congregation-blank\s*\{[^}]*background\s*:\s*var\(--cb-bg\)/s,
    );
  });
});

// ── page-enter animation ──────────────────────────────────────────────────────

describe('page-enter animation (FOUC prevention)', () => {
  it('page-enter keyframe is defined', () => {
    expect(congregationCss).toContain('@keyframes page-enter');
  });

  it('page-enter starts at opacity 0', () => {
    expect(congregationCss).toMatch(
      /@keyframes page-enter\s*\{[^}]*from\s*\{[^}]*opacity\s*:\s*0/s,
    );
  });

  it('congregation-body applies page-enter animation', () => {
    expect(congregationCss).toMatch(/\.congregation-body\s*\{[^}]*animation:[^;]*page-enter/s);
  });
});

// ── cross-fade transition ─────────────────────────────────────────────────────

describe('cross-fade transition', () => {
  it('fade duration token is 300ms', () => {
    expect(congregationCss).toContain('--cb-fade: 300ms');
  });

  it('congregation-state transitions opacity', () => {
    expect(congregationCss).toMatch(/\.congregation-state\s*\{[^}]*transition[^;]*opacity/s);
  });

  it('congregation-state transitions transform', () => {
    expect(congregationCss).toMatch(/\.congregation-state\s*\{[^}]*transition[^;]*transform/s);
  });

  it('congregation-state uses will-change for GPU compositing', () => {
    expect(congregationCss).toMatch(/\.congregation-state\s*\{[^}]*will-change[^;]*opacity/s);
  });

  it('hidden panels have opacity 0', () => {
    expect(congregationCss).toMatch(/\.congregation-state\[hidden\]\s*\{[^}]*opacity\s*:\s*0/s);
  });

  it('hidden panels override display:none with display:flex', () => {
    expect(congregationCss).toMatch(
      /\.congregation-state\[hidden\]\s*\{[^}]*display\s*:\s*flex\s*!important/s,
    );
  });

  it('hidden panels have pointer-events: none', () => {
    expect(congregationCss).toMatch(
      /\.congregation-state\[hidden\]\s*\{[^}]*pointer-events\s*:\s*none/s,
    );
  });

  it('incoming panel lifts in via translateY on hidden state', () => {
    expect(congregationCss).toMatch(/\.congregation-state\[hidden\]\s*\{[^}]*translateY/s);
  });
});

// ── font declarations ─────────────────────────────────────────────────────────

describe('font declarations (fonts.css)', () => {
  it('CongregationSerif is declared as a font-face', () => {
    expect(fontsCss).toContain("font-family: 'CongregationSerif'");
  });

  it('CongregationSans is declared as a font-face', () => {
    expect(fontsCss).toContain("font-family: 'CongregationSans'");
  });

  it('all font sources use local() — no network URLs', () => {
    // Strip comment block, then check no http/https URLs remain.
    const noComments = fontsCss.replace(/\/\*[\s\S]*?\*\//g, '');
    expect(noComments).not.toMatch(/https?:\/\//);
  });

  it('font-display: swap is set on every @font-face', () => {
    const blocks = fontsCss.match(/@font-face\s*\{[^}]+\}/gs) ?? [];
    expect(blocks.length).toBeGreaterThan(0);
    for (const block of blocks) {
      expect(block).toContain('font-display: swap');
    }
  });

  it('CongregationSerif regular weight uses Palatino as first local source', () => {
    expect(fontsCss).toMatch(/font-family:\s*'CongregationSerif'[^}]*local\('Palatino'\)/s);
  });

  it('CongregationSerif falls back to Georgia', () => {
    expect(fontsCss).toMatch(/font-family:\s*'CongregationSerif'[^}]*local\('Georgia'\)/s);
  });

  it('CongregationSans covers weights 400 500 600 700', () => {
    const weights = [400, 500, 600, 700];
    for (const w of weights) {
      expect(fontsCss).toMatch(
        new RegExp(`font-family:\\s*'CongregationSans'[^}]*font-weight:\\s*${w}`, 's'),
      );
    }
  });
});

describe('font families in congregation.css', () => {
  it('--cb-serif token references CongregationSerif', () => {
    expect(congregationCss).toContain("'CongregationSerif'");
  });

  it('--cb-sans token references CongregationSans', () => {
    expect(congregationCss).toContain("'CongregationSans'");
  });

  it('verse-text uses the serif token', () => {
    expect(congregationCss).toMatch(/\.verse-text\s*\{[^}]*font-family\s*:\s*var\(--cb-serif\)/s);
  });

  it('title-text uses the sans token', () => {
    expect(congregationCss).toMatch(/\.title-text\s*\{[^}]*font-family\s*:\s*var\(--cb-sans\)/s);
  });
});

// ── font-size legibility at 10 m ──────────────────────────────────────────────

describe('font-size legibility at 10 metres', () => {
  const MIN_READABLE_PX = 28;
  const MAX_SAFE_PX = 110;

  it('verse text clamp min is ≥ 28 px (readable at 10 m)', () => {
    const bounds = extractClampBounds(congregationCss, '--size-verse');
    expect(bounds.length).toBeGreaterThan(0);
    for (const [min] of bounds) {
      expect(min).toBeGreaterThanOrEqual(MIN_READABLE_PX);
    }
  });

  it('verse reference clamp min is ≥ 16 px', () => {
    const bounds = extractClampBounds(congregationCss, '--size-ref');
    expect(bounds.length).toBeGreaterThan(0);
    for (const [min] of bounds) {
      expect(min).toBeGreaterThanOrEqual(16);
    }
  });

  it('title clamp min is ≥ 36 px', () => {
    const bounds = extractClampBounds(congregationCss, '--size-title');
    expect(bounds.length).toBeGreaterThan(0);
    for (const [min] of bounds) {
      expect(min).toBeGreaterThanOrEqual(36);
    }
  });

  it('subpoint clamp min is ≥ 22 px', () => {
    const bounds = extractClampBounds(congregationCss, '--size-subpoint');
    expect(bounds.length).toBeGreaterThan(0);
    for (const [min] of bounds) {
      expect(min).toBeGreaterThanOrEqual(22);
    }
  });

  it('verse text clamp max does not exceed safe upper bound', () => {
    const bounds = extractClampBounds(congregationCss, '--size-verse');
    for (const [, max] of bounds) {
      expect(max).toBeLessThanOrEqual(MAX_SAFE_PX);
    }
  });

  it('title clamp max does not exceed safe upper bound', () => {
    const bounds = extractClampBounds(congregationCss, '--size-title');
    for (const [, max] of bounds) {
      expect(max).toBeLessThanOrEqual(MAX_SAFE_PX);
    }
  });
});

// ── accent colour ─────────────────────────────────────────────────────────────

describe('accent colour', () => {
  it('--cb-accent is the warm gold value', () => {
    expect(congregationCss).toContain('--cb-accent: #c9a96e');
  });

  it('verse-reference uses the accent colour', () => {
    expect(congregationCss).toMatch(/\.verse-reference\s*\{[^}]*color\s*:\s*var\(--cb-accent\)/s);
  });

  it('title-eyebrow uses the accent colour', () => {
    expect(congregationCss).toMatch(/\.title-eyebrow\s*\{[^}]*color\s*:\s*var\(--cb-accent\)/s);
  });

  it('subpoint left border uses the accent colour', () => {
    expect(congregationCss).toMatch(/\.subpoint-card\s*\{[^}]*border-left[^;]*var\(--cb-accent\)/s);
  });
});

// ── reduced-motion ────────────────────────────────────────────────────────────

describe('reduced-motion media query', () => {
  it('prefers-reduced-motion media query is present', () => {
    expect(congregationCss).toContain('prefers-reduced-motion: reduce');
  });

  it('page-enter animation is disabled under reduced-motion', () => {
    expect(congregationCss).toMatch(
      /prefers-reduced-motion[^}]*\{[^}]*\.congregation-body\s*\{[^}]*animation\s*:\s*none/s,
    );
  });

  it('transform is neutralised under reduced-motion', () => {
    // Extract everything after the @media open-brace and search within it,
    // because nested braces make a single [^}]* pattern fail.
    const afterMedia = congregationCss.split('prefers-reduced-motion')[1] ?? '';
    expect(afterMedia).toMatch(/transform\s*:\s*none\s*!important/s);
  });
});
