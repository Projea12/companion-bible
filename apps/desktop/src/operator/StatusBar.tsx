// ── types ─────────────────────────────────────────────────────────────────────

export type AudioStatus = 'idle' | 'flowing' | 'degraded' | 'lost';
export type InternetStatus = 'online' | 'offline';
export type AiStatus = 'idle' | 'all-layers' | 'local-only' | 'pattern-only';
export type StorageStatus = 'ample' | 'low' | 'critical';
type SignalLevel = 'green' | 'amber' | 'red' | 'idle';

export interface StatusBarProps {
  sessionActive: boolean;
  audio: AudioStatus;
  internet: InternetStatus;
  ai: AiStatus;
  storage: StorageStatus;
  totalScreens: number;
  hasSecondary: boolean;
}

// ── level helpers ─────────────────────────────────────────────────────────────

function audioLevel(audio: AudioStatus, sessionActive: boolean): SignalLevel {
  if (!sessionActive || audio === 'idle') return 'idle';
  if (audio === 'flowing') return 'green';
  if (audio === 'degraded') return 'amber';
  return 'red';
}

function internetLevel(internet: InternetStatus): SignalLevel {
  return internet === 'online' ? 'green' : 'amber';
}

function aiLevel(ai: AiStatus): SignalLevel {
  if (ai === 'idle') return 'idle';
  if (ai === 'all-layers') return 'green';
  if (ai === 'local-only') return 'amber';
  return 'red';
}

function storageLevel(storage: StorageStatus): SignalLevel {
  if (storage === 'ample') return 'green';
  if (storage === 'low') return 'amber';
  return 'red';
}

// ── description labels ────────────────────────────────────────────────────────

const AUDIO_DESC: Record<AudioStatus, string> = {
  idle: 'Idle',
  flowing: 'Flowing',
  degraded: 'Degraded',
  lost: 'Lost',
};

const INTERNET_DESC: Record<InternetStatus, string> = {
  online: 'Online',
  offline: 'Offline',
};

const AI_DESC: Record<AiStatus, string> = {
  idle: 'Idle',
  'all-layers': 'All Layers',
  'local-only': 'Local Only',
  'pattern-only': 'Pattern Only',
};

const STORAGE_DESC: Record<StorageStatus, string> = {
  ample: 'Ample',
  low: 'Low',
  critical: 'Critical',
};

// ── component ─────────────────────────────────────────────────────────────────

export function StatusBar({
  sessionActive,
  audio,
  internet,
  ai,
  storage,
  totalScreens,
  hasSecondary,
}: StatusBarProps) {
  return (
    <footer className="op-statusbar" aria-label="System status">
      <div className="statusbar-indicators">
        <StatusBadge
          label="Audio"
          level={audioLevel(audio, sessionActive)}
          description={AUDIO_DESC[audio]}
        />
        <StatusBadge
          label="Internet"
          level={internetLevel(internet)}
          description={INTERNET_DESC[internet]}
        />
        <StatusBadge label="AI" level={aiLevel(ai)} description={AI_DESC[ai]} />
        <StatusBadge
          label="Storage"
          level={storageLevel(storage)}
          description={STORAGE_DESC[storage]}
        />
      </div>

      <div className="statusbar-center">
        <span className="status-screens" data-dual={String(hasSecondary)}>
          {totalScreens} screen{totalScreens !== 1 ? 's' : ''}
          {hasSecondary ? ' ✓' : ''}
        </span>
      </div>

      <div className="statusbar-right">
        <kbd>Space</kbd>
        <span>Discard</span>
        <kbd>Ctrl+Z</kbd>
        <span>Undo</span>
        <kbd>Enter</kbd>
        <span>Override</span>
      </div>
    </footer>
  );
}

// ── badge ─────────────────────────────────────────────────────────────────────

interface StatusBadgeProps {
  label: string;
  level: SignalLevel;
  description: string;
}

function StatusBadge({ label, level, description }: StatusBadgeProps) {
  return (
    <div className="status-badge" data-level={level} aria-label={`${label}: ${description}`}>
      <span className="status-dot" aria-hidden="true" />
      <span className="status-label">{label}</span>
      <span className="status-description">{description}</span>
    </div>
  );
}
