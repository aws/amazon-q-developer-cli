import { describe, expect, it } from 'bun:test';
import { KAS_DEFAULT_AGENT_ID } from '../../constants/agents.js';
import { summarizeInitErrors, type InitError } from '../app-store.js';

describe('summarizeInitErrors', () => {
  it('returns null for empty errors', () => {
    expect(summarizeInitErrors([])).toBeNull();
  });

  // --- Single errors ---

  it('single MCP failure', () => {
    const errors: InitError[] = [
      { type: 'mcp_failure', serverName: 'broken-srv', error: 'No such file' },
    ];
    expect(summarizeInitErrors(errors)).toBe('1 MCP failure — see /mcp');
  });

  it('single agent not found', () => {
    const errors: InitError[] = [
      {
        type: 'agent_not_found',
        requestedAgent: 'foo',
        fallbackAgent: KAS_DEFAULT_AGENT_ID,
      },
    ];
    expect(summarizeInitErrors(errors)).toBe(
      `agent "foo" not found, using "${KAS_DEFAULT_AGENT_ID}"`
    );
  });

  it('single agent config error with path', () => {
    const errors: InitError[] = [
      {
        type: 'agent_config_error',
        path: '/home/user/agents/bad.json',
        error: 'parse error',
      },
    ];
    expect(summarizeInitErrors(errors)).toBe('invalid agent config: bad.json');
  });

  it('single agent config error without path', () => {
    const errors: InitError[] = [
      { type: 'agent_config_error', error: 'something broke' },
    ];
    expect(summarizeInitErrors(errors)).toBe('invalid agent config: unknown');
  });

  // --- Multiple same-type errors ---

  it('multiple MCP failures', () => {
    const errors: InitError[] = [
      { type: 'mcp_failure', serverName: 'a', error: 'err' },
      { type: 'mcp_failure', serverName: 'b', error: 'err' },
    ];
    expect(summarizeInitErrors(errors)).toBe('2 MCP failures — see /mcp');
  });

  it('2 agent config errors shows both filenames', () => {
    const errors: InitError[] = [
      { type: 'agent_config_error', path: '/p/one.json', error: 'e' },
      { type: 'agent_config_error', path: '/p/two.json', error: 'e' },
    ];
    expect(summarizeInitErrors(errors)).toBe(
      'invalid agent config: one.json, two.json'
    );
  });

  it('3 agent config errors shows all filenames', () => {
    const errors: InitError[] = [
      { type: 'agent_config_error', path: '/a.json', error: 'e' },
      { type: 'agent_config_error', path: '/b.json', error: 'e' },
      { type: 'agent_config_error', path: '/c.json', error: 'e' },
    ];
    expect(summarizeInitErrors(errors)).toBe(
      'invalid agent config: a.json, b.json, c.json'
    );
  });

  it('4+ agent config errors truncates with +N more', () => {
    const errors: InitError[] = [
      { type: 'agent_config_error', path: '/a.json', error: 'e' },
      { type: 'agent_config_error', path: '/b.json', error: 'e' },
      { type: 'agent_config_error', path: '/c.json', error: 'e' },
      { type: 'agent_config_error', path: '/d.json', error: 'e' },
      { type: 'agent_config_error', path: '/e.json', error: 'e' },
    ];
    expect(summarizeInitErrors(errors)).toBe(
      'invalid agent config: a.json, b.json, c.json +2 more'
    );
  });

  // --- Mixed errors ---

  it('agent not found + MCP failure', () => {
    const errors: InitError[] = [
      {
        type: 'agent_not_found',
        requestedAgent: 'foo',
        fallbackAgent: KAS_DEFAULT_AGENT_ID,
      },
      { type: 'mcp_failure', serverName: 'srv', error: 'err' },
    ];
    expect(summarizeInitErrors(errors)).toBe(
      `agent "foo" not found, using "${KAS_DEFAULT_AGENT_ID}"; 1 MCP failure — see /mcp`
    );
  });

  it('agent config error + MCP failures', () => {
    const errors: InitError[] = [
      { type: 'agent_config_error', path: '/bad.json', error: 'e' },
      { type: 'mcp_failure', serverName: 'a', error: 'e' },
      { type: 'mcp_failure', serverName: 'b', error: 'e' },
    ];
    expect(summarizeInitErrors(errors)).toBe(
      'invalid agent config: bad.json; 2 MCP failures — see /mcp'
    );
  });

  it('all three error types combined', () => {
    const errors: InitError[] = [
      {
        type: 'agent_not_found',
        requestedAgent: 'missing',
        fallbackAgent: KAS_DEFAULT_AGENT_ID,
      },
      { type: 'agent_config_error', path: '/broken.json', error: 'e' },
      { type: 'mcp_failure', serverName: 'srv', error: 'e' },
    ];
    expect(summarizeInitErrors(errors)).toBe(
      `agent "missing" not found, using "${KAS_DEFAULT_AGENT_ID}"; invalid agent config: broken.json; 1 MCP failure — see /mcp`
    );
  });

  // --- Governance ---

  it('coalesces MCP + web tools into one message on shared API failure', () => {
    const errors: InitError[] = [
      { type: 'mcp_governance_disabled', apiFailure: true },
      { type: 'web_tools_governance_disabled', apiFailure: true },
    ];
    expect(summarizeInitErrors(errors)).toBe(
      'failed to retrieve governance settings — MCP and web tools disabled'
    );
  });

  it('keeps MCP + web tools separate when admin-disabled (not API failure)', () => {
    const errors: InitError[] = [
      { type: 'mcp_governance_disabled', apiFailure: false },
      { type: 'web_tools_governance_disabled', apiFailure: false },
    ];
    expect(summarizeInitErrors(errors)).toBe(
      'MCP disabled by your administrator; web tools disabled by your administrator'
    );
  });

  it('web tools only — admin disabled', () => {
    const errors: InitError[] = [
      { type: 'web_tools_governance_disabled', apiFailure: false },
    ];
    expect(summarizeInitErrors(errors)).toBe(
      'web tools disabled by your administrator'
    );
  });

  // --- Path handling ---

  it('handles Windows-style paths', () => {
    const errors: InitError[] = [
      {
        type: 'agent_config_error',
        path: 'C:\\Users\\dev\\agents\\bad.json',
        error: 'e',
      },
    ];
    expect(summarizeInitErrors(errors)).toBe('invalid agent config: bad.json');
  });

  it('handles bare filename (no directory)', () => {
    const errors: InitError[] = [
      { type: 'agent_config_error', path: 'agent.json', error: 'e' },
    ];
    expect(summarizeInitErrors(errors)).toBe(
      'invalid agent config: agent.json'
    );
  });
});
