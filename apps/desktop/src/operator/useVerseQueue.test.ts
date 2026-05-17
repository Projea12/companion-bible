import { describe, it, expect } from 'vitest';
import { renderHook, act } from '@testing-library/react';
import { useVerseQueue, confidenceLevel, QUEUE_EXPIRY_MS } from './useVerseQueue';

// ── confidenceLevel ───────────────────────────────────────────────────────────

describe('confidenceLevel', () => {
  it('returns high for 75 and above', () => {
    expect(confidenceLevel(75)).toBe('high');
    expect(confidenceLevel(100)).toBe('high');
    expect(confidenceLevel(85)).toBe('high');
  });

  it('returns medium for 40–74', () => {
    expect(confidenceLevel(40)).toBe('medium');
    expect(confidenceLevel(60)).toBe('medium');
    expect(confidenceLevel(74)).toBe('medium');
  });

  it('returns low for below 40', () => {
    expect(confidenceLevel(0)).toBe('low');
    expect(confidenceLevel(39)).toBe('low');
  });
});

// ── initial state ─────────────────────────────────────────────────────────────

describe('useVerseQueue — initial state', () => {
  it('starts with an empty queue', () => {
    const { result } = renderHook(() => useVerseQueue());
    expect(result.current.items).toEqual([]);
  });
});

// ── enqueue ───────────────────────────────────────────────────────────────────

describe('useVerseQueue — enqueue', () => {
  it('adds a verse to the queue', () => {
    const { result } = renderHook(() => useVerseQueue());
    act(() => result.current.enqueue('John 3:16', 85));
    expect(result.current.items).toHaveLength(1);
    expect(result.current.items[0]?.label).toBe('John 3:16');
    expect(result.current.items[0]?.confidence).toBe(85);
  });

  it('sets expiresAt to now + QUEUE_EXPIRY_MS', () => {
    const now = 1_000_000;
    const { result } = renderHook(() => useVerseQueue());
    act(() => result.current.enqueue('John 3:16', 85, now));
    expect(result.current.items[0]?.expiresAt).toBe(now + QUEUE_EXPIRY_MS);
  });

  it('assigns unique ids to each item', () => {
    const { result } = renderHook(() => useVerseQueue());
    act(() => {
      result.current.enqueue('John 3:16', 85);
      result.current.enqueue('Romans 8:28', 60);
    });
    const [a, b] = result.current.items;
    expect(a?.id).not.toBe(b?.id);
  });

  it('ignores a duplicate label already in the queue', () => {
    const { result } = renderHook(() => useVerseQueue());
    act(() => {
      result.current.enqueue('John 3:16', 85);
      result.current.enqueue('John 3:16', 90);
    });
    expect(result.current.items).toHaveLength(1);
  });

  it('allows the same label again after the first was removed', () => {
    const { result } = renderHook(() => useVerseQueue());
    act(() => result.current.enqueue('John 3:16', 85));
    const id = result.current.items[0].id;
    act(() => result.current.remove(id));
    act(() => result.current.enqueue('John 3:16', 85));
    expect(result.current.items).toHaveLength(1);
  });

  it('preserves insertion order', () => {
    const { result } = renderHook(() => useVerseQueue());
    act(() => {
      result.current.enqueue('John 3:16', 85);
      result.current.enqueue('Romans 8:28', 60);
      result.current.enqueue('Psalm 23:1', 30);
    });
    expect(result.current.items.map((v) => v.label)).toEqual([
      'John 3:16',
      'Romans 8:28',
      'Psalm 23:1',
    ]);
  });
});

// ── remove ────────────────────────────────────────────────────────────────────

describe('useVerseQueue — remove', () => {
  it('removes the item with the given id', () => {
    const { result } = renderHook(() => useVerseQueue());
    act(() => result.current.enqueue('John 3:16', 85));
    const id = result.current.items[0].id;
    act(() => result.current.remove(id));
    expect(result.current.items).toEqual([]);
  });

  it('leaves other items untouched', () => {
    const { result } = renderHook(() => useVerseQueue());
    act(() => {
      result.current.enqueue('John 3:16', 85);
      result.current.enqueue('Romans 8:28', 60);
    });
    const id = result.current.items[0].id;
    act(() => result.current.remove(id));
    expect(result.current.items).toHaveLength(1);
    expect(result.current.items[0]?.label).toBe('Romans 8:28');
  });

  it('is a no-op for an unknown id', () => {
    const { result } = renderHook(() => useVerseQueue());
    act(() => result.current.enqueue('John 3:16', 85));
    act(() => result.current.remove(9999));
    expect(result.current.items).toHaveLength(1);
  });
});

// ── expireOld ─────────────────────────────────────────────────────────────────

describe('useVerseQueue — expireOld', () => {
  it('removes items whose expiresAt ≤ now', () => {
    const past = 1_000;
    const future = Date.now() + 60_000;
    const { result } = renderHook(() => useVerseQueue());

    act(() => {
      // expired: expiresAt = past + QUEUE_EXPIRY_MS; we expire with now = far future
      result.current.enqueue('John 3:16', 85, past);
      result.current.enqueue('Romans 8:28', 60, future);
    });

    // Expire with a timestamp well past the first item's expiresAt
    act(() => result.current.expireOld(past + QUEUE_EXPIRY_MS + 1));

    expect(result.current.items).toHaveLength(1);
    expect(result.current.items[0]?.label).toBe('Romans 8:28');
  });

  it('keeps items whose expiresAt > now', () => {
    const now = Date.now();
    const { result } = renderHook(() => useVerseQueue());
    act(() => result.current.enqueue('John 3:16', 85, now));
    // expire just before expiry
    act(() => result.current.expireOld(now + QUEUE_EXPIRY_MS - 1));
    expect(result.current.items).toHaveLength(1);
  });

  it('removes all items when all are expired', () => {
    const past = 1_000;
    const { result } = renderHook(() => useVerseQueue());
    act(() => {
      result.current.enqueue('John 3:16', 85, past);
      result.current.enqueue('Romans 8:28', 60, past);
    });
    act(() => result.current.expireOld(past + QUEUE_EXPIRY_MS + 1));
    expect(result.current.items).toEqual([]);
  });

  it('is a no-op on an empty queue', () => {
    const { result } = renderHook(() => useVerseQueue());
    act(() => result.current.expireOld(Date.now() + 1_000_000));
    expect(result.current.items).toEqual([]);
  });
});

// ── clear ─────────────────────────────────────────────────────────────────────

describe('useVerseQueue — clear', () => {
  it('removes all items', () => {
    const { result } = renderHook(() => useVerseQueue());
    act(() => {
      result.current.enqueue('John 3:16', 85);
      result.current.enqueue('Romans 8:28', 60);
    });
    act(() => result.current.clear());
    expect(result.current.items).toEqual([]);
  });

  it('can enqueue again after clearing', () => {
    const { result } = renderHook(() => useVerseQueue());
    act(() => result.current.enqueue('John 3:16', 85));
    act(() => result.current.clear());
    act(() => result.current.enqueue('Romans 8:28', 60));
    expect(result.current.items).toHaveLength(1);
    expect(result.current.items[0]?.label).toBe('Romans 8:28');
  });
});
