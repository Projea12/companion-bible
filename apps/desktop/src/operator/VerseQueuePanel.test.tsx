import { describe, it, expect, vi } from 'vitest';
import { render, screen, fireEvent } from '@testing-library/react';
import { VerseQueuePanel } from './VerseQueuePanel';
import type { QueuedVerse } from './useVerseQueue';
import { QUEUE_EXPIRY_MS } from './useVerseQueue';

// ── helpers ───────────────────────────────────────────────────────────────────

const FUTURE = Date.now() + QUEUE_EXPIRY_MS;

function makeItem(overrides: Partial<QueuedVerse> & { label: string }): QueuedVerse {
  return { id: 1, confidence: 85, expiresAt: FUTURE, ...overrides };
}

const noop = vi.fn();

// ── empty state ───────────────────────────────────────────────────────────────

describe('empty state', () => {
  it('shows listening message when session is active', () => {
    render(<VerseQueuePanel items={[]} sessionActive={true} onConfirm={noop} onReject={noop} />);
    expect(screen.getByText('Listening for scripture references…')).toBeInTheDocument();
  });

  it('shows start-session prompt when session is inactive', () => {
    render(<VerseQueuePanel items={[]} sessionActive={false} onConfirm={noop} onReject={noop} />);
    expect(screen.getByText('Start a session to begin')).toBeInTheDocument();
  });

  it('does not show a pending count badge when queue is empty', () => {
    render(<VerseQueuePanel items={[]} sessionActive={true} onConfirm={noop} onReject={noop} />);
    expect(screen.queryByText(/pending/)).toBeNull();
  });
});

// ── rendering items ───────────────────────────────────────────────────────────

describe('rendering queue items', () => {
  it('renders the reference label', () => {
    const items = [makeItem({ label: 'John 3:16' })];
    render(<VerseQueuePanel items={items} sessionActive={true} onConfirm={noop} onReject={noop} />);
    expect(screen.getByText('John 3:16')).toBeInTheDocument();
  });

  it('shows the confidence percentage', () => {
    const items = [makeItem({ label: 'John 3:16', confidence: 62 })];
    render(<VerseQueuePanel items={items} sessionActive={true} onConfirm={noop} onReject={noop} />);
    expect(screen.getByLabelText('Confidence 62%')).toBeInTheDocument();
  });

  it('shows a pending count badge when items are queued', () => {
    const items = [makeItem({ label: 'John 3:16' }), makeItem({ id: 2, label: 'Romans 8:28' })];
    render(<VerseQueuePanel items={items} sessionActive={true} onConfirm={noop} onReject={noop} />);
    expect(screen.getByText('2 pending')).toBeInTheDocument();
  });

  it('renders a list item for each queued verse', () => {
    const items = [
      makeItem({ id: 1, label: 'John 3:16' }),
      makeItem({ id: 2, label: 'Romans 8:28' }),
    ];
    const { container } = render(
      <VerseQueuePanel items={items} sessionActive={true} onConfirm={noop} onReject={noop} />,
    );
    expect(container.querySelectorAll('.queue-item')).toHaveLength(2);
  });
});

// ── confidence color coding ───────────────────────────────────────────────────

describe('confidence color coding', () => {
  it('applies queue-item--high for confidence ≥ 75', () => {
    const items = [makeItem({ label: 'John 3:16', confidence: 85 })];
    const { container } = render(
      <VerseQueuePanel items={items} sessionActive={true} onConfirm={noop} onReject={noop} />,
    );
    expect(container.querySelector('.queue-item--high')).not.toBeNull();
  });

  it('applies queue-item--medium for confidence 40–74', () => {
    const items = [makeItem({ label: 'John 3:16', confidence: 60 })];
    const { container } = render(
      <VerseQueuePanel items={items} sessionActive={true} onConfirm={noop} onReject={noop} />,
    );
    expect(container.querySelector('.queue-item--medium')).not.toBeNull();
  });

  it('applies queue-item--low for confidence < 40', () => {
    const items = [makeItem({ label: 'John 3:16', confidence: 25 })];
    const { container } = render(
      <VerseQueuePanel items={items} sessionActive={true} onConfirm={noop} onReject={noop} />,
    );
    expect(container.querySelector('.queue-item--low')).not.toBeNull();
  });

  it('renders the level badge text', () => {
    const items = [makeItem({ label: 'John 3:16', confidence: 60 })];
    render(<VerseQueuePanel items={items} sessionActive={true} onConfirm={noop} onReject={noop} />);
    expect(screen.getByText('MEDIUM')).toBeInTheDocument();
  });
});

// ── confirm and reject actions ────────────────────────────────────────────────

describe('confirm and reject', () => {
  it('calls onConfirm with (id, label) when Confirm is clicked', () => {
    const onConfirm = vi.fn();
    const items = [makeItem({ id: 42, label: 'John 3:16' })];
    render(
      <VerseQueuePanel items={items} sessionActive={true} onConfirm={onConfirm} onReject={noop} />,
    );
    fireEvent.click(screen.getByLabelText('Confirm John 3:16'));
    expect(onConfirm).toHaveBeenCalledWith(42, 'John 3:16');
  });

  it('calls onReject with (id, label) when Reject is clicked', () => {
    const onReject = vi.fn();
    const items = [makeItem({ id: 7, label: 'Romans 8:28' })];
    render(
      <VerseQueuePanel items={items} sessionActive={true} onConfirm={noop} onReject={onReject} />,
    );
    fireEvent.click(screen.getByLabelText('Reject Romans 8:28'));
    expect(onReject).toHaveBeenCalledWith(7, 'Romans 8:28');
  });

  it('renders independent confirm/reject buttons for each item', () => {
    const items = [
      makeItem({ id: 1, label: 'John 3:16' }),
      makeItem({ id: 2, label: 'Romans 8:28' }),
    ];
    render(<VerseQueuePanel items={items} sessionActive={true} onConfirm={noop} onReject={noop} />);
    expect(screen.getByLabelText('Confirm John 3:16')).toBeInTheDocument();
    expect(screen.getByLabelText('Confirm Romans 8:28')).toBeInTheDocument();
  });
});

// ── accessibility ─────────────────────────────────────────────────────────────

describe('accessibility', () => {
  it('queue list has an accessible label', () => {
    const items = [makeItem({ label: 'John 3:16' })];
    render(<VerseQueuePanel items={items} sessionActive={true} onConfirm={noop} onReject={noop} />);
    expect(screen.getByRole('list', { name: 'Queued verses' })).toBeInTheDocument();
  });

  it('section has an accessible label', () => {
    render(<VerseQueuePanel items={[]} sessionActive={true} onConfirm={noop} onReject={noop} />);
    expect(screen.getByRole('region', { name: 'Verse queue' })).toBeInTheDocument();
  });

  it('pending count has aria-live for live announcement', () => {
    const items = [makeItem({ label: 'John 3:16' })];
    render(<VerseQueuePanel items={items} sessionActive={true} onConfirm={noop} onReject={noop} />);
    expect(screen.getByText('1 pending')).toHaveAttribute('aria-live', 'polite');
  });
});
