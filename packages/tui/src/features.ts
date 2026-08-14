import { Feature } from './types/generated/chat-internal';

export { Feature };

class FeatureManager {
  private enabled: ReadonlySet<string> | null = null;

  private get enabledSet(): ReadonlySet<string> {
    if (this.enabled === null) {
      this.enabled = parseEnabledFeatures(process.env.KIRO_ENABLED_FEATURES);
    }
    return this.enabled;
  }

  isEnabled(feature: Feature): boolean {
    if (!this.enabledSet.has(feature)) return false;
    return feature !== Feature.Voice || isVoiceInputAvailable();
  }

  get isInternalUser(): boolean {
    return process.env.KIRO_INTERNAL === '1';
  }

  _resetForTests(): void {
    this.enabled = null;
  }
}

function parseEnabledFeatures(raw: string | undefined): ReadonlySet<string> {
  if (!raw) return new Set();
  try {
    const parsed: unknown = JSON.parse(raw);
    if (Array.isArray(parsed)) {
      return new Set(parsed.filter((f): f is string => typeof f === 'string'));
    }
  } catch {
    // Malformed: all features off.
  }
  return new Set();
}

export function isVoiceInputAvailable(): boolean {
  if (process.env.KIRO_VOICE_SERVER_URL?.trim()) return true;
  return process.env.KIRO_VOICE_SUPPORTED !== '0';
}

export const features = new FeatureManager();
