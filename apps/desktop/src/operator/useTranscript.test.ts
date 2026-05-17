import { describe, it, expect } from 'vitest';
import { renderHook, act } from '@testing-library/react';
import { useTranscript, MAX_TRANSCRIPT_LINES, matchesSource } from './useTranscript';

// ── matchesSource ─────────────────────────────────────────────────────────────

describe('matchesSource', () => {
  it('matches identical strings', () => {
    expect(matchesSource('John 3:16', 'John 3:16')).toBe(true);
  });

  it('matches when line contains source', () => {
    expect(matchesSource('He said John 3:16 to the crowd', 'John 3:16')).toBe(true);
  });

  it('matches when source contains line', () => {
    expect(matchesSource('John 3:16', 'He quoted John 3:16 here')).toBe(true);
  });

  it('returns false for unrelated strings', () => {
    expect(matchesSource('Romans 8:28', 'John 3:16')).toBe(false);
  });

  it('returns false for empty strings', () => {
    expect(matchesSource('', 'John 3:16')).toBe(false);
    expect(matchesSource('John 3:16', '')).toBe(false);
  });
});

// ── useTranscript ─────────────────────────────────────────────────────────────

describe('useTranscript — initial state', () => {
  it('starts with an empty lines array', () => {
    const { result } = renderHook(() => useTranscript());
    expect(result.current.lines).toEqual([]);
  });
});

describe('useTranscript — addLine', () => {
  it('adds a line with the given text', () => {
    const { result } = renderHook(() => useTranscript());
    act(() => result.current.addLine(1, 'Hello world'));
    expect(result.current.lines).toHaveLength(1);
    expect(result.current.lines[0]?.text).toBe('Hello world');
  });

  it('tracks the chunkId on each line', () => {
    const { result } = renderHook(() => useTranscript());
    act(() => result.current.addLine(42, 'some text'));
    expect(result.current.lines[0]?.chunkId).toBe(42);
  });

  it('initialises detectedRef to null', () => {
    const { result } = renderHook(() => useTranscript());
    act(() => result.current.addLine(1, 'text'));
    expect(result.current.lines[0]?.detectedRef).toBeNull();
  });

  it('assigns unique ascending ids', () => {
    const { result } = renderHook(() => useTranscript());
    act(() => {
      result.current.addLine(1, 'a');
      result.current.addLine(2, 'b');
    });
    const [first, second] = result.current.lines;
    expect(first?.id).toBeLessThan(second.id);
  });

  it(`caps at ${MAX_TRANSCRIPT_LINES} lines`, () => {
    const { result } = renderHook(() => useTranscript());
    act(() => {
      for (let i = 0; i < MAX_TRANSCRIPT_LINES + 3; i++) {
        result.current.addLine(i, `line ${i}`);
      }
    });
    expect(result.current.lines).toHaveLength(MAX_TRANSCRIPT_LINES);
  });

  it('evicts the oldest line when over capacity', () => {
    const { result } = renderHook(() => useTranscript());
    act(() => {
      for (let i = 0; i < MAX_TRANSCRIPT_LINES + 1; i++) {
        result.current.addLine(i, `line ${i}`);
      }
    });
    expect(result.current.lines[0]?.text).toBe('line 1');
    expect(result.current.lines[MAX_TRANSCRIPT_LINES - 1]?.text).toBe(
      `line ${MAX_TRANSCRIPT_LINES}`,
    );
  });
});

describe('useTranscript — markDetection', () => {
  it('annotates the matching line with a ref label', () => {
    const { result } = renderHook(() => useTranscript());
    act(() => result.current.addLine(1, 'He read John 3:16 aloud'));
    act(() => result.current.markDetection('John 3:16', 'John 3:16'));
    expect(result.current.lines[0]?.detectedRef).toBe('John 3:16');
  });

  it('leaves non-matching lines unchanged', () => {
    const { result } = renderHook(() => useTranscript());
    act(() => {
      result.current.addLine(1, 'Romans 8:28 is a comfort');
      result.current.addLine(2, 'He read John 3:16 aloud');
    });
    act(() => result.current.markDetection('John 3:16', 'John 3:16'));
    expect(result.current.lines[0]?.detectedRef).toBeNull();
    expect(result.current.lines[1]?.detectedRef).toBe('John 3:16');
  });

  it('does not mutate existing lines without a match', () => {
    const { result } = renderHook(() => useTranscript());
    act(() => result.current.addLine(1, 'unrelated sentence'));
    act(() => result.current.markDetection('John 3:16', 'John 3:16'));
    expect(result.current.lines[0]?.detectedRef).toBeNull();
  });

  it('works when source_text is wider than the line', () => {
    const { result } = renderHook(() => useTranscript());
    act(() => result.current.addLine(1, 'John 3:16'));
    act(() => result.current.markDetection('He quoted John 3:16 here', 'John 3:16'));
    expect(result.current.lines[0]?.detectedRef).toBe('John 3:16');
  });
});

describe('useTranscript — clear', () => {
  it('removes all lines', () => {
    const { result } = renderHook(() => useTranscript());
    act(() => {
      result.current.addLine(1, 'a');
      result.current.addLine(2, 'b');
    });
    act(() => result.current.clear());
    expect(result.current.lines).toEqual([]);
  });

  it('can add lines again after clearing', () => {
    const { result } = renderHook(() => useTranscript());
    act(() => result.current.addLine(1, 'before'));
    act(() => result.current.clear());
    act(() => result.current.addLine(2, 'after'));
    expect(result.current.lines).toHaveLength(1);
    expect(result.current.lines[0]?.text).toBe('after');
  });
});
