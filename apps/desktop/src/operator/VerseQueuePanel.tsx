import { useEffect, useState } from 'react';
import { type QueuedVerse, confidenceLevel, QUEUE_EXPIRY_MS } from './useVerseQueue';

// ── panel ─────────────────────────────────────────────────────────────────────

export interface VerseQueuePanelProps {
  items: QueuedVerse[];
  sessionActive: boolean;
  onConfirm: (id: number, label: string) => void;
  onReject: (id: number, label: string) => void;
}

export function VerseQueuePanel({
  items,
  sessionActive,
  onConfirm,
  onReject,
}: VerseQueuePanelProps) {
  const count = items.length;

  return (
    <section className="op-panel op-panel-queue" aria-label="Verse queue">
      <div className="op-panel-header">
        <h2 className="op-panel-heading">Verse Queue</h2>
        {count > 0 && (
          <span className="queue-count" aria-live="polite">
            {count} pending
          </span>
        )}
      </div>

      {count === 0 ? (
        <p className="queue-empty">
          {sessionActive ? 'Listening for scripture references…' : 'Start a session to begin'}
        </p>
      ) : (
        <ul className="queue-list" aria-label="Queued verses">
          {items.map((item) => (
            <QueueItem key={item.id} item={item} onConfirm={onConfirm} onReject={onReject} />
          ))}
        </ul>
      )}
    </section>
  );
}

// ── queue item ────────────────────────────────────────────────────────────────

interface QueueItemProps {
  item: QueuedVerse;
  onConfirm: (id: number, label: string) => void;
  onReject: (id: number, label: string) => void;
}

function QueueItem({ item, onConfirm, onReject }: QueueItemProps) {
  const level = confidenceLevel(item.confidence);

  return (
    <li className={`queue-item queue-item--${level}`} data-confidence={level}>
      <div className="queue-item-top">
        <span className="queue-ref">{item.label}</span>
        <div className="queue-meta">
          <span className={`queue-level-badge queue-level-badge--${level}`}>
            {level.toUpperCase()}
          </span>
          <ExpiryTimer expiresAt={item.expiresAt} />
        </div>
      </div>

      <div className="queue-confidence-row">
        <div className="queue-confidence-track" aria-hidden="true">
          <div
            className={`queue-confidence-fill queue-confidence-fill--${level}`}
            style={{ width: `${item.confidence}%` }}
          />
        </div>
        <span className="queue-confidence-pct" aria-label={`Confidence ${item.confidence}%`}>
          {item.confidence}%
        </span>
      </div>

      <div className="queue-actions">
        <button
          className="btn btn-approve queue-btn"
          onClick={() => onConfirm(item.id, item.label)}
          aria-label={`Confirm ${item.label}`}
        >
          ✓ Confirm
        </button>
        <button
          className="btn btn-reject queue-btn"
          onClick={() => onReject(item.id, item.label)}
          aria-label={`Reject ${item.label}`}
        >
          ✗ Reject
        </button>
      </div>

      {/* Shrinking expiry bar */}
      <ExpiryBar expiresAt={item.expiresAt} />
    </li>
  );
}

// ── expiry countdown ──────────────────────────────────────────────────────────

function ExpiryTimer({ expiresAt }: { expiresAt: number }) {
  const [, setTick] = useState(0);

  useEffect(() => {
    const id = setInterval(() => setTick((t) => t + 1), 1000);
    return () => clearInterval(id);
  }, []);

  const secsLeft = Math.max(0, Math.ceil((expiresAt - Date.now()) / 1000));
  return (
    <span className="queue-expiry" aria-label={`Expires in ${secsLeft} seconds`}>
      {secsLeft}s
    </span>
  );
}

// ── shrinking progress bar that represents remaining time ─────────────────────

function ExpiryBar({ expiresAt }: { expiresAt: number }) {
  const [, setTick] = useState(0);

  useEffect(() => {
    const id = setInterval(() => setTick((t) => t + 1), 200);
    return () => clearInterval(id);
  }, []);

  const pct = Math.max(0, ((expiresAt - Date.now()) / QUEUE_EXPIRY_MS) * 100);
  return (
    <div className="queue-expiry-track" aria-hidden="true">
      <div className="queue-expiry-fill" style={{ width: `${pct}%` }} />
    </div>
  );
}
