import { useCallback, useEffect, useMemo, useRef, useState, type KeyboardEvent } from 'react';
import { suggestBooks, validateReference } from './bibleBooks';

export interface ManualOverrideProps {
  onSubmit: (reference: string) => void;
}

export function ManualOverride({ onSubmit }: ManualOverrideProps) {
  const [input, setInput] = useState('');
  const [activeIdx, setActiveIdx] = useState(-1);
  const [open, setOpen] = useState(false);
  const inputRef = useRef<HTMLInputElement>(null);

  const suggestions = useMemo(() => suggestBooks(input), [input]);
  const validation = useMemo(() => validateReference(input), [input]);

  useEffect(() => {
    setOpen(suggestions.length > 0);
    setActiveIdx(-1);
  }, [suggestions]);

  const applySelection = useCallback((book: string) => {
    setInput(book + ' ');
    setOpen(false);
    setActiveIdx(-1);
    inputRef.current?.focus();
  }, []);

  const handleSubmit = useCallback(() => {
    if (validation !== 'valid') return;
    onSubmit(input.trim());
    setInput('');
    setOpen(false);
  }, [validation, onSubmit, input]);

  const handleKeyDown = useCallback(
    (e: KeyboardEvent<HTMLInputElement>) => {
      if (e.key === 'ArrowDown') {
        e.preventDefault();
        if (open) setActiveIdx((i) => Math.min(i + 1, suggestions.length - 1));
      } else if (e.key === 'ArrowUp') {
        e.preventDefault();
        if (open) setActiveIdx((i) => Math.max(i - 1, -1));
      } else if (e.key === 'Enter') {
        e.preventDefault();
        if (open && activeIdx >= 0 && suggestions[activeIdx]) {
          applySelection(suggestions[activeIdx]);
        } else {
          handleSubmit();
        }
      } else if (e.key === 'Escape') {
        setOpen(false);
        setActiveIdx(-1);
      }
    },
    [open, activeIdx, suggestions, applySelection, handleSubmit],
  );

  const activeDescendant = open && activeIdx >= 0 ? `override-suggestion-${activeIdx}` : undefined;

  return (
    <section className="op-panel op-panel-override" aria-label="Manual override">
      <h2 className="op-panel-heading">Manual Override</h2>
      <div className="override-row">
        <div className="override-combobox">
          <input
            ref={inputRef}
            role="combobox"
            className="override-input"
            placeholder="e.g. John 3:16"
            value={input}
            onChange={(e) => setInput(e.target.value)}
            onKeyDown={handleKeyDown}
            onBlur={() => setTimeout(() => setOpen(false), 150)}
            aria-label="Manual reference override"
            aria-autocomplete="list"
            aria-expanded={open}
            aria-haspopup="listbox"
            aria-controls={open ? 'override-suggestions' : undefined}
            aria-activedescendant={activeDescendant}
            data-validation={input ? validation : undefined}
          />

          {input && (
            <span className="override-validation-icon" aria-hidden="true" data-state={validation}>
              {validation === 'valid' ? '✓' : validation === 'invalid' ? '✗' : ''}
            </span>
          )}

          {open && (
            <ul
              id="override-suggestions"
              className="override-suggestions"
              role="listbox"
              aria-label="Book suggestions"
            >
              {suggestions.map((book, idx) => (
                <li
                  key={book}
                  id={`override-suggestion-${idx}`}
                  role="option"
                  aria-selected={idx === activeIdx}
                  className={`override-suggestion${idx === activeIdx ? ' override-suggestion--active' : ''}`}
                  onMouseDown={(e) => {
                    e.preventDefault();
                    applySelection(book);
                  }}
                >
                  {book}
                </li>
              ))}
            </ul>
          )}
        </div>

        <button
          className="btn btn-primary"
          disabled={validation !== 'valid'}
          onClick={handleSubmit}
          title="Keyboard: Enter"
        >
          Show
        </button>
      </div>
    </section>
  );
}
