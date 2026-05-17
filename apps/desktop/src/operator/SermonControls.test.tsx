import { describe, it, expect, vi } from 'vitest';
import { render, screen, fireEvent, within } from '@testing-library/react';
import { SermonControls } from './SermonControls';
import type { SermonControlsProps, SermonSetup } from './SermonControls';

// ── helpers ───────────────────────────────────────────────────────────────────

function defaults(overrides: Partial<SermonControlsProps> = {}): SermonControlsProps {
  return {
    sermonActive: false,
    subPoints: [],
    subPointIndex: -1,
    onStartService: vi.fn(),
    onEndService: vi.fn(),
    onAddSubPoint: vi.fn(),
    onNextSubPoint: vi.fn(),
    ...overrides,
  };
}

function renderControls(overrides: Partial<SermonControlsProps> = {}) {
  const props = defaults(overrides);
  render(<SermonControls {...props} />);
  return props;
}

function openStartDialog() {
  fireEvent.click(screen.getByRole('button', { name: 'Start Service' }));
  return screen.getByRole('dialog', { name: 'Start Service' });
}

function openEndDialog() {
  fireEvent.click(screen.getByRole('button', { name: 'End Service' }));
  return screen.getByRole('dialog', { name: 'End Service' });
}

// ── before service starts ─────────────────────────────────────────────────────

describe('before service starts', () => {
  it('shows Start Service button', () => {
    renderControls();
    expect(screen.getByRole('button', { name: 'Start Service' })).toBeInTheDocument();
  });

  it('does not show sub-point input', () => {
    renderControls();
    expect(screen.queryByLabelText('Sub-point text')).toBeNull();
  });

  it('does not show End Service button', () => {
    renderControls();
    expect(screen.queryByRole('button', { name: 'End Service' })).toBeNull();
  });

  it('does not show Next button', () => {
    renderControls();
    expect(screen.queryByRole('button', { name: 'Advance to next sub-point' })).toBeNull();
  });

  it('section has an accessible label', () => {
    renderControls();
    expect(screen.getByRole('region', { name: 'Sermon controls' })).toBeInTheDocument();
  });
});

// ── start service dialog ──────────────────────────────────────────────────────

describe('Start Service dialog', () => {
  it('opens when Start Service is clicked', () => {
    renderControls();
    openStartDialog();
    expect(screen.getByRole('dialog', { name: 'Start Service' })).toBeInTheDocument();
  });

  it('has inputs for title, pastor, and anchor scripture', () => {
    renderControls();
    const dialog = openStartDialog();
    expect(within(dialog).getByLabelText('Title')).toBeInTheDocument();
    expect(within(dialog).getByLabelText('Pastor')).toBeInTheDocument();
    expect(within(dialog).getByLabelText('Anchor Scripture')).toBeInTheDocument();
  });

  it('Cancel closes dialog without calling onStartService', () => {
    const props = renderControls();
    const dialog = openStartDialog();
    fireEvent.click(within(dialog).getByRole('button', { name: 'Cancel' }));
    expect(screen.queryByRole('dialog')).toBeNull();
    expect(props.onStartService).not.toHaveBeenCalled();
  });

  it('Escape closes dialog without calling onStartService', () => {
    const props = renderControls();
    openStartDialog();
    fireEvent.keyDown(document.body, { key: 'Escape', bubbles: true });
    expect(screen.queryByRole('dialog')).toBeNull();
    expect(props.onStartService).not.toHaveBeenCalled();
  });

  it('clicking backdrop closes dialog without calling onStartService', () => {
    const props = renderControls();
    openStartDialog();
    fireEvent.click(screen.getByRole('presentation'));
    expect(screen.queryByRole('dialog')).toBeNull();
    expect(props.onStartService).not.toHaveBeenCalled();
  });

  it('calls onStartService with form values on confirm', () => {
    const props = renderControls();
    const dialog = openStartDialog();
    fireEvent.change(within(dialog).getByLabelText('Title'), {
      target: { value: 'Grace & Truth' },
    });
    fireEvent.change(within(dialog).getByLabelText('Pastor'), {
      target: { value: 'Pastor John' },
    });
    fireEvent.change(within(dialog).getByLabelText('Anchor Scripture'), {
      target: { value: 'John 1:14' },
    });
    fireEvent.click(within(dialog).getByRole('button', { name: 'Begin Service' }));
    expect(props.onStartService).toHaveBeenCalledWith<[SermonSetup]>({
      title: 'Grace & Truth',
      pastor: 'Pastor John',
      anchorScripture: 'John 1:14',
    });
  });

  it('calls onStartService with empty strings when no inputs are filled', () => {
    const props = renderControls();
    const dialog = openStartDialog();
    fireEvent.click(within(dialog).getByRole('button', { name: 'Begin Service' }));
    expect(props.onStartService).toHaveBeenCalledWith({
      title: '',
      pastor: '',
      anchorScripture: '',
    });
  });

  it('dialog closes after confirming start', () => {
    renderControls();
    const dialog = openStartDialog();
    fireEvent.click(within(dialog).getByRole('button', { name: 'Begin Service' }));
    expect(screen.queryByRole('dialog')).toBeNull();
  });
});

// ── when service is active ────────────────────────────────────────────────────

describe('when service is active', () => {
  it('shows End Service button', () => {
    renderControls({ sermonActive: true });
    expect(screen.getByRole('button', { name: 'End Service' })).toBeInTheDocument();
  });

  it('shows sub-point input and Add button', () => {
    renderControls({ sermonActive: true });
    expect(screen.getByLabelText('Sub-point text')).toBeInTheDocument();
    expect(screen.getByRole('button', { name: 'Add sub-point' })).toBeInTheDocument();
  });

  it('shows Next sub-point button', () => {
    renderControls({ sermonActive: true });
    expect(screen.getByRole('button', { name: 'Advance to next sub-point' })).toBeInTheDocument();
  });

  it('does not show Start Service button', () => {
    renderControls({ sermonActive: true });
    expect(screen.queryByRole('button', { name: 'Start Service' })).toBeNull();
  });
});

// ── sub-point controls ────────────────────────────────────────────────────────

describe('sub-point controls', () => {
  it('Add button is disabled when input is empty', () => {
    renderControls({ sermonActive: true });
    expect(screen.getByRole('button', { name: 'Add sub-point' })).toBeDisabled();
  });

  it('Add button is enabled when input has text', () => {
    renderControls({ sermonActive: true });
    fireEvent.change(screen.getByLabelText('Sub-point text'), {
      target: { value: 'The Cost of Discipleship' },
    });
    expect(screen.getByRole('button', { name: 'Add sub-point' })).toBeEnabled();
  });

  it('calls onAddSubPoint with trimmed text when Add is clicked', () => {
    const props = renderControls({ sermonActive: true });
    fireEvent.change(screen.getByLabelText('Sub-point text'), {
      target: { value: '  Faith in Action  ' },
    });
    fireEvent.click(screen.getByRole('button', { name: 'Add sub-point' }));
    expect(props.onAddSubPoint).toHaveBeenCalledWith('Faith in Action');
  });

  it('clears the input after adding', () => {
    renderControls({ sermonActive: true });
    const input = screen.getByLabelText('Sub-point text');
    fireEvent.change(input, { target: { value: 'Point 1' } });
    fireEvent.click(screen.getByRole('button', { name: 'Add sub-point' }));
    expect((input as HTMLInputElement).value).toBe('');
  });

  it('calls onAddSubPoint when Enter is pressed in the input', () => {
    const props = renderControls({ sermonActive: true });
    const input = screen.getByLabelText('Sub-point text');
    fireEvent.change(input, { target: { value: 'Point via Enter' } });
    fireEvent.keyDown(input, { key: 'Enter' });
    expect(props.onAddSubPoint).toHaveBeenCalledWith('Point via Enter');
  });

  it('Next is disabled when no sub-points exist', () => {
    renderControls({ sermonActive: true, subPoints: [], subPointIndex: -1 });
    expect(screen.getByRole('button', { name: 'Advance to next sub-point' })).toBeDisabled();
  });

  it('Next is disabled when already at the last sub-point', () => {
    renderControls({
      sermonActive: true,
      subPoints: ['Point A', 'Point B'],
      subPointIndex: 1,
    });
    expect(screen.getByRole('button', { name: 'Advance to next sub-point' })).toBeDisabled();
  });

  it('Next is enabled when more sub-points are available', () => {
    renderControls({
      sermonActive: true,
      subPoints: ['Point A', 'Point B'],
      subPointIndex: 0,
    });
    expect(screen.getByRole('button', { name: 'Advance to next sub-point' })).toBeEnabled();
  });

  it('calls onNextSubPoint when Next is clicked', () => {
    const props = renderControls({
      sermonActive: true,
      subPoints: ['Point A', 'Point B'],
      subPointIndex: 0,
    });
    fireEvent.click(screen.getByRole('button', { name: 'Advance to next sub-point' }));
    expect(props.onNextSubPoint).toHaveBeenCalled();
  });

  it('displays the current sub-point prominently', () => {
    renderControls({
      sermonActive: true,
      subPoints: ['Living by Faith', 'Walking in Grace'],
      subPointIndex: 0,
    });
    expect(screen.getByLabelText('Current sub-point')).toHaveTextContent('Living by Faith');
  });

  it('does not show sub-point display when index is -1', () => {
    renderControls({
      sermonActive: true,
      subPoints: ['Living by Faith'],
      subPointIndex: -1,
    });
    expect(screen.queryByLabelText('Current sub-point')).toBeNull();
  });

  it('shows correct sub-point count', () => {
    renderControls({
      sermonActive: true,
      subPoints: ['A', 'B', 'C'],
      subPointIndex: 1,
    });
    expect(screen.getByText('Sub-point 2 of 3')).toBeInTheDocument();
  });
});

// ── end service dialog ────────────────────────────────────────────────────────

describe('End Service dialog', () => {
  it('opens when End Service is clicked', () => {
    renderControls({ sermonActive: true });
    openEndDialog();
    expect(screen.getByRole('dialog', { name: 'End Service' })).toBeInTheDocument();
  });

  it('Cancel closes without calling onEndService', () => {
    const props = renderControls({ sermonActive: true });
    const dialog = openEndDialog();
    fireEvent.click(within(dialog).getByRole('button', { name: 'Cancel' }));
    expect(screen.queryByRole('dialog')).toBeNull();
    expect(props.onEndService).not.toHaveBeenCalled();
  });

  it('Escape closes without calling onEndService', () => {
    const props = renderControls({ sermonActive: true });
    openEndDialog();
    fireEvent.keyDown(document.body, { key: 'Escape', bubbles: true });
    expect(screen.queryByRole('dialog')).toBeNull();
    expect(props.onEndService).not.toHaveBeenCalled();
  });

  it('clicking backdrop closes without calling onEndService', () => {
    const props = renderControls({ sermonActive: true });
    openEndDialog();
    fireEvent.click(screen.getByRole('presentation'));
    expect(screen.queryByRole('dialog')).toBeNull();
    expect(props.onEndService).not.toHaveBeenCalled();
  });

  it('confirm calls onEndService', () => {
    const props = renderControls({ sermonActive: true });
    const dialog = openEndDialog();
    fireEvent.click(within(dialog).getByRole('button', { name: 'End Service' }));
    expect(props.onEndService).toHaveBeenCalled();
  });

  it('dialog closes after confirming end', () => {
    renderControls({ sermonActive: true });
    const dialog = openEndDialog();
    fireEvent.click(within(dialog).getByRole('button', { name: 'End Service' }));
    expect(screen.queryByRole('dialog')).toBeNull();
  });

  it('shows confirmation message', () => {
    renderControls({ sermonActive: true });
    openEndDialog();
    expect(screen.getByText(/save the sermon record/i)).toBeInTheDocument();
  });
});
