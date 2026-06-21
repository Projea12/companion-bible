import { ManualHymnOverride } from './ManualHymnOverride';

interface HymnTabProps {
  activeHymn: { number: number; title: string } | null;
  hymnSection: { stanzaNumber: number | null; isChorus: boolean; lines: string[] } | null;
  onLoadHymn: (number: number) => void;
  onNextStanza: () => void;
  onPrevStanza: () => void;
}

export function HymnTab({
  activeHymn,
  hymnSection,
  onLoadHymn,
  onNextStanza,
  onPrevStanza,
}: HymnTabProps) {
  return (
    <>
      <ManualHymnOverride onSubmit={onLoadHymn} />
      {activeHymn && (
        <section className="op-panel op-panel-hymn">
          <h2 className="op-panel-heading">
            GHS {activeHymn.number} — {activeHymn.title}
          </h2>
          {hymnSection && (
            <>
              <p className="hymn-section-label">
                {hymnSection.isChorus ? 'Chorus' : `Stanza ${hymnSection.stanzaNumber ?? ''}`}
              </p>
              <div className="hymn-section-lines">
                {hymnSection.lines.map((line, i) => (
                  <p key={i} className="hymn-section-line">
                    {line}
                  </p>
                ))}
              </div>
            </>
          )}
          <div className="hymn-nav-btns">
            <button className="btn btn-secondary hymn-prev-btn" onClick={onPrevStanza}>
              ← Previous
            </button>
            <button className="btn btn-primary hymn-next-btn" onClick={onNextStanza}>
              Next →
            </button>
          </div>
        </section>
      )}
    </>
  );
}
