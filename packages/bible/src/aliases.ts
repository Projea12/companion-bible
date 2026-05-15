/**
 * Exhaustive alias map for all 66 KJV Bible books.
 *
 * Covers:
 *  - Full canonical names
 *  - Standard abbreviations (SBL, Logos, Accordance)
 *  - Alternate ancient/traditional names
 *  - Ordinal prefix variants: "1", "I", "First", "1st", "One"
 *  - Common spoken forms used in English-speaking churches
 *  - Nigerian church spoken forms (see §Nigerian below)
 *  - Common Whisper ASR transcription artifacts
 *  - Frequent misspellings that survive spell-check
 *
 * ── Nigerian church forms ────────────────────────────────────────────────────
 * Based on documented patterns in Nigerian Pentecostal/Charismatic churches:
 *
 *  - "Revelations"      → Revelation  (ubiquitous — plural form treated as standard)
 *  - "Songs of Solomon" → Song of Solomon
 *  - "Habakuk"          → Habakkuk    (one-k spelling, common in writing + speech)
 *  - "Ecclesiastics"    → Ecclesiastes (adding '-ics' suffix)
 *  - "Nehimiah"         → Nehemiah    (vowel transposition)
 *  - "Ezekial"          → Ezekiel     (vowel swap at end)
 *  - "Obediah"          → Obadiah     (vowel substitution)
 *  - "Zacharias"        → Zechariah   (New Testament spelling carried to OT)
 *  - "Zephania"         → Zephaniah   (dropped terminal h)
 *  - "Phillipians"      → Philippians (double-l misspelling)
 *  - "Corinthian"       → 1/2 Corinthians (singular, no 's') — ambiguous, not included
 *  - "One Kings" etc.   → cardinal ordinal prefix ("One", "Two", "Three")
 *
 * ── Ordinal prefix matrix ────────────────────────────────────────────────────
 * For every numbered book the following prefix forms are covered:
 *   Digit:   "1", "2", "3"
 *   Roman:   "I", "II", "III"
 *   Word:    "First", "Second", "Third"
 *   Ordinal: "1st", "2nd", "3rd"
 *   Cardinal:"One", "Two", "Three"
 */

export interface BookAliasEntry {
  /** Exact name as it appears in kjv.json */
  canonical: string;
  /** First letter capitalised; normalisation (lowercase, trim, strip dots) applied before lookup */
  aliases: string[];
}

// ─── helpers ──────────────────────────────────────────────────────────────────

function cap(s: string): string {
  return s.charAt(0).toUpperCase() + s.slice(1);
}

function ordinal(n: 1 | 2 | 3, base: string[]): string[] {
  const prefixes: Record<1 | 2 | 3, string[]> = {
    1: ['1', 'I', 'First', '1st', 'One'],
    2: ['2', 'II', 'Second', '2nd', 'Two'],
    3: ['3', 'III', 'Third', '3rd', 'Three'],
  };
  return prefixes[n].flatMap((p) => base.map((b) => `${p} ${cap(b)}`));
}

// ─── Old Testament ────────────────────────────────────────────────────────────

export const BOOK_ALIASES: BookAliasEntry[] = [
  {
    canonical: 'Genesis',
    aliases: ['Genesis', 'Gen', 'Ge', 'Gn', 'The book of Genesis'],
  },
  {
    canonical: 'Exodus',
    aliases: ['Exodus', 'Exod', 'Exo', 'Ex', 'The book of Exodus'],
  },
  {
    canonical: 'Leviticus',
    aliases: ['Leviticus', 'Lev', 'Le', 'Lv'],
  },
  {
    canonical: 'Numbers',
    aliases: ['Numbers', 'Num', 'Nu', 'Nm', 'Nb', 'The book of Numbers'],
  },
  {
    canonical: 'Deuteronomy',
    aliases: [
      'Deuteronomy',
      'Deut',
      'Deu',
      'De',
      'Dt',
      'Dueteronomy',
      'Deutronomy',
      'Deuteronmy', // common misspellings
    ],
  },
  {
    canonical: 'Joshua',
    aliases: [
      'Joshua',
      'Josh',
      'Jos',
      'Jsh',
      'Josua', // German form sometimes encountered
    ],
  },
  {
    canonical: 'Judges',
    aliases: ['Judges', 'Judg', 'Jdg', 'Jg', 'Jdgs'],
  },
  {
    canonical: 'Ruth',
    aliases: ['Ruth', 'Rth', 'Ru'],
  },
  {
    canonical: '1 Samuel',
    aliases: [...ordinal(1, ['samuel', 'sam', 'sa', 'sm', 's']), '1samuel'],
  },
  {
    canonical: '2 Samuel',
    aliases: [...ordinal(2, ['samuel', 'sam', 'sa', 'sm', 's']), '2samuel'],
  },
  {
    canonical: '1 Kings',
    aliases: [...ordinal(1, ['kings', 'kgs', 'ki', 'kg', 'king']), '1kings'],
  },
  {
    canonical: '2 Kings',
    aliases: [...ordinal(2, ['kings', 'kgs', 'ki', 'kg', 'king']), '2kings'],
  },
  {
    canonical: '1 Chronicles',
    aliases: [
      ...ordinal(1, ['chronicles', 'chron', 'chr', 'ch', 'chro', 'chronicle']),
      '1chronicles',
    ],
  },
  {
    canonical: '2 Chronicles',
    aliases: [
      ...ordinal(2, ['chronicles', 'chron', 'chr', 'ch', 'chro', 'chronicle']),
      '2chronicles',
    ],
  },
  {
    canonical: 'Ezra',
    aliases: ['Ezra', 'Ezr', 'Ez'],
  },
  {
    canonical: 'Nehemiah',
    aliases: [
      'Nehemiah',
      'Neh',
      'Ne',
      // Nigerian forms
      'Nehimiah',
      'Nehemia',
      'Nehemyah',
      'Nehimia',
    ],
  },
  {
    canonical: 'Esther',
    aliases: ['Esther', 'Est', 'Esth', 'Es'],
  },
  {
    canonical: 'Job',
    aliases: ['Job', 'Jb'],
  },
  {
    canonical: 'Psalms',
    aliases: ['Psalms', 'Psalm', 'Ps', 'Pss', 'Pslm', 'Psa', 'The Psalms'],
  },
  {
    canonical: 'Proverbs',
    aliases: [
      'Proverbs',
      'Prov',
      'Pro',
      'Prv',
      'Pr',
      'Proverb', // singular form
    ],
  },
  {
    canonical: 'Ecclesiastes',
    aliases: [
      'Ecclesiastes',
      'Eccles',
      'Eccle',
      'Ecc',
      'Ec',
      'Qoh',
      // Nigerian forms
      'Ecclesiastics',
      'Ecclesiastic',
      'Eclesiastes',
      'Ecclesiates',
    ],
  },
  {
    canonical: 'Song of Solomon',
    aliases: [
      'Song of Solomon',
      'Song',
      'Sos',
      'Cant',
      'Canticles',
      'Song of Songs',
      'Songs',
      'Song of Sol',
      'Songs of Solomon',
      'Canticle of Canticles',
      'Ss',
      // Nigerian forms
      'Songs of Songs',
    ],
  },
  {
    canonical: 'Isaiah',
    aliases: [
      'Isaiah',
      'Isa',
      'Is',
      'Isaias', // Latin/Douay form
    ],
  },
  {
    canonical: 'Jeremiah',
    aliases: [
      'Jeremiah',
      'Jer',
      'Je',
      'Jr',
      'Jeremias', // Latin form
    ],
  },
  {
    canonical: 'Lamentations',
    aliases: [
      'Lamentations',
      'Lam',
      'La',
      'Lamentation', // singular
      'The Lamentations',
    ],
  },
  {
    canonical: 'Ezekiel',
    aliases: [
      'Ezekiel',
      'Ezek',
      'Eze',
      'Ezk',
      // Nigerian forms
      'Ezekial',
      'Ezeikiel',
      'Ezekeil',
    ],
  },
  {
    canonical: 'Daniel',
    aliases: ['Daniel', 'Dan', 'Da', 'Dn'],
  },
  {
    canonical: 'Hosea',
    aliases: ['Hosea', 'Hos', 'Ho', 'Hoseas', 'Hoshea'],
  },
  {
    canonical: 'Joel',
    aliases: ['Joel', 'Jl'],
  },
  {
    canonical: 'Amos',
    aliases: ['Amos', 'Am'],
  },
  {
    canonical: 'Obadiah',
    aliases: [
      'Obadiah',
      'Obad',
      'Ob',
      // Nigerian forms
      'Obediah',
      'Obadia',
    ],
  },
  {
    canonical: 'Jonah',
    aliases: [
      'Jonah',
      'Jnh',
      'Jon',
      'Jonas', // Greek/NT form (Luke 11:29)
    ],
  },
  {
    canonical: 'Micah',
    aliases: [
      'Micah',
      'Mic',
      'Mc',
      'Micha', // shortened spoken form
    ],
  },
  {
    canonical: 'Nahum',
    aliases: [
      'Nahum',
      'Nah',
      'Na',
      'Nahun', // common spoken/Nigerian misspelling
    ],
  },
  {
    canonical: 'Habakkuk',
    aliases: [
      'Habakkuk',
      'Hab',
      'Hb',
      // Nigerian forms
      'Habakuk',
      'Habakku',
      'Habbakuk',
      'Habakkak',
      'Habacuc',
    ],
  },
  {
    canonical: 'Zephaniah',
    aliases: [
      'Zephaniah',
      'Zeph',
      'Zep',
      'Zp',
      // Nigerian forms
      'Zephania',
    ],
  },
  {
    canonical: 'Haggai',
    aliases: [
      'Haggai',
      'Hag',
      'Hg',
      // Nigerian forms
      'Hagai',
      'Haggi',
    ],
  },
  {
    canonical: 'Zechariah',
    aliases: [
      'Zechariah',
      'Zech',
      'Zec',
      'Zc',
      // Nigerian / NT form crossover
      'Zacharias',
      'Zacharia',
      'Zecharia',
      'Zecharaiah',
    ],
  },
  {
    canonical: 'Malachi',
    aliases: [
      'Malachi',
      'Mal',
      'Ml',
      'Malachai',
      'Malachy',
      'Malaci', // alternate spellings
    ],
  },

  // ─── New Testament ──────────────────────────────────────────────────────────

  {
    canonical: 'Matthew',
    aliases: [
      'Matthew',
      'Matt',
      'Mt',
      'Mathew', // common misspelling (one t)
    ],
  },
  {
    canonical: 'Mark',
    aliases: ['Mark', 'Mrk', 'Mar', 'Mk', 'Mr'],
  },
  {
    canonical: 'Luke',
    aliases: ['Luke', 'Luk', 'Lk'],
  },
  {
    canonical: 'John',
    aliases: [
      'John',
      'Joh',
      'Jhn',
      'Jn',
      // NOTE: "John" alone is ambiguous with 1/2/3 John.
      // The detection engine must resolve by ordinal presence.
    ],
  },
  {
    canonical: 'Acts',
    aliases: ['Acts', 'Act', 'Ac', 'Acts of the Apostles', 'The Acts', 'The Acts of the Apostles'],
  },
  {
    canonical: 'Romans',
    aliases: [
      'Romans',
      'Rom',
      'Ro',
      'Rm',
      'Roman', // singular
    ],
  },
  {
    canonical: '1 Corinthians',
    aliases: [...ordinal(1, ['corinthians', 'cor', 'co', 'corinthian']), '1corinthians'],
  },
  {
    canonical: '2 Corinthians',
    aliases: [...ordinal(2, ['Corinthians', 'Cor', 'Co', 'Corinthian']), '2corinthians'],
  },
  {
    canonical: 'Galatians',
    aliases: [
      'Galatians',
      'Gal',
      'Ga',
      'Galatian', // singular
      'Galations', // common misspelling
    ],
  },
  {
    canonical: 'Ephesians',
    aliases: [
      'Ephesians',
      'Eph',
      'Ephes',
      'Ephesian', // singular
    ],
  },
  {
    canonical: 'Philippians',
    aliases: [
      'Philippians',
      'Phil',
      'Php',
      'Pp',
      // Nigerian forms
      'Phillipians',
      'Philipians',
      'Philippian',
      'Phillippians',
      'Philippeans', // phonetic misspelling
    ],
  },
  {
    canonical: 'Colossians',
    aliases: [
      'Colossians',
      'Col',
      'Colossian', // singular
      'Colosians',
      'Colossions', // misspellings
    ],
  },
  {
    canonical: '1 Thessalonians',
    aliases: [
      ...ordinal(1, ['Thessalonians', 'Thess', 'Thes', 'Th', 'Thessalonian']),
      '1thessalonians',
      // common misspellings
      ...ordinal(1, ['Thessolonians', 'Thesalonians', 'Thessalonicans']),
    ],
  },
  {
    canonical: '2 Thessalonians',
    aliases: [
      ...ordinal(2, ['thessalonians', 'thess', 'thes', 'th', 'thessalonian']),
      '2thessalonians',
      ...ordinal(2, ['thessolonians', 'thesalonians', 'thessalonicans']),
    ],
  },
  {
    canonical: '1 Timothy',
    aliases: [...ordinal(1, ['Timothy', 'Tim', 'Ti']), '1timothy'],
  },
  {
    canonical: '2 Timothy',
    aliases: [...ordinal(2, ['timothy', 'tim', 'ti']), '2timothy'],
  },
  {
    canonical: 'Titus',
    aliases: ['Titus', 'Tit'],
  },
  {
    canonical: 'Philemon',
    aliases: [
      'Philemon',
      'Philem',
      'Phm',
      'Pm',
      'Phillemmon',
      'Phillimon', // Nigerian misspellings
    ],
  },
  {
    canonical: 'Hebrews',
    aliases: [
      'Hebrews',
      'Heb',
      'Hebrew', // singular
    ],
  },
  {
    canonical: 'James',
    aliases: ['James', 'Jas', 'Jm'],
  },
  {
    canonical: '1 Peter',
    aliases: [...ordinal(1, ['peter', 'pet', 'pe', 'pt', 'p']), '1peter'],
  },
  {
    canonical: '2 Peter',
    aliases: [...ordinal(2, ['peter', 'pet', 'pe', 'pt', 'p']), '2peter'],
  },
  {
    canonical: '1 John',
    aliases: [...ordinal(1, ['john', 'jhn', 'jn', 'jo', 'j']), '1john'],
  },
  {
    canonical: '2 John',
    aliases: [...ordinal(2, ['john', 'jhn', 'jn', 'jo', 'j']), '2john'],
  },
  {
    canonical: '3 John',
    aliases: [...ordinal(3, ['John', 'Jhn', 'Jn', 'Jo', 'J']), '3john'],
  },
  {
    canonical: 'Jude',
    aliases: ['Jude', 'Jud', 'Jd'],
  },
  {
    canonical: 'Revelation',
    aliases: [
      'Revelation',
      'Rev',
      'Re',
      'The Revelation',
      // Nigerian forms — "Revelations" is extremely common
      'Revelations',
      'The Revelations',
      'The book of Revelation',
      'The book of Revelations',
      'Apocalypse',
    ],
  },
];

// ─── Flat lookup map ──────────────────────────────────────────────────────────

/** Normalise a raw string before alias lookup */
export function normalizeBookInput(raw: string): string {
  return raw
    .toLowerCase()
    .trim()
    .replace(/\./g, '') // strip abbreviation dots
    .replace(/\s+/g, ' '); // collapse whitespace
}

/**
 * Map from every normalised alias → canonical book name.
 * Built once at module load; O(1) lookups at runtime.
 */
export const ALIAS_LOOKUP: ReadonlyMap<string, string> = new Map(
  BOOK_ALIASES.flatMap(({ canonical, aliases }) => [
    [normalizeBookInput(canonical), canonical] as [string, string],
    ...aliases.map((a) => [normalizeBookInput(a), canonical] as [string, string]),
  ]),
);

/**
 * Resolve a raw book name (any form) to its canonical KJV name.
 * Returns `undefined` when no alias matches.
 */
export function resolveBook(raw: string): string | undefined {
  return ALIAS_LOOKUP.get(normalizeBookInput(raw));
}
