import { useCallback, useState, type KeyboardEvent } from 'react';

export interface ManualHymnOverrideProps {
  onSubmit: (number: number) => void;
}

function parseHymnNumber(raw: string): number | null {
  const trimmed = raw.trim().replace(/^ghs\s*/i, '');
  const n = parseInt(trimmed, 10);
  if (isNaN(n) || n < 1 || n > 260) return null;
  return n;
}

export function ManualHymnOverride({ onSubmit }: ManualHymnOverrideProps) {
  const [input, setInput] = useState('');

  const parsed = parseHymnNumber(input);
  const valid = parsed !== null;

  const handleSubmit = useCallback(() => {
    if (!valid || parsed === null) return;
    onSubmit(parsed);
    setInput('');
  }, [valid, parsed, onSubmit]);

  const handleKeyDown = useCallback(
    (e: KeyboardEvent<HTMLInputElement>) => {
      if (e.key === 'Enter') {
        e.preventDefault();
        handleSubmit();
      }
    },
    [handleSubmit],
  );

  return (
    <section className="op-panel op-panel-override" aria-label="Manual hymn override">
      <h2 className="op-panel-heading">Load Hymn</h2>
      <div className="override-row">
        <div className="override-combobox">
          <input
            className="override-input"
            placeholder="e.g. 42 or GHS 42"
            value={input}
            onChange={(e) => setInput(e.target.value)}
            onKeyDown={handleKeyDown}
            aria-label="Hymn number"
            data-validation={input ? (valid ? 'valid' : 'invalid') : undefined}
          />
          {input && (
            <span
              className="override-validation-icon"
              aria-hidden="true"
              data-state={valid ? 'valid' : 'invalid'}
            >
              {valid ? '✓' : '✗'}
            </span>
          )}
        </div>
        <button
          className="btn btn-primary"
          disabled={!valid}
          onClick={handleSubmit}
          title="Keyboard: Enter"
        >
          Show
        </button>
      </div>
    </section>
  );
}
