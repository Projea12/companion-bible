import { describe, it, expect, vi, beforeEach } from 'vitest';
import { render, screen } from '@testing-library/react';
import { TranscriptPanel } from './TranscriptPanel';
import { HighlightedText } from './TranscriptPanel';
import type { TranscriptLine } from './useTranscript';

// jsdom does not implement scrollIntoView; stub it globally.
beforeEach(() => {
  HTMLElement.prototype.scrollIntoView = vi.fn();
});

// ── helpers ───────────────────────────────────────────────────────────────────

function makeLine(overrides: Partial<TranscriptLine> & { text: string }, id = 1): TranscriptLine {
  return { id, chunkId: id, detectedRef: null, ...overrides };
}

// ── empty state ───────────────────────────────────────────────────────────────

describe('empty state', () => {
  it('shows idle message when session is inactive', () => {
    render(<TranscriptPanel lines={[]} sessionActive={false} />);
    expect(screen.getByText('Start a session to see transcript')).toBeInTheDocument();
  });

  it('shows waiting message when session is active but no audio yet', () => {
    render(<TranscriptPanel lines={[]} sessionActive={true} />);
    expect(screen.getByText('Waiting for audio…')).toBeInTheDocument();
  });
});

// ── rendering lines ───────────────────────────────────────────────────────────

describe('rendering transcript lines', () => {
  it('renders each line text', () => {
    const lines = [
      makeLine({ text: 'He opened to John chapter 3' }, 1),
      makeLine({ text: 'For God so loved the world' }, 2),
    ];
    render(<TranscriptPanel lines={lines} sessionActive={true} />);
    expect(screen.getByText('He opened to John chapter 3')).toBeInTheDocument();
    expect(screen.getByText('For God so loved the world')).toBeInTheDocument();
  });

  it('has role=log for screen-reader live region', () => {
    render(<TranscriptPanel lines={[]} sessionActive={false} />);
    expect(screen.getByRole('log')).toBeInTheDocument();
  });

  it('has aria-live=polite on the log region', () => {
    render(<TranscriptPanel lines={[]} sessionActive={false} />);
    expect(screen.getByRole('log')).toHaveAttribute('aria-live', 'polite');
  });

  it('applies transcript-line class to each paragraph', () => {
    const lines = [makeLine({ text: 'line one' }, 1)];
    const { container } = render(<TranscriptPanel lines={lines} sessionActive={true} />);
    expect(container.querySelector('.transcript-line')).not.toBeNull();
  });

  it('adds transcript-line--detected modifier when line has a ref', () => {
    const lines = [makeLine({ text: 'John 3:16 says…', detectedRef: 'John 3:16' }, 1)];
    const { container } = render(<TranscriptPanel lines={lines} sessionActive={true} />);
    expect(container.querySelector('.transcript-line--detected')).not.toBeNull();
  });

  it('does not add detected modifier for unmatched lines', () => {
    const lines = [makeLine({ text: 'no reference here' }, 1)];
    const { container } = render(<TranscriptPanel lines={lines} sessionActive={true} />);
    expect(container.querySelector('.transcript-line--detected')).toBeNull();
  });
});

// ── reference badge ───────────────────────────────────────────────────────────

describe('reference badge', () => {
  it('renders a badge when detectedRef is set', () => {
    const lines = [makeLine({ text: 'John 3:16 says…', detectedRef: 'John 3:16' }, 1)];
    render(<TranscriptPanel lines={lines} sessionActive={true} />);
    const badge = screen.getByText('John 3:16', { selector: '.transcript-ref-badge' });
    expect(badge).toBeInTheDocument();
  });

  it('does not render a badge when detectedRef is null', () => {
    const lines = [makeLine({ text: 'no reference here' }, 1)];
    const { container } = render(<TranscriptPanel lines={lines} sessionActive={true} />);
    expect(container.querySelector('.transcript-ref-badge')).toBeNull();
  });
});

// ── auto-scroll ───────────────────────────────────────────────────────────────

describe('auto-scroll', () => {
  it('calls scrollIntoView when lines are first rendered', () => {
    const lines = [makeLine({ text: 'first line' }, 1)];
    render(<TranscriptPanel lines={lines} sessionActive={true} />);
    // eslint-disable-next-line @typescript-eslint/unbound-method
    expect(HTMLElement.prototype.scrollIntoView).toHaveBeenCalled();
  });
});

// ── HighlightedText ───────────────────────────────────────────────────────────

describe('HighlightedText', () => {
  it('renders plain text when highlight is null', () => {
    render(
      <p>
        <HighlightedText text="Hello world" highlight={null} />
      </p>,
    );
    expect(screen.getByText('Hello world')).toBeInTheDocument();
    expect(document.querySelector('mark')).toBeNull();
  });

  it('wraps matched text in a <mark> element', () => {
    render(
      <p>
        <HighlightedText text="He read John 3:16 aloud" highlight="John 3:16" />
      </p>,
    );
    const mark = document.querySelector('mark');
    expect(mark).not.toBeNull();
    expect(mark?.textContent).toBe('John 3:16');
  });

  it('applies transcript-highlight class to <mark>', () => {
    render(
      <p>
        <HighlightedText text="John 3:16" highlight="John 3:16" />
      </p>,
    );
    expect(document.querySelector('mark.transcript-highlight')).not.toBeNull();
  });

  it('preserves text before and after the highlighted span', () => {
    const { container } = render(
      <p>
        <HighlightedText text="He read John 3:16 aloud" highlight="John 3:16" />
      </p>,
    );
    expect(container.textContent).toBe('He read John 3:16 aloud');
  });

  it('is case-insensitive when finding the highlight position', () => {
    render(
      <p>
        <HighlightedText text="JOHN 3:16 was quoted" highlight="John 3:16" />
      </p>,
    );
    const mark = document.querySelector('mark');
    expect(mark).not.toBeNull();
    expect(mark?.textContent).toBe('JOHN 3:16');
  });

  it('renders plain text when the highlight is not found in the string', () => {
    render(
      <p>
        <HighlightedText text="Romans 8:28" highlight="John 3:16" />
      </p>,
    );
    expect(document.querySelector('mark')).toBeNull();
    expect(screen.getByText('Romans 8:28')).toBeInTheDocument();
  });
});
