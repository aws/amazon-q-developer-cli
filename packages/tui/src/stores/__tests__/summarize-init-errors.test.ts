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

  it('agent rejected for CLI-only fields points at the command that fixes it', () => {
    const errors: InitError[] = [
      {
        type: 'agent_not_found',
        requestedAgent: 'thunder-agent',
        fallbackAgent: KAS_DEFAULT_AGENT_ID,
        skipped: {
          path: '/home/user/.kiro/agents/thunder-agent.json',
          reasonCode: 'cli_only_agent',
          error: 'uses fields this agent engine does not support: allowedTools',
        },
      },
    ];
    expect(summarizeInitErrors(errors)).toBe(
      `agent "thunder-agent" needs upgrading for this agent engine, using "${KAS_DEFAULT_AGENT_ID}" — run /upgrade-agent to convert thunder-agent.json`
    );
  });

  it('agent rejected for an invalid config reports the file it was declared in', () => {
    const errors: InitError[] = [
      {
        type: 'agent_not_found',
        requestedAgent: 'amzn-builder',
        fallbackAgent: KAS_DEFAULT_AGENT_ID,
        skipped: {
          path: '/a/AmazonBuilderCoreAIAgents-amzn-builder.json',
          reasonCode: 'invalid_config',
          error: 'Error: Schema validation failed: tools: Invalid input',
        },
      },
    ];
    expect(summarizeInitErrors(errors)).toBe(
      `agent "amzn-builder" has an invalid config, using "${KAS_DEFAULT_AGENT_ID}" — AmazonBuilderCoreAIAgents-amzn-builder.json: Schema validation failed: tools: Invalid input`
    );
  });

  it('reports an engine-side load failure as the engine failing, not as bad config', () => {
    const errors: InitError[] = [
      {
        type: 'agent_not_found',
        requestedAgent: 'unlucky',
        fallbackAgent: KAS_DEFAULT_AGENT_ID,
        skipped: {
          path: '/a/unlucky.json',
          reasonCode: 'internal_error',
          error: 'EMFILE: too many open files',
        },
      },
    ];
    expect(summarizeInitErrors(errors)).toBe(
      `agent "unlucky" could not be loaded by the agent engine, using "${KAS_DEFAULT_AGENT_ID}" — unlucky.json: EMFILE: too many open files`
    );
  });

  it('unreadable agent config', () => {
    const errors: InitError[] = [
      {
        type: 'agent_not_found',
        requestedAgent: 'locked',
        fallbackAgent: KAS_DEFAULT_AGENT_ID,
        skipped: {
          path: '/a/locked.json',
          reasonCode: 'unreadable',
          error: 'EACCES: permission denied',
        },
      },
    ];
    expect(summarizeInitErrors(errors)).toBe(
      `agent "locked" config could not be read, using "${KAS_DEFAULT_AGENT_ID}" — locked.json: EACCES: permission denied`
    );
  });

  it('falls back to a neutral verdict when the reason code is absent', () => {
    const errors: InitError[] = [
      {
        type: 'agent_not_found',
        requestedAgent: 'mystery',
        fallbackAgent: KAS_DEFAULT_AGENT_ID,
        skipped: { path: '/a/mystery.json', error: 'something new' },
      },
    ];
    expect(summarizeInitErrors(errors)).toBe(
      `agent "mystery" is not usable, using "${KAS_DEFAULT_AGENT_ID}" — mystery.json: something new`
    );
  });

  it('truncates a long defect so the file name stays visible', () => {
    const errors: InitError[] = [
      {
        type: 'agent_not_found',
        requestedAgent: 'verbose',
        fallbackAgent: KAS_DEFAULT_AGENT_ID,
        skipped: {
          path: '/a/verbose.json',
          reasonCode: 'invalid_config',
          error: `Schema validation failed: ${'x'.repeat(200)}`,
        },
      },
    ];
    const message = summarizeInitErrors(errors)!;
    expect(message).toContain('verbose.json: Schema validation failed: ');
    expect(message.endsWith('…')).toBe(true);
    expect(message.length).toBeLessThan(180);
  });

  it('truncates a long defect on a code-point boundary', () => {
    const errors: InitError[] = [
      {
        type: 'agent_not_found',
        requestedAgent: 'emoji',
        fallbackAgent: KAS_DEFAULT_AGENT_ID,
        skipped: {
          path: '/a/emoji.json',
          reasonCode: 'invalid_config',
          // The 90th code point onward is dropped; each 🚀 is two code units, so a
          // code-unit slice would cut one in half.
          error: `Schema validation failed: ${'🚀'.repeat(80)}`,
        },
      },
    ];
    const message = summarizeInitErrors(errors)!;
    expect(message.endsWith('…')).toBe(true);
    expect(message).not.toContain('\ufffd');
    // No lone surrogate survived the cut.
    expect(/[\uD800-\uDBFF](?![\uDC00-\uDFFF])/.test(message)).toBe(false);
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
