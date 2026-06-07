import { useCallback, useState } from 'react';
import { invoke } from '@tauri-apps/api/core';
import { suggestBooks } from './bibleBooks';

// ── data model ────────────────────────────────────────────────────────────────

interface SermonScripture {
  id: number;
  reference: string;
}

interface SermonSubPoint {
  id: number;
  text: string;
  scriptures: SermonScripture[];
}

interface SermonPoint {
  id: number;
  romanNumeral: string;
  text: string;
  subPoints: SermonSubPoint[];
}

interface SermonOutline {
  title: string;
  preacher: string;
  points: SermonPoint[];
}

let nextId = 1;
function uid(): number {
  return nextId++;
}

function toRoman(n: number): string {
  const vals = [1000, 900, 500, 400, 100, 90, 50, 40, 10, 9, 5, 4, 1];
  const syms = ['M', 'CM', 'D', 'CD', 'C', 'XC', 'L', 'XL', 'X', 'IX', 'V', 'IV', 'I'];
  let result = '';
  let remaining = n;
  for (let i = 0; i < vals.length; i++) {
    while (remaining >= vals[i]) {
      result += syms[i];
      remaining -= vals[i];
    }
  }
  return result;
}

// ── scripture combobox ────────────────────────────────────────────────────────

interface ScriptureInputProps {
  value: string;
  onChange: (val: string) => void;
}

function ScriptureInput({ value, onChange }: ScriptureInputProps) {
  const [open, setOpen] = useState(false);
  const suggestions = suggestBooks(value);

  return (
    <div className="scripture-input-wrap">
      <input
        className="override-input scripture-ref-input"
        placeholder="e.g. John 3:16"
        value={value}
        onChange={(e) => {
          onChange(e.target.value);
          setOpen(true);
        }}
        onBlur={() => setTimeout(() => setOpen(false), 150)}
      />
      {open && suggestions.length > 0 && (
        <ul className="override-suggestions">
          {suggestions.map((book) => (
            <li
              key={book}
              className="override-suggestion"
              onMouseDown={(e) => {
                e.preventDefault();
                onChange(book + ' ');
                setOpen(false);
              }}
            >
              {book}
            </li>
          ))}
        </ul>
      )}
    </div>
  );
}

// ── setup view ────────────────────────────────────────────────────────────────

interface SetupViewProps {
  outline: SermonOutline;
  onChange: (outline: SermonOutline) => void;
  onGoLive: () => void;
}

function SetupView({ outline, onChange, onGoLive }: SetupViewProps) {
  const canGoLive = outline.title.trim().length > 0;

  const addPoint = () =>
    onChange({
      ...outline,
      points: [
        ...outline.points,
        { id: uid(), romanNumeral: toRoman(outline.points.length + 1), text: '', subPoints: [] },
      ],
    });

  const removePoint = (pid: number) =>
    onChange({ ...outline, points: outline.points.filter((p) => p.id !== pid) });

  const updatePoint = (pid: number, text: string) =>
    onChange({
      ...outline,
      points: outline.points.map((p) => (p.id === pid ? { ...p, text } : p)),
    });

  const movePoint = (pid: number, dir: -1 | 1) => {
    const pts = [...outline.points];
    const i = pts.findIndex((p) => p.id === pid);
    const j = i + dir;
    if (j < 0 || j >= pts.length) return;
    [pts[i], pts[j]] = [pts[j], pts[i]];
    onChange({ ...outline, points: pts });
  };

  const addSubPoint = (pid: number) =>
    onChange({
      ...outline,
      points: outline.points.map((p) =>
        p.id === pid
          ? { ...p, subPoints: [...p.subPoints, { id: uid(), text: '', scriptures: [] }] }
          : p,
      ),
    });

  const removeSubPoint = (pid: number, sid: number) =>
    onChange({
      ...outline,
      points: outline.points.map((p) =>
        p.id === pid ? { ...p, subPoints: p.subPoints.filter((s) => s.id !== sid) } : p,
      ),
    });

  const updateSubPoint = (pid: number, sid: number, text: string) =>
    onChange({
      ...outline,
      points: outline.points.map((p) =>
        p.id === pid
          ? { ...p, subPoints: p.subPoints.map((s) => (s.id === sid ? { ...s, text } : s)) }
          : p,
      ),
    });

  const moveSubPoint = (pid: number, sid: number, dir: -1 | 1) => {
    const point = outline.points.find((p) => p.id === pid);
    if (!point) return;
    const subs = [...point.subPoints];
    const i = subs.findIndex((s) => s.id === sid);
    const j = i + dir;
    if (j < 0 || j >= subs.length) return;
    [subs[i], subs[j]] = [subs[j], subs[i]];
    onChange({
      ...outline,
      points: outline.points.map((p) => (p.id === pid ? { ...p, subPoints: subs } : p)),
    });
  };

  const addScripture = (pid: number, sid: number) =>
    onChange({
      ...outline,
      points: outline.points.map((p) =>
        p.id === pid
          ? {
              ...p,
              subPoints: p.subPoints.map((s) =>
                s.id === sid
                  ? { ...s, scriptures: [...s.scriptures, { id: uid(), reference: '' }] }
                  : s,
              ),
            }
          : p,
      ),
    });

  const removeScripture = (pid: number, sid: number, rid: number) =>
    onChange({
      ...outline,
      points: outline.points.map((p) =>
        p.id === pid
          ? {
              ...p,
              subPoints: p.subPoints.map((s) =>
                s.id === sid ? { ...s, scriptures: s.scriptures.filter((r) => r.id !== rid) } : s,
              ),
            }
          : p,
      ),
    });

  const updateScripture = (pid: number, sid: number, rid: number, reference: string) =>
    onChange({
      ...outline,
      points: outline.points.map((p) =>
        p.id === pid
          ? {
              ...p,
              subPoints: p.subPoints.map((s) =>
                s.id === sid
                  ? {
                      ...s,
                      scriptures: s.scriptures.map((r) => (r.id === rid ? { ...r, reference } : r)),
                    }
                  : s,
              ),
            }
          : p,
      ),
    });

  return (
    <div className="sermon-setup">
      {/* Title + Preacher */}
      <div className="op-panel sermon-setup-header">
        <h2 className="op-panel-heading">Sermon Setup</h2>
        <div className="sermon-setup-fields">
          <div className="sermon-field">
            <label className="sermon-field-label">Title</label>
            <input
              className="override-input"
              placeholder="e.g. Walking by Faith"
              value={outline.title}
              onChange={(e) => onChange({ ...outline, title: e.target.value })}
            />
          </div>
          <div className="sermon-field">
            <label className="sermon-field-label">Preacher</label>
            <input
              className="override-input"
              placeholder="e.g. Pastor John"
              value={outline.preacher}
              onChange={(e) => onChange({ ...outline, preacher: e.target.value })}
            />
          </div>
        </div>
      </div>

      {/* Points */}
      {outline.points.map((point, pIdx) => (
        <div key={point.id} className="op-panel sermon-point-block">
          <div className="sermon-point-header">
            <span className="sermon-point-numeral">{point.romanNumeral}.</span>
            <input
              className="override-input sermon-point-input"
              placeholder="Point text…"
              value={point.text}
              onChange={(e) => updatePoint(point.id, e.target.value)}
            />
            <div className="sermon-reorder-btns">
              <button
                className="btn-icon-sm"
                onClick={() => movePoint(point.id, -1)}
                disabled={pIdx === 0}
                title="Move up"
              >
                ↑
              </button>
              <button
                className="btn-icon-sm"
                onClick={() => movePoint(point.id, 1)}
                disabled={pIdx === outline.points.length - 1}
                title="Move down"
              >
                ↓
              </button>
              <button
                className="btn-icon-sm btn-icon-sm--danger"
                onClick={() => removePoint(point.id)}
                title="Remove point"
              >
                ✕
              </button>
            </div>
          </div>

          {/* Sub-points */}
          {point.subPoints.map((sub, sIdx) => (
            <div key={sub.id} className="sermon-subpoint-block">
              <div className="sermon-subpoint-header">
                <span className="sermon-subpoint-num">{sIdx + 1}.</span>
                <input
                  className="override-input sermon-subpoint-input"
                  placeholder="Sub-point text…"
                  value={sub.text}
                  onChange={(e) => updateSubPoint(point.id, sub.id, e.target.value)}
                />
                <div className="sermon-reorder-btns">
                  <button
                    className="btn-icon-sm"
                    onClick={() => moveSubPoint(point.id, sub.id, -1)}
                    disabled={sIdx === 0}
                  >
                    ↑
                  </button>
                  <button
                    className="btn-icon-sm"
                    onClick={() => moveSubPoint(point.id, sub.id, 1)}
                    disabled={sIdx === point.subPoints.length - 1}
                  >
                    ↓
                  </button>
                  <button
                    className="btn-icon-sm btn-icon-sm--danger"
                    onClick={() => removeSubPoint(point.id, sub.id)}
                  >
                    ✕
                  </button>
                </div>
              </div>

              {/* Scriptures */}
              {sub.scriptures.map((ref) => (
                <div key={ref.id} className="sermon-scripture-row">
                  <span className="sermon-scripture-icon">→</span>
                  <ScriptureInput
                    value={ref.reference}
                    onChange={(val) => updateScripture(point.id, sub.id, ref.id, val)}
                  />
                  <button
                    className="btn-icon-sm btn-icon-sm--danger"
                    onClick={() => removeScripture(point.id, sub.id, ref.id)}
                  >
                    ✕
                  </button>
                </div>
              ))}

              <button className="btn-outline-sm" onClick={() => addScripture(point.id, sub.id)}>
                + Scripture
              </button>
            </div>
          ))}

          <button className="btn-outline-sm" onClick={() => addSubPoint(point.id)}>
            + Sub-point
          </button>
        </div>
      ))}

      {/* Footer */}
      <div className="op-panel sermon-setup-footer">
        <button className="btn btn-secondary btn-sm" onClick={addPoint}>
          + Add Point
        </button>
        <button
          className="btn btn-primary"
          disabled={!canGoLive}
          onClick={onGoLive}
          title={!canGoLive ? 'Add a sermon title to go live' : undefined}
        >
          Go Live →
        </button>
      </div>
    </div>
  );
}

// ── live view ─────────────────────────────────────────────────────────────────

interface LiveViewProps {
  outline: SermonOutline;
  onEditOutline: () => void;
}

function LiveView({ outline, onEditOutline }: LiveViewProps) {
  const [shown, setShown] = useState<Set<string>>(new Set());
  const [activeId, setActiveId] = useState<string | null>(null);

  const tap = useCallback((id: string, action: () => Promise<unknown>) => {
    void action();
    setShown((prev) => new Set([...prev, id]));
    setActiveId(id);
  }, []);

  return (
    <div className="sermon-live">
      <div className="sermon-live-toolbar op-panel">
        <button className="btn-outline-sm" onClick={onEditOutline}>
          ← Edit Outline
        </button>
        {outline.preacher && <span className="sermon-live-preacher">{outline.preacher}</span>}
      </div>

      {/* Title row */}
      <button
        className={`sermon-live-row sermon-live-row--title${activeId === 'title' ? ' sermon-live-row--active' : ''}`}
        onClick={() => tap('title', () => invoke('show_sermon_title', { title: outline.title }))}
      >
        <div className="sermon-live-row-body">
          <span className="sermon-live-eyebrow">Sermon Title</span>
          <span className="sermon-live-text">{outline.title || '—'}</span>
        </div>
        {shown.has('title') && <span className="sermon-check">✓</span>}
      </button>

      {/* Points */}
      {outline.points.map((point, pIdx) => {
        const pointId = `p-${point.id}`;
        return (
          <div key={point.id} className="sermon-live-point-group">
            <button
              className={`sermon-live-row sermon-live-row--point${activeId === pointId ? ' sermon-live-row--active' : ''}`}
              onClick={() =>
                tap(pointId, () =>
                  invoke('show_sermon_point', { text: point.text, number: pIdx + 1 }),
                )
              }
            >
              <div className="sermon-live-row-body">
                <span className="sermon-live-eyebrow">Point {point.romanNumeral}</span>
                <span className="sermon-live-text">{point.text || '—'}</span>
              </div>
              {shown.has(pointId) && <span className="sermon-check">✓</span>}
            </button>

            {/* Sub-points */}
            {point.subPoints.map((sub, sIdx) => {
              const subId = `p-${point.id}-sp-${sub.id}`;
              return (
                <div key={sub.id} className="sermon-live-sub-group">
                  <button
                    className={`sermon-live-row sermon-live-row--subpoint${activeId === subId ? ' sermon-live-row--active' : ''}`}
                    onClick={() =>
                      tap(subId, () => invoke('show_sub_point', { subPoint: sub.text }))
                    }
                  >
                    <div className="sermon-live-row-body">
                      <span className="sermon-live-eyebrow">{sIdx + 1}.</span>
                      <span className="sermon-live-text">{sub.text || '—'}</span>
                    </div>
                    {shown.has(subId) && <span className="sermon-check">✓</span>}
                  </button>

                  {/* Scriptures */}
                  {sub.scriptures.map((ref) => {
                    const refId = `p-${point.id}-sp-${sub.id}-sc-${ref.id}`;
                    return (
                      <button
                        key={ref.id}
                        className={`sermon-live-row sermon-live-row--scripture${activeId === refId ? ' sermon-live-row--active' : ''}`}
                        onClick={() =>
                          tap(refId, () =>
                            invoke('show_verse', { reference: ref.reference, text: '' }),
                          )
                        }
                        disabled={!ref.reference.trim()}
                      >
                        <div className="sermon-live-row-body">
                          <span className="sermon-live-eyebrow">→</span>
                          <span className="sermon-live-text sermon-live-text--ref">
                            {ref.reference || '(no reference)'}
                          </span>
                        </div>
                        {shown.has(refId) && <span className="sermon-check">✓</span>}
                      </button>
                    );
                  })}
                </div>
              );
            })}
          </div>
        );
      })}
    </div>
  );
}

// ── main component ─────────────────────────────────────────────────────────────

const EMPTY_OUTLINE: SermonOutline = { title: '', preacher: '', points: [] };

export function SermonTab() {
  const [outline, setOutline] = useState<SermonOutline>(EMPTY_OUTLINE);
  const [isLive, setIsLive] = useState(false);

  if (!isLive) {
    return <SetupView outline={outline} onChange={setOutline} onGoLive={() => setIsLive(true)} />;
  }

  return <LiveView outline={outline} onEditOutline={() => setIsLive(false)} />;
}
