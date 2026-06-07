import { useRef, useState } from 'react';
import { invoke } from '@tauri-apps/api/core';

interface MoreTabProps {
  serviceItems: { id: number; label: string; isCurrent: boolean }[];
  onAddServiceItem: (label: string) => void;
  onRemoveServiceItem: (id: number) => void;
  onSetCurrentServiceItem: (id: number) => void;
  onNextServiceItem: () => void;
  onClearServiceItems: () => void;
  announcements: { id: number; body: string; durationSecs: number }[];
  announcementRunning: boolean;
  announcementIndex: number | null;
  onAddAnnouncement: (body: string, durationSecs: number) => void;
  onRemoveAnnouncement: (id: number) => void;
  onStartAnnouncements: () => void;
  onStopAnnouncements: () => void;
  onNextAnnouncement: () => void;
  onPrevAnnouncement: () => void;
  sessionActive: boolean;
}

export function MoreTab({
  serviceItems,
  onAddServiceItem,
  onRemoveServiceItem,
  onSetCurrentServiceItem,
  onNextServiceItem,
  onClearServiceItems,
  announcements,
  announcementRunning,
  announcementIndex,
  onAddAnnouncement,
  onRemoveAnnouncement,
  onStartAnnouncements,
  onStopAnnouncements,
  onNextAnnouncement,
  onPrevAnnouncement,
  sessionActive,
}: MoreTabProps) {
  const [newLabel, setNewLabel] = useState('');
  const [newAnnouncementBody, setNewAnnouncementBody] = useState('');
  const [newAnnouncementDuration, setNewAnnouncementDuration] = useState(30);
  const [confirmClear, setConfirmClear] = useState(false);
  const addInputRef = useRef<HTMLInputElement>(null);

  const [assemblyaiKey, setAssemblyaiKey] = useState('');
  const [deepgramKey, setDeepgramKey] = useState('');
  const [openaiKey, setOpenaiKey] = useState('');
  const [savedKey, setSavedKey] = useState<Set<string>>(new Set());

  const saveKey = (service: string, key: string, cmd: string) => {
    void invoke(cmd, { key });
    setSavedKey((prev) => new Set([...prev, service]));
  };

  const currentIdx = serviceItems.findIndex((i) => i.isCurrent);
  const nextItem =
    currentIdx >= 0 && currentIdx < serviceItems.length - 1 ? serviceItems[currentIdx + 1] : null;
  const isOnLast = currentIdx === serviceItems.length - 1 && serviceItems.length > 0;

  const handleAdd = () => {
    const label = newLabel.trim().toUpperCase();
    if (!label) return;
    onAddServiceItem(label);
    setNewLabel('');
    addInputRef.current?.focus();
  };

  const handleClearConfirm = () => {
    if (confirmClear) {
      onClearServiceItems();
      setConfirmClear(false);
    } else {
      setConfirmClear(true);
      setTimeout(() => setConfirmClear(false), 3000);
    }
  };

  const handleAddAnnouncement = () => {
    const body = newAnnouncementBody.trim();
    if (!body) return;
    onAddAnnouncement(body, newAnnouncementDuration);
    setNewAnnouncementBody('');
  };

  // Common service items for quick-add
  const PRESETS = [
    'OPENING PRAYER',
    'WORSHIP',
    'SCRIPTURE',
    'SERMON',
    'OFFERING',
    'COMMUNION',
    'CLOSING PRAYER',
  ];
  const unusedPresets = PRESETS.filter((p) => !serviceItems.some((i) => i.label === p));

  return (
    <>
      {/* ── Order of Service Runsheet ── */}
      <section className="op-panel runsheet-panel">
        <div className="runsheet-header">
          <h2 className="op-panel-heading">Order of Service</h2>
          {serviceItems.length > 0 && (
            <button
              className={`runsheet-clear-btn${confirmClear ? ' runsheet-clear-btn--confirm' : ''}`}
              onClick={handleClearConfirm}
              title="Clear all items"
            >
              {confirmClear ? 'Confirm clear?' : 'Clear all'}
            </button>
          )}
        </div>

        {/* Runsheet list */}
        {serviceItems.length > 0 ? (
          <ol className="runsheet-list">
            {serviceItems.map((item, idx) => {
              const isDone = currentIdx >= 0 && idx < currentIdx;
              const isCurrent = item.isCurrent;
              const isNext = idx === currentIdx + 1;

              let rowClass = 'runsheet-row';
              if (isDone) rowClass += ' runsheet-row--done';
              if (isCurrent) rowClass += ' runsheet-row--current';
              if (isNext) rowClass += ' runsheet-row--next';

              return (
                <li key={item.id} className={rowClass}>
                  <button
                    className="runsheet-row-btn"
                    onClick={() => onSetCurrentServiceItem(item.id)}
                    title={isCurrent ? 'Current item' : 'Jump to this item'}
                  >
                    <span className="runsheet-indicator" aria-hidden="true">
                      {isDone ? '✓' : isCurrent ? '●' : ''}
                    </span>
                    <span className="runsheet-label">{item.label}</span>
                    {isCurrent && <span className="runsheet-now-pill">NOW</span>}
                    {isNext && <span className="runsheet-next-pill">NEXT</span>}
                  </button>
                  <button
                    className="runsheet-remove-btn"
                    onClick={() => onRemoveServiceItem(item.id)}
                    title="Remove"
                    aria-label={`Remove ${item.label}`}
                  >
                    ✕
                  </button>
                </li>
              );
            })}
          </ol>
        ) : (
          <p className="runsheet-empty">No items yet. Add items below or use quick-add.</p>
        )}

        {/* Quick-add presets */}
        {unusedPresets.length > 0 && (
          <div className="runsheet-presets">
            {unusedPresets.map((p) => (
              <button key={p} className="runsheet-preset-chip" onClick={() => onAddServiceItem(p)}>
                + {p}
              </button>
            ))}
          </div>
        )}

        {/* Custom add */}
        <div className="runsheet-add-row">
          <input
            ref={addInputRef}
            className="runsheet-add-input"
            placeholder="Custom item…"
            value={newLabel}
            onChange={(e) => setNewLabel(e.target.value)}
            onKeyDown={(e) => {
              if (e.key === 'Enter') handleAdd();
            }}
          />
          <button
            className="btn btn-secondary btn-sm"
            disabled={!newLabel.trim()}
            onClick={handleAdd}
          >
            Add
          </button>
        </div>

        {/* NEXT — the hero action */}
        <button
          className="runsheet-next-btn"
          disabled={serviceItems.length === 0 || isOnLast}
          onClick={onNextServiceItem}
        >
          {nextItem ? (
            <>
              <span className="runsheet-next-btn-label">NEXT</span>
              <span className="runsheet-next-btn-item">{nextItem.label}</span>
              <span className="runsheet-next-btn-arrow">→</span>
            </>
          ) : serviceItems.length === 0 ? (
            'Add items to begin'
          ) : (
            'End of service'
          )}
        </button>
      </section>

      {/* ── Announcements ── */}
      <section className="op-panel op-panel-announcement">
        <h2 className="op-panel-heading">Announcements</h2>

        <div className="announcement-add-row">
          <textarea
            className="announcement-body-input"
            placeholder="Announcement text…"
            value={newAnnouncementBody}
            onChange={(e) => setNewAnnouncementBody(e.target.value)}
            rows={3}
          />
          <div className="announcement-add-controls">
            <label className="announcement-duration-label">
              Duration (s)
              <input
                type="number"
                className="announcement-duration-input"
                min={5}
                max={300}
                value={newAnnouncementDuration}
                onChange={(e) => setNewAnnouncementDuration(Number(e.target.value))}
              />
            </label>
            <button
              className="btn btn-primary"
              disabled={!newAnnouncementBody.trim()}
              onClick={handleAddAnnouncement}
            >
              + Add
            </button>
          </div>
        </div>

        {announcements.length > 0 && (
          <ol className="announcement-list">
            {announcements.map((a, i) => (
              <li
                key={a.id}
                className={`announcement-item${announcementIndex === i ? ' announcement-item--active' : ''}`}
              >
                <span className="announcement-item-body">{a.body}</span>
                <span className="announcement-item-dur">{a.durationSecs}s</span>
                <button
                  className="btn btn-sm btn-danger"
                  onClick={() => onRemoveAnnouncement(a.id)}
                >
                  ✕
                </button>
              </li>
            ))}
          </ol>
        )}

        <div className="announcement-controls">
          {!announcementRunning ? (
            <button
              className="btn btn-primary"
              disabled={announcements.length === 0}
              onClick={onStartAnnouncements}
            >
              ▶ Start
            </button>
          ) : (
            <>
              <button className="btn btn-secondary" onClick={onPrevAnnouncement}>
                ← Prev
              </button>
              <button className="btn btn-secondary" onClick={onNextAnnouncement}>
                Next →
              </button>
              <button className="btn btn-danger" onClick={onStopAnnouncements}>
                ■ Stop
              </button>
            </>
          )}
        </div>
      </section>

      {/* ── Settings ── */}
      {!sessionActive && (
        <div className="op-panel settings-panel-inner">
          <h2 className="op-panel-heading">API Keys</h2>
          <p className="settings-hint">
            Transcription: AssemblyAI → Deepgram → Whisper (offline). Detection: OpenAI (primary).
          </p>
          <div className="settings-key-grid">
            {(
              [
                {
                  id: 'assemblyai',
                  label: 'AssemblyAI',
                  tag: 'Recommended',
                  tagPrimary: true,
                  placeholder: 'aai-…',
                  value: assemblyaiKey,
                  onChange: setAssemblyaiKey,
                  cmd: 'set_assemblyai_key',
                },
                {
                  id: 'deepgram',
                  label: 'Deepgram',
                  tag: 'Fallback',
                  tagPrimary: false,
                  placeholder: 'Paste Deepgram key…',
                  value: deepgramKey,
                  onChange: setDeepgramKey,
                  cmd: 'set_deepgram_key',
                },
                {
                  id: 'openai',
                  label: 'OpenAI',
                  tag: 'Verse Detection',
                  tagPrimary: true,
                  placeholder: 'sk-…',
                  value: openaiKey,
                  onChange: setOpenaiKey,
                  cmd: 'set_openai_key',
                },
              ] as const
            ).map(({ id, label, tag, tagPrimary, placeholder, value, onChange, cmd }, i, arr) => (
              <div key={id}>
                <div className="settings-key-row">
                  <div className="settings-key-meta">
                    <span className="settings-key-label">{label}</span>
                    <span
                      className={`settings-key-tag${tagPrimary ? ' settings-key-tag--primary' : ''}`}
                    >
                      {tag}
                    </span>
                  </div>
                  <div className="settings-key-input-row">
                    <input
                      className="settings-input"
                      type="password"
                      placeholder={placeholder}
                      value={value}
                      onChange={(e) => onChange(e.target.value)}
                      onBlur={() => saveKey(id, value, cmd)}
                      autoComplete="off"
                    />
                    <button
                      className="btn btn-secondary btn-sm"
                      onClick={() => saveKey(id, value, cmd)}
                    >
                      Save
                    </button>
                  </div>
                  {savedKey.has(id) && <span className="settings-key-saved">✓ Saved</span>}
                </div>
                {i < arr.length - 1 && <div className="settings-key-divider" />}
              </div>
            ))}
          </div>
        </div>
      )}
    </>
  );
}
