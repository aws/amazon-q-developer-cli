import { describe, it, expect } from 'bun:test';
import { webToolsGovernanceMessage } from '../toolsPanelMessages.js';
import type { InitError } from '../../../stores/app-store.js';

describe('webToolsGovernanceMessage', () => {
  it('returns undefined when no web tools governance error is present', () => {
    expect(webToolsGovernanceMessage([])).toBeUndefined();
    const others: InitError[] = [
      { type: 'mcp_governance_disabled', apiFailure: false },
      { type: 'agent_not_found', requestedAgent: 'x', fallbackAgent: 'y' },
    ];
    expect(webToolsGovernanceMessage(others)).toBeUndefined();
  });

  it('returns the admin-disabled message when not an API failure', () => {
    const errors: InitError[] = [
      { type: 'web_tools_governance_disabled', apiFailure: false },
    ];
    expect(webToolsGovernanceMessage(errors)).toBe(
      'Web tools have been disabled by your administrator'
    );
  });

  it('returns the API-failure message when apiFailure is true', () => {
    const errors: InitError[] = [
      { type: 'web_tools_governance_disabled', apiFailure: true },
    ];
    expect(webToolsGovernanceMessage(errors)).toBe(
      'Failed to retrieve web tools settings — web tools disabled'
    );
  });
});
