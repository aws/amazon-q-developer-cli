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
    });
  });

  it('skips "chat" subcommand', () => {
    setArgs('chat');
    expect(parseCliArgs()).toEqual({
      trustAllTools: false,
      noInteractive: false,
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
      input: 'do something',
    });
  });

  it('parses --model', () => {
    setArgs('chat', '--model', 'gpt-4');
    expect(parseCliArgs().model).toBe('gpt-4');
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
