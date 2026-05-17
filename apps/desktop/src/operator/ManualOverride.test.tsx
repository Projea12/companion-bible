import { describe, it, expect, vi } from 'vitest';
import { render, screen, fireEvent, waitFor } from '@testing-library/react';
import { ManualOverride } from './ManualOverride';

// ── helpers ───────────────────────────────────────────────────────────────────

function renderOverride(onSubmit = vi.fn()) {
  render(<ManualOverride onSubmit={onSubmit} />);
  const input = screen.getByRole('combobox', { name: 'Manual reference override' });
  return { input, onSubmit };
}

function type(input: HTMLElement, value: string) {
  fireEvent.change(input, { target: { value } });
}

// ── autocomplete suggestions ──────────────────────────────────────────────────

describe('autocomplete suggestions', () => {
  it('shows no suggestions initially', () => {
    renderOverride();
    expect(screen.queryByRole('listbox')).toBeNull();
  });

  it('shows suggestion list when typing a book prefix', () => {
    const { input } = renderOverride();
    type(input, 'Jo');
    expect(screen.getByRole('listbox')).toBeInTheDocument();
  });

  it('lists matching books', () => {
    const { input } = renderOverride();
    type(input, 'Jo');
    expect(screen.getByText('John')).toBeInTheDocument();
    expect(screen.getByText('Job')).toBeInTheDocument();
  });

  it('hides suggestions once chapter is typed', () => {
    const { input } = renderOverride();
    type(input, 'John 3');
    expect(screen.queryByRole('listbox')).toBeNull();
  });

  it('clicking a suggestion fills the input with book name + space', () => {
    const { input } = renderOverride();
    type(input, 'Jo');
    fireEvent.mouseDown(screen.getByText('John'));
    expect((input as HTMLInputElement).value).toBe('John ');
  });

  it('closes the suggestion list after a suggestion is clicked', () => {
    const { input } = renderOverride();
    type(input, 'Jo');
    fireEvent.mouseDown(screen.getByText('John'));
    expect(screen.queryByRole('listbox')).toBeNull();
  });

  it('ArrowDown highlights the first suggestion', () => {
    const { input } = renderOverride();
    type(input, 'Jo');
    fireEvent.keyDown(input, { key: 'ArrowDown' });
    const options = screen.getAllByRole('option');
    expect(options[0]).toHaveAttribute('aria-selected', 'true');
  });

  it('pressing Enter on a highlighted suggestion fills the input', () => {
    const { input } = renderOverride();
    type(input, 'Jo');
    fireEvent.keyDown(input, { key: 'ArrowDown' });
    fireEvent.keyDown(input, { key: 'Enter' });
    expect((input as HTMLInputElement).value).toMatch(/ $/);
  });

  it('pressing Escape closes the suggestion list', () => {
    const { input } = renderOverride();
    type(input, 'Jo');
    fireEvent.keyDown(input, { key: 'Escape' });
    expect(screen.queryByRole('listbox')).toBeNull();
  });
});

// ── validation feedback ───────────────────────────────────────────────────────

describe('validation feedback', () => {
  it('shows no validation icon when input is empty', () => {
    renderOverride();
    expect(screen.queryByText('✓')).toBeNull();
    expect(screen.queryByText('✗')).toBeNull();
  });

  it('shows green checkmark for a valid reference', () => {
    const { input } = renderOverride();
    type(input, 'John 3:16');
    const icon = screen.getByText('✓');
    expect(icon).toBeInTheDocument();
    expect(icon).toHaveAttribute('data-state', 'valid');
  });

  it('shows red cross for an invalid reference', () => {
    const { input } = renderOverride();
    type(input, 'Jhn 3:16');
    const icon = screen.getByText('✗');
    expect(icon).toBeInTheDocument();
    expect(icon).toHaveAttribute('data-state', 'invalid');
  });

  it('input carries data-validation attribute when non-empty', () => {
    const { input } = renderOverride();
    type(input, 'John 3:16');
    expect(input).toHaveAttribute('data-validation', 'valid');
  });

  it('input carries data-validation=invalid for unknown book', () => {
    const { input } = renderOverride();
    type(input, 'Xyz 1:1');
    expect(input).toHaveAttribute('data-validation', 'invalid');
  });
});

// ── submit behaviour ──────────────────────────────────────────────────────────

describe('submit', () => {
  it('Show button is disabled when input is empty', () => {
    renderOverride();
    expect(screen.getByRole('button', { name: 'Show' })).toBeDisabled();
  });

  it('Show button is disabled when reference is invalid', () => {
    const { input } = renderOverride();
    type(input, 'Jhn 3:16');
    expect(screen.getByRole('button', { name: 'Show' })).toBeDisabled();
  });

  it('Show button is enabled when reference is valid', () => {
    const { input } = renderOverride();
    type(input, 'John 3:16');
    expect(screen.getByRole('button', { name: 'Show' })).toBeEnabled();
  });

  it('calls onSubmit with the trimmed reference when Show is clicked', () => {
    const { input, onSubmit } = renderOverride();
    type(input, 'John 3:16');
    fireEvent.click(screen.getByRole('button', { name: 'Show' }));
    expect(onSubmit).toHaveBeenCalledWith('John 3:16');
  });

  it('calls onSubmit when Enter is pressed and reference is valid', () => {
    const { input, onSubmit } = renderOverride();
    type(input, 'John 3:16');
    fireEvent.keyDown(input, { key: 'Enter' });
    expect(onSubmit).toHaveBeenCalledWith('John 3:16');
  });

  it('does not call onSubmit when Enter is pressed and reference is invalid', () => {
    const { input, onSubmit } = renderOverride();
    type(input, 'Jhn 3:16');
    fireEvent.keyDown(input, { key: 'Enter' });
    expect(onSubmit).not.toHaveBeenCalled();
  });

  it('clears the input after a successful submit', async () => {
    const { input } = renderOverride();
    type(input, 'John 3:16');
    fireEvent.click(screen.getByRole('button', { name: 'Show' }));
    await waitFor(() => {
      expect((input as HTMLInputElement).value).toBe('');
    });
  });
});

// ── accessibility ─────────────────────────────────────────────────────────────

describe('accessibility', () => {
  it('section has an accessible label', () => {
    renderOverride();
    expect(screen.getByRole('region', { name: 'Manual override' })).toBeInTheDocument();
  });

  it('suggestion list has an accessible label', () => {
    const { input } = renderOverride();
    type(input, 'Jo');
    expect(screen.getByRole('listbox', { name: 'Book suggestions' })).toBeInTheDocument();
  });
});
