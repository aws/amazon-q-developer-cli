import { describe, it, expect } from 'bun:test';
import { deriveShellTrustOptions } from './shell-trust-options.js';

describe('deriveShellTrustOptions', () => {
  it('targets the GATED sub-command for a compound shell command', () => {
    // Compound command gated one segment at a time: the whole command is the
    // `resource`, but `echo "done"` is the segment requiring consent now. Both
    // trust options must target that segment, NOT `git *` / the whole command —
    // otherwise nothing authorizes the gated segment and it re-prompts forever.
    const result = deriveShellTrustOptions({
      capability: 'shell',
      resource: 'git status && echo "done"',
      triggeringResource: 'echo "done"',
    });
    expect(result.gatedResource).toBe('echo "done"');
    expect(result.exactResource).toBe('echo "done"');
    expect(result.patternResource).toBe('echo *');
  });

  it('falls back to the whole resource for a single command (no triggeringResource)', () => {
    const result = deriveShellTrustOptions({
      capability: 'shell',
      resource: 'git status',
    });
    expect(result.gatedResource).toBe('git status');
    expect(result.exactResource).toBe('git status');
    expect(result.patternResource).toBe('git *');
  });

  it('also gates on the exec capability', () => {
    const result = deriveShellTrustOptions({
      capability: 'exec',
      resource: 'ls -la && pwd',
      triggeringResource: 'pwd',
    });
    expect(result.exactResource).toBe('pwd');
    // single-token gated segment → no pattern
    expect(result.patternResource).toBeUndefined();
  });

  it('derives no shell pattern for a non-shell capability', () => {
    const result = deriveShellTrustOptions({
      capability: 'fs_write',
      resource: '/etc/hosts now',
      triggeringResource: '/etc/hosts now',
    });
    expect(result.exactResource).toBe('/etc/hosts now');
    expect(result.patternResource).toBeUndefined();
  });

  it('derives no pattern for a single-token gated segment', () => {
    const result = deriveShellTrustOptions({
      capability: 'shell',
      resource: 'ls',
    });
    expect(result.exactResource).toBe('ls');
    expect(result.patternResource).toBeUndefined();
  });

  it('returns undefined values when there is no resource at all', () => {
    const result = deriveShellTrustOptions({ capability: 'shell' });
    expect(result.gatedResource).toBeUndefined();
    expect(result.exactResource).toBeUndefined();
    expect(result.patternResource).toBeUndefined();
  });

  // ── Pattern hardening: a `<token> *` pattern is only offered when it is a
  //    safe, meaningful base command. ──

  it('emits no " *" garbage pattern for leading-whitespace input', () => {
    // Old bug: no trim → first token '' → pattern ' *' (matches everything).
    // After trimming, '  ls' is a single bare token → no pattern at all.
    const result = deriveShellTrustOptions({
      capability: 'shell',
      resource: '  ls',
    });
    expect(result.patternResource).toBeUndefined();
    // Exact stays verbatim (the literal command the agent sent).
    expect(result.exactResource).toBe('  ls');
  });

  it('uses \\s+ tokenization so tab-separated args still yield a pattern', () => {
    // Consistency: the old code gated on includes(' ') but split on /\s+/, so a
    // tab-delimited command was treated as single-token. One rule now governs both.
    const result = deriveShellTrustOptions({
      capability: 'shell',
      resource: 'git\tstatus',
    });
    expect(result.patternResource).toBe('git *');
  });

  it('offers no pattern for a privilege wrapper (sudo)', () => {
    // `sudo *` would authorize every sudo-prefixed command — privilege-wide.
    const result = deriveShellTrustOptions({
      capability: 'shell',
      resource: 'sudo rm -rf /',
    });
    expect(result.exactResource).toBe('sudo rm -rf /');
    expect(result.patternResource).toBeUndefined();
  });

  it('offers no pattern for an env-assignment prefix', () => {
    // `FOO=bar *` is both wrong (assignment is not a command) and won't match.
    const result = deriveShellTrustOptions({
      capability: 'shell',
      resource: 'FOO=bar cmd',
    });
    expect(result.patternResource).toBeUndefined();
  });

  it('offers no pattern when the first token is a literal wildcard', () => {
    const result = deriveShellTrustOptions({
      capability: 'shell',
      resource: '* foo',
    });
    expect(result.patternResource).toBeUndefined();
  });
});
