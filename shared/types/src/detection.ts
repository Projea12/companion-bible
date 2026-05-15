import type { BibleReference } from './bible.js';

export interface DetectionMatch {
  reference: BibleReference;
  rawText: string;
  confidence: number;
  startIndex: number;
  endIndex: number;
}

export interface DetectionResult {
  matches: DetectionMatch[];
  sourceText: string;
  processedAt: number;
}
