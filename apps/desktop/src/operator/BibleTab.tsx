import { ChapterBrowserPanel } from './ChapterBrowserPanel';
import { ManualOverride } from './ManualOverride';

interface BibleTabProps {
  chapterBook: string | null;
  chapterNum: number | null;
  chapterActiveVerse: number | null;
  onSelectVerse: (ref: string) => void;
  onManualSubmit: (ref: string) => void;
}

export function BibleTab({
  chapterBook,
  chapterNum,
  chapterActiveVerse,
  onSelectVerse,
  onManualSubmit,
}: BibleTabProps) {
  return (
    <>
      <ManualOverride onSubmit={onManualSubmit} />
      <ChapterBrowserPanel
        book={chapterBook}
        chapter={chapterNum}
        activeVerse={chapterActiveVerse}
        onSelectVerse={onSelectVerse}
      />
    </>
  );
}
