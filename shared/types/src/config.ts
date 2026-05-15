import type { BibleTranslation } from './bible.js';

export interface AudioConfig {
  deviceId: string;
  sampleRate: number;
  channels: number;
  chunkDurationMs: number;
}

export interface ModelConfig {
  whisperModel: string;
  phi3Model: string;
  modelsDir: string;
}

export interface AppConfig {
  translation: BibleTranslation;
  audio: AudioConfig;
  models: ModelConfig;
  autoDisplay: boolean;
  showAiContext: boolean;
}

export interface UserPreferences {
  fontSize: number;
  theme: 'light' | 'dark' | 'system';
  displayDurationMs: number;
  enableNotifications: boolean;
}

export const DEFAULT_AUDIO_CONFIG: AudioConfig = {
  deviceId: 'default',
  sampleRate: 16_000,
  channels: 1,
  chunkDurationMs: 3_000,
};

export const DEFAULT_MODEL_CONFIG: ModelConfig = {
  whisperModel: 'whisper-small',
  phi3Model: 'phi3-mini-4k-instruct',
  modelsDir: 'models',
};

export const DEFAULT_APP_CONFIG: AppConfig = {
  translation: 'ESV',
  audio: DEFAULT_AUDIO_CONFIG,
  models: DEFAULT_MODEL_CONFIG,
  autoDisplay: true,
  showAiContext: true,
};

export const DEFAULT_USER_PREFERENCES: UserPreferences = {
  fontSize: 18,
  theme: 'system',
  displayDurationMs: 8_000,
  enableNotifications: true,
};
