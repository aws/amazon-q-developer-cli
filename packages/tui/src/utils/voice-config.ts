/**
 * Persistent voice settings stored at ~/.kiro/voice.json
 */

import fs from 'fs';
import path from 'path';
import os from 'os';

interface VoiceConfig {
  voiceAutoSubmit: boolean;
  voiceHintIndex: number;
}

function configPath(): string {
  return path.join(os.homedir(), '.kiro', 'voice.json');
}

export function loadVoiceConfig(): VoiceConfig {
  try {
    const raw = fs.readFileSync(configPath(), 'utf-8');
    const parsed = JSON.parse(raw) as Partial<VoiceConfig>;
    return {
      voiceAutoSubmit: parsed.voiceAutoSubmit ?? true,
      voiceHintIndex: parsed.voiceHintIndex ?? 0,
    };
  } catch {
    return { voiceAutoSubmit: true, voiceHintIndex: 0 };
  }
}

export function saveVoiceConfig(config: Partial<VoiceConfig>): void {
  try {
    const current = loadVoiceConfig();
    const updated = { ...current, ...config };
    const dir = path.dirname(configPath());
    if (!fs.existsSync(dir)) {
      fs.mkdirSync(dir, { recursive: true });
    }
    fs.writeFileSync(configPath(), JSON.stringify(updated, null, 2));
  } catch {
    // Non-fatal: config just won't persist
  }
}
