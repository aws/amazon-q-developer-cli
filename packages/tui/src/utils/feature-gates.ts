/**
 * Feature gates resolved by the Rust launcher from rollout.json.
 * KIRO_ENABLED_FEATURES is a JSON array of enabled feature names (e.g. ["voice","lite"]).
 * KIRO_INTERNAL=1 when the user authenticated via Amazon-internal SSO.
 */

const enabledFeatures: ReadonlySet<string> = new Set(
  JSON.parse(process.env.KIRO_ENABLED_FEATURES || '[]') as string[]
);

/** True when the user authenticated via Amazon-internal SSO. Set by the Rust launcher. */
export const isInternalUser = process.env.KIRO_INTERNAL === '1';

/** True when the voice rollout is enabled for this user (segment + percent + channel). */
export const isVoiceEnabled = enabledFeatures.has('voice');

/** True when lite mode rollout is enabled. */
export const isLiteEnabled = enabledFeatures.has('lite');
