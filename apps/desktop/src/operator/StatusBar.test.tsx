import { describe, it, expect } from 'vitest';
import { render, screen } from '@testing-library/react';
import { StatusBar } from './StatusBar';
import type { AudioStatus, InternetStatus, AiStatus, StorageStatus } from './StatusBar';

// ── helpers ───────────────────────────────────────────────────────────────────

interface Overrides {
  sessionActive?: boolean;
  audio?: AudioStatus;
  internet?: InternetStatus;
  ai?: AiStatus;
  storage?: StorageStatus;
}

function renderBar(overrides: Overrides = {}) {
  return render(
    <StatusBar
      sessionActive={overrides.sessionActive ?? true}
      audio={overrides.audio ?? 'flowing'}
      internet={overrides.internet ?? 'online'}
      ai={overrides.ai ?? 'all-layers'}
      storage={overrides.storage ?? 'ample'}
      totalScreens={1}
      hasSecondary={false}
    />,
  );
}

function audioBadge() {
  return screen.getByLabelText(/^Audio:/);
}
function internetBadge() {
  return screen.getByLabelText(/^Internet:/);
}
function aiBadge() {
  return screen.getByLabelText(/^AI:/);
}
function storageBadge() {
  return screen.getByLabelText(/^Storage:/);
}

// ── audio indicator ───────────────────────────────────────────────────────────

describe('audio indicator', () => {
  it('shows idle level when session is inactive, regardless of audio state', () => {
    renderBar({ sessionActive: false, audio: 'flowing' });
    expect(audioBadge()).toHaveAttribute('data-level', 'idle');
  });

  it('shows idle level when audio is idle and session is active', () => {
    renderBar({ audio: 'idle' });
    expect(audioBadge()).toHaveAttribute('data-level', 'idle');
  });

  it('shows green level when audio is flowing', () => {
    renderBar({ audio: 'flowing' });
    expect(audioBadge()).toHaveAttribute('data-level', 'green');
  });

  it('shows amber level when audio is degraded', () => {
    renderBar({ audio: 'degraded' });
    expect(audioBadge()).toHaveAttribute('data-level', 'amber');
  });

  it('shows red level when audio is lost', () => {
    renderBar({ audio: 'lost' });
    expect(audioBadge()).toHaveAttribute('data-level', 'red');
  });

  it('aria label includes the description', () => {
    renderBar({ audio: 'degraded' });
    expect(audioBadge()).toHaveAttribute('aria-label', 'Audio: Degraded');
  });
});

// ── internet indicator ────────────────────────────────────────────────────────

describe('internet indicator', () => {
  it('shows green level when online', () => {
    renderBar({ internet: 'online' });
    expect(internetBadge()).toHaveAttribute('data-level', 'green');
  });

  it('shows amber level when offline', () => {
    renderBar({ internet: 'offline' });
    expect(internetBadge()).toHaveAttribute('data-level', 'amber');
  });

  it('aria label includes the description', () => {
    renderBar({ internet: 'offline' });
    expect(internetBadge()).toHaveAttribute('aria-label', 'Internet: Offline');
  });
});

// ── AI indicator ──────────────────────────────────────────────────────────────

describe('AI indicator', () => {
  it('shows idle level when AI is idle', () => {
    renderBar({ ai: 'idle' });
    expect(aiBadge()).toHaveAttribute('data-level', 'idle');
  });

  it('shows green level when all layers are running', () => {
    renderBar({ ai: 'all-layers' });
    expect(aiBadge()).toHaveAttribute('data-level', 'green');
  });

  it('shows amber level when running local-only', () => {
    renderBar({ ai: 'local-only' });
    expect(aiBadge()).toHaveAttribute('data-level', 'amber');
  });

  it('shows red level when running pattern-only', () => {
    renderBar({ ai: 'pattern-only' });
    expect(aiBadge()).toHaveAttribute('data-level', 'red');
  });

  it('aria label includes the description', () => {
    renderBar({ ai: 'local-only' });
    expect(aiBadge()).toHaveAttribute('aria-label', 'AI: Local Only');
  });
});

// ── storage indicator ─────────────────────────────────────────────────────────

describe('storage indicator', () => {
  it('shows green level when storage is ample', () => {
    renderBar({ storage: 'ample' });
    expect(storageBadge()).toHaveAttribute('data-level', 'green');
  });

  it('shows amber level when storage is low', () => {
    renderBar({ storage: 'low' });
    expect(storageBadge()).toHaveAttribute('data-level', 'amber');
  });

  it('shows red level when storage is critical', () => {
    renderBar({ storage: 'critical' });
    expect(storageBadge()).toHaveAttribute('data-level', 'red');
  });

  it('aria label includes the description', () => {
    renderBar({ storage: 'critical' });
    expect(storageBadge()).toHaveAttribute('aria-label', 'Storage: Critical');
  });
});

// ── accessibility ─────────────────────────────────────────────────────────────

describe('accessibility', () => {
  it('footer has an accessible label', () => {
    renderBar();
    expect(screen.getByRole('contentinfo', { name: 'System status' })).toBeInTheDocument();
  });

  it('renders all four indicator labels', () => {
    renderBar();
    expect(screen.getByText('Audio')).toBeInTheDocument();
    expect(screen.getByText('Internet')).toBeInTheDocument();
    expect(screen.getByText('AI')).toBeInTheDocument();
    expect(screen.getByText('Storage')).toBeInTheDocument();
  });
});

// ── screen info ───────────────────────────────────────────────────────────────

describe('screen info', () => {
  it('shows single screen count', () => {
    render(
      <StatusBar
        sessionActive={true}
        audio="flowing"
        internet="online"
        ai="all-layers"
        storage="ample"
        totalScreens={1}
        hasSecondary={false}
      />,
    );
    expect(screen.getByText('1 screen')).toBeInTheDocument();
  });

  it('shows plural screen count', () => {
    render(
      <StatusBar
        sessionActive={true}
        audio="flowing"
        internet="online"
        ai="all-layers"
        storage="ample"
        totalScreens={2}
        hasSecondary={true}
      />,
    );
    expect(screen.getByText('2 screens ✓')).toBeInTheDocument();
  });
});
