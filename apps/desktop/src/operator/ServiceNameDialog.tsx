import { useState } from 'react';

function getGreeting(): string {
  const h = new Date().getHours();
  if (h < 12) return 'Good morning';
  if (h < 17) return 'Good afternoon';
  return 'Good evening';
}

interface ServiceNameDialogProps {
  onSubmit: (name: string) => void;
  onSkip: () => void;
}

export function ServiceNameDialog({ onSubmit, onSkip }: ServiceNameDialogProps) {
  const [name, setName] = useState('SUNDAY WORSHIP SERVICE');

  const handleSubmit = () => {
    const trimmed = name.trim();
    if (trimmed) onSubmit(trimmed);
  };

  return (
    <div className="snd-backdrop">
      <div className="snd-surface">
        <p className="snd-greeting">{getGreeting()}</p>
        <h2 className="snd-question">What service are we holding today?</h2>
        <input
          className="snd-input"
          value={name}
          onChange={(e) => setName(e.target.value)}
          onKeyDown={(e) => {
            if (e.key === 'Enter') handleSubmit();
          }}
          autoFocus
          spellCheck={false}
        />
        <button className="snd-btn-primary" onClick={handleSubmit}>
          Let's Go
        </button>
        <button className="snd-btn-skip" onClick={onSkip}>
          Skip for now
        </button>
      </div>
    </div>
  );
}
