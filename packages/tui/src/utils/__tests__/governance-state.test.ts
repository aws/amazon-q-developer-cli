import { describe, it, expect } from 'bun:test';
import { webToolsGovernanceFromState } from '../governance-state.js';

describe('webToolsGovernanceFromState', () => {
  it('returns undefined when web tools are enabled', () => {
    expect(
      webToolsGovernanceFromState({
        sessionId: 's1',
        isEnterprise: true,
        features: { webToolsEnabled: true },
      })
    ).toBeUndefined();
  });

  it('returns undefined when features are absent (non-enterprise)', () => {
    expect(webToolsGovernanceFromState({ sessionId: 's1' })).toBeUndefined();
  });

  it('flags admin-disabled web tools as apiFailure=false', () => {
    expect(
      webToolsGovernanceFromState({
        features: { webToolsEnabled: false },
        disabledReason: 'admin_disabled',
      })
    ).toEqual({ apiFailure: false });
  });

  it('flags api_failure as apiFailure=true', () => {
    expect(
      webToolsGovernanceFromState({
        features: { webToolsEnabled: false },
        disabledReason: 'api_failure',
      })
    ).toEqual({ apiFailure: true });
  });

  it('flags no_endpoint as apiFailure=true', () => {
    expect(
      webToolsGovernanceFromState({
        features: { webToolsEnabled: false },
        disabledReason: 'no_endpoint',
      })
    ).toEqual({ apiFailure: true });
  });

  it('defaults to apiFailure=false when reason is missing', () => {
    expect(
      webToolsGovernanceFromState({ features: { webToolsEnabled: false } })
    ).toEqual({ apiFailure: false });
  });
});
