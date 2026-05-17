export const BIBLE_BOOKS = [
  // Old Testament
  'Genesis',
  'Exodus',
  'Leviticus',
  'Numbers',
  'Deuteronomy',
  'Joshua',
  'Judges',
  'Ruth',
  '1 Samuel',
  '2 Samuel',
  '1 Kings',
  '2 Kings',
  '1 Chronicles',
  '2 Chronicles',
  'Ezra',
  'Nehemiah',
  'Esther',
  'Job',
  'Psalms',
  'Proverbs',
  'Ecclesiastes',
  'Song of Solomon',
  'Isaiah',
  'Jeremiah',
  'Lamentations',
  'Ezekiel',
  'Daniel',
  'Hosea',
  'Joel',
  'Amos',
  'Obadiah',
  'Jonah',
  'Micah',
  'Nahum',
  'Habakkuk',
  'Zephaniah',
  'Haggai',
  'Zechariah',
  'Malachi',
  // New Testament
  'Matthew',
  'Mark',
  'Luke',
  'John',
  'Acts',
  'Romans',
  '1 Corinthians',
  '2 Corinthians',
  'Galatians',
  'Ephesians',
  'Philippians',
  'Colossians',
  '1 Thessalonians',
  '2 Thessalonians',
  '1 Timothy',
  '2 Timothy',
  'Titus',
  'Philemon',
  'Hebrews',
  'James',
  '1 Peter',
  '2 Peter',
  '1 John',
  '2 John',
  '3 John',
  'Jude',
  'Revelation',
] as const;

export type BibleBook = (typeof BIBLE_BOOKS)[number];

const BOOK_SET = new Set(BIBLE_BOOKS.map((b) => b.toLowerCase()));

export function suggestBooks(input: string): string[] {
  const trimmed = input.trim();
  if (!trimmed) return [];
  const lower = trimmed.toLowerCase();
  // If the raw input ends with a space and the trimmed portion is an exact book
  // match, the user just confirmed a book — no suggestions needed.
  if (input.endsWith(' ') && BOOK_SET.has(lower)) return [];
  // Once a recognised book name is followed by a space and digit, the user is
  // typing the chapter/verse — no suggestions needed.
  for (const book of BIBLE_BOOKS) {
    const bl = book.toLowerCase();
    if (lower.startsWith(bl + ' ') && /\d/.test(trimmed.slice(bl.length + 1))) {
      return [];
    }
  }
  return BIBLE_BOOKS.filter((b) => b.toLowerCase().startsWith(lower)).slice(0, 6);
}

export type ValidationState = 'empty' | 'valid' | 'invalid';

export function validateReference(input: string): ValidationState {
  const trimmed = input.trim();
  if (!trimmed) return 'empty';
  const match = /^(.+?)\s+(\d+)(?::(\d+))?$/.exec(trimmed);
  if (!match) return 'invalid';
  const [, bookRaw, chapterStr, verseStr] = match;
  if (!BOOK_SET.has(bookRaw.trim().toLowerCase())) return 'invalid';
  if (parseInt(chapterStr, 10) < 1) return 'invalid';
  if (verseStr !== undefined && parseInt(verseStr, 10) < 1) return 'invalid';
  return 'valid';
}
