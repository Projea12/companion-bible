import { useEffect, useRef, useState } from 'react';

// ── types ─────────────────────────────────────────────────────────────────────

export interface SermonSetup {
  title: string;
  pastor: string;
  anchorScripture: string;
}

export interface SermonControlsProps {
  sermonActive: boolean;
  subPoints: string[];
  subPointIndex: number;
  onStartService: (setup: SermonSetup) => void;
  onEndService: () => void;
  onAddSubPoint: (text: string) => void;
  onNextSubPoint: () => void;
}

// ── main panel ────────────────────────────────────────────────────────────────

export function SermonControls({
  sermonActive,
  subPoints,
  subPointIndex,
  onStartService,
  onEndService,
  onAddSubPoint,
  onNextSubPoint,
}: SermonControlsProps) {
  const [startOpen, setStartOpen] = useState(false);
  const [endOpen, setEndOpen] = useState(false);
  const [subPointInput, setSubPointInput] = useState('');

  const currentSubPoint = subPoints[subPointIndex] ?? null;
  const hasNext = subPointIndex < subPoints.length - 1;

  const handleAdd = () => {
    const text = subPointInput.trim();
    if (!text) return;
    onAddSubPoint(text);
    setSubPointInput('');
  };

  return (
    <section className="op-panel op-panel-sermon" aria-label="Sermon controls">
      <div className="op-panel-header">
        <h2 className="op-panel-heading">Sermon</h2>
        {sermonActive && (
          <button className="btn btn-danger btn-sm" onClick={() => setEndOpen(true)}>
            End Service
          </button>
        )}
      </div>

      {!sermonActive ? (
        <button className="btn btn-primary btn-start-service" onClick={() => setStartOpen(true)}>
          Start Service
        </button>
      ) : (
        <div className="sermon-active-controls">
          {currentSubPoint && (
            <div className="subpoint-current" aria-live="polite" aria-label="Current sub-point">
              {currentSubPoint}
            </div>
          )}

          <div className="subpoint-nav">
            <span className="subpoint-count">
              {subPoints.length === 0
                ? 'No sub-points'
                : `Sub-point ${Math.max(subPointIndex + 1, 0)} of ${subPoints.length}`}
            </span>
            <button
              className="btn btn-secondary btn-sm"
              disabled={!hasNext}
              onClick={onNextSubPoint}
              aria-label="Advance to next sub-point"
            >
              Next
            </button>
          </div>

          <div className="subpoint-add-row">
            <input
              className="override-input"
              placeholder="New sub-point…"
              value={subPointInput}
              onChange={(e) => setSubPointInput(e.target.value)}
              onKeyDown={(e) => {
                if (e.key === 'Enter') {
                  e.preventDefault();
                  handleAdd();
                }
              }}
              aria-label="Sub-point text"
            />
            <button
              className="btn btn-secondary btn-sm"
              disabled={!subPointInput.trim()}
              onClick={handleAdd}
              aria-label="Add sub-point"
            >
              Add
            </button>
          </div>
        </div>
      )}

      {startOpen && (
        <StartSermonDialog
          onConfirm={(setup) => {
            onStartService(setup);
            setStartOpen(false);
          }}
          onCancel={() => setStartOpen(false)}
        />
      )}

      {endOpen && (
        <EndSermonDialog
          onConfirm={() => {
            onEndService();
            setEndOpen(false);
          }}
          onCancel={() => setEndOpen(false)}
        />
      )}
    </section>
  );
}

// ── start sermon dialog ───────────────────────────────────────────────────────

interface StartSermonDialogProps {
  onConfirm: (setup: SermonSetup) => void;
  onCancel: () => void;
}

function StartSermonDialog({ onConfirm, onCancel }: StartSermonDialogProps) {
  const [title, setTitle] = useState('');
  const [pastor, setPastor] = useState('');
  const [anchorScripture, setAnchorScripture] = useState('');
  const titleRef = useRef<HTMLInputElement>(null);

  useEffect(() => {
    titleRef.current?.focus();
  }, []);

  useEffect(() => {
    const onKey = (e: KeyboardEvent) => {
      if (e.key === 'Escape') onCancel();
    };
    document.addEventListener('keydown', onKey);
    return () => document.removeEventListener('keydown', onKey);
  }, [onCancel]);

  return (
    <div className="dialog-backdrop" onClick={onCancel} role="presentation">
      <div
        className="dialog"
        role="dialog"
        aria-modal="true"
        aria-labelledby="start-dialog-title"
        onClick={(e) => e.stopPropagation()}
      >
        <h2 className="dialog-title" id="start-dialog-title">
          Start Service
        </h2>

        <div className="dialog-body">
          <div className="dialog-field">
            <label htmlFor="sermon-title">Title</label>
            <input
              ref={titleRef}
              id="sermon-title"
              className="override-input"
              placeholder="e.g. Walking by Faith"
              value={title}
              onChange={(e) => setTitle(e.target.value)}
            />
          </div>
          <div className="dialog-field">
            <label htmlFor="sermon-pastor">Pastor</label>
            <input
              id="sermon-pastor"
              className="override-input"
              placeholder="e.g. Pastor John"
              value={pastor}
              onChange={(e) => setPastor(e.target.value)}
            />
          </div>
          <div className="dialog-field">
            <label htmlFor="sermon-anchor">Anchor Scripture</label>
            <input
              id="sermon-anchor"
              className="override-input"
              placeholder="e.g. Hebrews 11:1"
              value={anchorScripture}
              onChange={(e) => setAnchorScripture(e.target.value)}
            />
          </div>
        </div>

        <div className="dialog-footer">
          <button className="btn btn-secondary" onClick={onCancel}>
            Cancel
          </button>
          <button
            className="btn btn-primary"
            onClick={() => onConfirm({ title, pastor, anchorScripture })}
          >
            Begin Service
          </button>
        </div>
      </div>
    </div>
  );
}

// ── end sermon dialog ─────────────────────────────────────────────────────────

interface EndSermonDialogProps {
  onConfirm: () => void;
  onCancel: () => void;
}

function EndSermonDialog({ onConfirm, onCancel }: EndSermonDialogProps) {
  const confirmRef = useRef<HTMLButtonElement>(null);

  useEffect(() => {
    confirmRef.current?.focus();
  }, []);

  useEffect(() => {
    const onKey = (e: KeyboardEvent) => {
      if (e.key === 'Escape') onCancel();
    };
    document.addEventListener('keydown', onKey);
    return () => document.removeEventListener('keydown', onKey);
  }, [onCancel]);

  return (
    <div className="dialog-backdrop" onClick={onCancel} role="presentation">
      <div
        className="dialog"
        role="dialog"
        aria-modal="true"
        aria-labelledby="end-dialog-title"
        onClick={(e) => e.stopPropagation()}
      >
        <h2 className="dialog-title" id="end-dialog-title">
          End Service
        </h2>
        <p className="dialog-message">
          This will save the sermon record and generate a summary. Continue?
        </p>
        <div className="dialog-footer">
          <button className="btn btn-secondary" onClick={onCancel}>
            Cancel
          </button>
          <button ref={confirmRef} className="btn btn-danger" onClick={onConfirm}>
            End Service
          </button>
        </div>
      </div>
    </div>
  );
}
