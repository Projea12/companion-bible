export interface TranscriptionSegment {
  text: string;
  startMs: number;
  endMs: number;
  confidence: number;
}

export interface TranscriptionResult {
  chunkId: number;
  segments: TranscriptionSegment[];
  fullText: string;
  durationMs: number;
  modelUsed: string;
}
