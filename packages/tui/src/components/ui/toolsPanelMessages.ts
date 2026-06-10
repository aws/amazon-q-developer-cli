import type { InitError } from '../../stores/app-store.js';

/**
 * Derive the web-tools governance warning shown in the tools panel from the
 * accumulated init errors. Returns `undefined` when web tools are not
 * governance-disabled. Pure (no glyphs/theme/React) so it can be unit-tested.
 */
export function webToolsGovernanceMessage(
  initErrors: InitError[]
): string | undefined {
  const e = initErrors.find(
    (x): x is Extract<InitError, { type: 'web_tools_governance_disabled' }> =>
      x.type === 'web_tools_governance_disabled'
  );
  if (!e) return undefined;
  return e.apiFailure
    ? 'Failed to retrieve web tools settings — web tools disabled'
    : 'Web tools have been disabled by your administrator';
}
