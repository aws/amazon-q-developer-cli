import { describe, it, expect, beforeEach, afterEach } from 'bun:test';
import { parseCliArgs, buildAcpArgs } from '../cli-args';

describe('parseCliArgs', () => {
  let originalArgv: string[];

  beforeEach(() => {
    originalArgv = process.argv;
  });

  afterEach(() => {
    process.argv = originalArgv;
  });

  const setArgs = (...args: string[]) => {
    process.argv = ['bun', 'index.ts', ...args];
  };

  it('returns defaults with no args', () => {
    setArgs();
    expect(parseCliArgs()).toEqual({
      trustAllTools: false,
      noInteractive: false,
      resume: false,
      resumePicker: false,
    });
  });

  it('skips "chat" subcommand', () => {
    setArgs('chat');
    expect(parseCliArgs()).toEqual({
      trustAllTools: false,
      noInteractive: false,
      resume: false,
      resumePicker: false,
    });
  });

  it('parses --agent', () => {
    setArgs('chat', '--agent', 'my-agent');
    expect(parseCliArgs().agent).toBe('my-agent');
  });

  it('parses --profile as agent alias', () => {
    setArgs('chat', '--profile', 'my-profile');
    expect(parseCliArgs().agent).toBe('my-profile');
  });

  it('parses --trust-all-tools', () => {
    setArgs('chat', '--trust-all-tools');
    expect(parseCliArgs().trustAllTools).toBe(true);
  });

  it('parses -a as trust-all-tools shorthand', () => {
    setArgs('chat', '-a');
    expect(parseCliArgs().trustAllTools).toBe(true);
  });

  it('parses --no-interactive', () => {
    setArgs('chat', '--no-interactive');
    expect(parseCliArgs().noInteractive).toBe(true);
  });

  it('parses --non-interactive', () => {
    setArgs('chat', '--non-interactive');
    expect(parseCliArgs().noInteractive).toBe(true);
  });

  it('parses positional input', () => {
    setArgs('chat', 'hello world');
    expect(parseCliArgs().input).toBe('hello world');
  });

  it('parses all flags together', () => {
    setArgs(
      'chat',
      '--agent',
      'test',
      '--model',
      'claude-3',
      '--trust-all-tools',
      '--no-interactive',
      'do something'
    );
    const result = parseCliArgs();
    expect(result).toEqual({
      agent: 'test',
      model: 'claude-3',
      trustAllTools: true,
      noInteractive: true,
      resume: false,
      resumePicker: false,
      input: 'do something',
    });
  });

  it('parses --model', () => {
    setArgs('chat', '--model', 'gpt-4');
    expect(parseCliArgs().model).toBe('gpt-4');
  });

  it('parses --model=value syntax', () => {
    setArgs('chat', '--model=claude-opus-4.6-1m');
    expect(parseCliArgs().model).toBe('claude-opus-4.6-1m');
  });

  it('parses --agent=value syntax', () => {
    setArgs('chat', '--agent=my-agent');
    expect(parseCliArgs().agent).toBe('my-agent');
  });

  it('parses --resume-id=value syntax', () => {
    setArgs('chat', '--resume-id=abc-123');
    expect(parseCliArgs().resumeId).toBe('abc-123');
  });

  it('parses --trust-tools=value and forwards to ACP', () => {
    setArgs('chat', '--trust-tools=fs_read,fs_write', '--model', 'gpt-4');
    const result = parseCliArgs();
    expect(result.model).toBe('gpt-4');
    expect(result.trustTools).toEqual(['fs_read', 'fs_write']);
  });

  it('forwards --trust-tools in buildAcpArgs', () => {
    expect(buildAcpArgs({ trustTools: ['read', 'write'] })).toEqual([
      '--trust-tools',
      'read,write',
    ]);
  });

  it('parses positional input after --model', () => {
    setArgs('chat', '--model', 'gpt-4', 'hello');
    const result = parseCliArgs();
    expect(result.model).toBe('gpt-4');
    expect(result.input).toBe('hello');
  });

  it('works without chat subcommand', () => {
    setArgs('--agent', 'direct', 'some input');
    const result = parseCliArgs();
    expect(result.agent).toBe('direct');
    expect(result.input).toBe('some input');
  });

  it('parses --resume', () => {
    setArgs('chat', '--resume');
    expect(parseCliArgs().resume).toBe(true);
  });

  it('parses -r as resume shorthand', () => {
    setArgs('chat', '-r');
    expect(parseCliArgs().resume).toBe(true);
  });

  it('parses --resume-picker', () => {
    setArgs('chat', '--resume-picker');
    expect(parseCliArgs().resumePicker).toBe(true);
  });

  it('parses --list as alias for --resume-picker', () => {
    setArgs('--list');
    expect(parseCliArgs().resumePicker).toBe(true);
  });

  it('does not swallow positional input after --tui', () => {
    setArgs('chat', '--tui', 'Tell me something');
    expect(parseCliArgs().input).toBe('Tell me something');
  });

  it('does not swallow positional input after --v3', () => {
    setArgs('chat', '--v3', 'Tell me something');
    expect(parseCliArgs().input).toBe('Tell me something');
  });

  it('skips --v3 without setting unknown fields', () => {
    setArgs('chat', '--v3');
    expect(parseCliArgs()).toEqual({
      trustAllTools: false,
      noInteractive: false,
      resume: false,
      resumePicker: false,
    });
  });

  it('does not forward --v3 to ACP args', () => {
    // --v3 is consumed by Rust (sets KIRO_AGENT_ENGINE=kas); it must not
    // appear in the ACP arg list.
    setArgs('chat', '--v3', '--model', 'gpt-4');
    const acpArgs = buildAcpArgs(parseCliArgs());
    expect(acpArgs).not.toContain('--v3');
  });

  it('resume flags default to false/undefined', () => {
    setArgs('chat');
    const result = parseCliArgs();
    expect(result.resume).toBe(false);
    expect(result.resumePicker).toBe(false);
    expect(result.resumeId).toBeUndefined();
  });

  it('parses --resume-id with session id', () => {
    setArgs('chat', '--resume-id', 'abc-123');
    const result = parseCliArgs();
    expect(result.resumeId).toBe('abc-123');
  });

  it('--resume-id does not set resume flag', () => {
    setArgs('chat', '--resume-id', 'abc-123');
    const result = parseCliArgs();
    expect(result.resume).toBe(false);
    expect(result.resumeId).toBe('abc-123');
  });

  it('ignores the removed --lite flag without error', () => {
    // `--lite` is no longer a recognized flag (lite mode is reached via
    // `/lite` or the `chat.ui.mode` setting). An unknown flag must be
    // skipped gracefully rather than throwing or consuming the next arg.
    setArgs('chat', '--lite', '--resume-id', 'abc-123');
    const result = parseCliArgs();
    expect(result.resumeId).toBe('abc-123');
  });
});

describe('buildAcpArgs', () => {
  it('returns empty array with defaults', () => {
    expect(buildAcpArgs({})).toEqual([]);
  });

  it('includes --agent when set', () => {
    expect(buildAcpArgs({ agent: 'my-agent' })).toEqual([
      '--agent',
      'my-agent',
    ]);
  });

  it('includes --model when set', () => {
    expect(buildAcpArgs({ model: 'claude-3' })).toEqual([
      '--model',
      'claude-3',
    ]);
  });

  it('includes --trust-all-tools when set', () => {
    expect(buildAcpArgs({ trustAllTools: true })).toEqual([
      '--trust-all-tools',
    ]);
  });

  it('noInteractive is not part of AcpSpawnArgs (enforced by type system)', () => {
    // buildAcpArgs accepts AcpSpawnArgs which has no noInteractive field,
    // so it's impossible to accidentally forward it to the backend.
    const result = buildAcpArgs({ trustAllTools: false });
    expect(result).toEqual([]);
  });

  it('combines agent and trust-all-tools', () => {
    expect(buildAcpArgs({ agent: 'test', trustAllTools: true })).toEqual([
      '--agent',
      'test',
      '--trust-all-tools',
    ]);
  });

  it('combines all flags', () => {
    expect(
      buildAcpArgs({ agent: 'test', model: 'claude-3', trustAllTools: true })
    ).toEqual(['--agent', 'test', '--model', 'claude-3', '--trust-all-tools']);
  });
});

describe('remote sandbox flags (--cloud / --repo)', () => {
  let originalArgv: string[];

  beforeEach(() => {
    originalArgv = process.argv;
  });

  afterEach(() => {
    process.argv = originalArgv;
  });

  const setArgs = (...args: string[]) => {
    process.argv = ['bun', 'index.ts', ...args];
  };

  it('parses --cloud', () => {
    setArgs('chat', '--cloud');
    expect(parseCliArgs().cloud).toBe(true);
  });

  it('parses --repo as a comma-list', () => {
    setArgs('chat', '--repo', 'owner/name,other');
    expect(parseCliArgs().repo).toEqual(['owner/name', 'other']);
  });

  it('parses --repo=value syntax', () => {
    setArgs('chat', '--repo=my-service');
    expect(parseCliArgs().repo).toEqual(['my-service']);
  });

  it('does NOT forward --cloud to the KAS subprocess (no acp mapping)', () => {
    // executionTarget rides _meta.kiro on session/new, never a forwarded
    // subprocess flag — so --cloud must be absent from buildAcpArgs output.
    setArgs('chat', '--cloud', '--model', 'gpt-4');
    const acpArgs = buildAcpArgs(parseCliArgs());
    expect(acpArgs).not.toContain('--cloud');
    expect(acpArgs).toContain('--model'); // sanity: real ACP flags still forward
  });

  it('does NOT forward --repo to the KAS subprocess', () => {
    setArgs('chat', '--repo', 'a,b');
    expect(buildAcpArgs(parseCliArgs())).not.toContain('--repo');
  });

  it('--cloud does not swallow positional input', () => {
    setArgs('chat', '--cloud', 'hello there');
    const r = parseCliArgs();
    expect(r.cloud).toBe(true);
    expect(r.input).toBe('hello there');
  });
});
