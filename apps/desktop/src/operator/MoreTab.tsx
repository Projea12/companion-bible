import { useState } from 'react';
import { invoke } from '@tauri-apps/api/core';

interface MoreTabProps {
  // Service items
  serviceItems: { id: number; label: string; isCurrent: boolean }[];
  currentServiceLabel: string | null;
  onAddServiceItem: (label: string) => void;
  onRemoveServiceItem: (id: number) => void;
  onSetCurrentServiceItem: (id: number) => void;
  onNextServiceItem: () => void;
  onClearServiceItems: () => void;
  // Announcements
  announcements: { id: number; body: string; durationSecs: number }[];
  announcementRunning: boolean;
  announcementIndex: number | null;
  onAddAnnouncement: (body: string, durationSecs: number) => void;
  onRemoveAnnouncement: (id: number) => void;
  onStartAnnouncements: () => void;
  onStopAnnouncements: () => void;
  onNextAnnouncement: () => void;
  onPrevAnnouncement: () => void;
  // Settings
  sessionActive: boolean;
}

export function MoreTab({
  serviceItems,
  currentServiceLabel,
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
  // Form state — local to this tab
  const [newServiceItemLabel, setNewServiceItemLabel] = useState('');
  const [newAnnouncementBody, setNewAnnouncementBody] = useState('');
  const [newAnnouncementDuration, setNewAnnouncementDuration] = useState(30);

  // API key state — local to this tab
  const [assemblyaiKey, setAssemblyaiKey] = useState('');
  const [deepgramKey, setDeepgramKey] = useState('');
  const [openaiKey, setOpenaiKey] = useState('');
  const [savedKey, setSavedKey] = useState<Set<string>>(new Set());

  const saveKey = (service: string, key: string, cmd: string) => {
    void invoke(cmd, { key });
    setSavedKey((prev) => new Set([...prev, service]));
  };

  const handleAddServiceItem = () => {
    const label = newServiceItemLabel.trim().toUpperCase();
    if (!label) return;
    onAddServiceItem(label);
    setNewServiceItemLabel('');
  };

  const handleAddAnnouncement = () => {
    const body = newAnnouncementBody.trim();
    if (!body) return;
    onAddAnnouncement(body, newAnnouncementDuration);
    setNewAnnouncementBody('');
  };

  return (
    <>
      {/* ── Order of Service ── */}
      <section className="op-panel op-panel-service">
        <div className="service-panel-header">
          <h2 className="op-panel-heading">Order of Service</h2>
          {currentServiceLabel && (
            <span className="service-now-badge">● {currentServiceLabel}</span>
          )}
        </div>

        <div className="service-add-row">
          <input
            className="service-add-input"
            placeholder="e.g. OPENING PRAYER"
            value={newServiceItemLabel}
            onChange={(e) => setNewServiceItemLabel(e.target.value)}
            onKeyDown={(e) => {
              if (e.key === 'Enter') handleAddServiceItem();
            }}
          />
          <button
            className="btn btn-primary btn-sm"
            disabled={!newServiceItemLabel.trim()}
            onClick={handleAddServiceItem}
          >
            + Add
          </button>
        </div>

        {serviceItems.length > 0 && (
          <ol className="service-list">
            {serviceItems.map((item) => (
              <li
                key={item.id}
                className={`service-item${item.isCurrent ? ' service-item--active' : ''}`}
                onClick={() => onSetCurrentServiceItem(item.id)}
                title="Click to set as current"
              >
                <span className="service-item-dot" aria-hidden="true">
                  {item.isCurrent ? '●' : '○'}
                </span>
                <span className="service-item-label">{item.label}</span>
                <button
                  className="btn btn-sm btn-danger service-item-remove"
                  onClick={(e) => {
                    e.stopPropagation();
                    onRemoveServiceItem(item.id);
                  }}
                  title="Remove"
                >
                  ✕
                </button>
              </li>
            ))}
          </ol>
        )}

        <div className="service-controls">
          <button
            className="btn btn-secondary btn-sm"
            disabled={serviceItems.length === 0}
            onClick={onNextServiceItem}
          >
            Next →
          </button>
          <button
            className="btn btn-sm btn-danger"
            disabled={serviceItems.length === 0}
            onClick={onClearServiceItems}
          >
            Clear All
          </button>
        </div>
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
