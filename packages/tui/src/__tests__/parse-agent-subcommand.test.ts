import { describe, it, expect } from 'bun:test';
import { parseAgentSubcommand } from '../acp-client';

describe('parseAgentSubcommand', () => {
  it('treats empty / missing args as list', () => {
    expect(parseAgentSubcommand(undefined)).toEqual({ kind: 'list' });
    expect(parseAgentSubcommand({})).toEqual({ kind: 'list' });
    expect(parseAgentSubcommand({ value: '' })).toEqual({ kind: 'list' });
    expect(parseAgentSubcommand({ value: '   ' })).toEqual({ kind: 'list' });
  });

  it('recognizes the `swap <name>` verb', () => {
    expect(parseAgentSubcommand({ value: 'swap docs' })).toEqual({
      kind: 'swap',
      name: 'docs',
    });
    expect(parseAgentSubcommand({ value: '  swap   research  ' })).toEqual({
      kind: 'swap',
      name: 'research',
    });
  });

  it('preserves spaces inside agent names after the verb', () => {
    expect(parseAgentSubcommand({ value: 'swap my agent' })).toEqual({
      kind: 'swap',
      name: 'my agent',
    });
  });

  it('treats bare values without a verb as swap shorthand', () => {
    // This path matches what the selection menu produces (no "swap " prefix).
    expect(parseAgentSubcommand({ value: 'research' })).toEqual({
      kind: 'swap',
      name: 'research',
    });
    expect(parseAgentSubcommand({ agentName: 'research' })).toEqual({
      kind: 'swap',
      name: 'research',
    });
  });

  it('swap with no name returns an empty-named swap (caller surfaces usage)', () => {
    expect(parseAgentSubcommand({ value: 'swap' })).toEqual({
      kind: 'swap',
      name: '',
    });
  });

  it('parses `create` with and without a name', () => {
    expect(parseAgentSubcommand({ value: 'create' })).toEqual({
      kind: 'create',
      name: undefined,
    });
    expect(parseAgentSubcommand({ value: 'create my-agent' })).toEqual({
      kind: 'create',
      name: 'my-agent',
    });
  });

  it('parses `edit` with and without a name', () => {
    expect(parseAgentSubcommand({ value: 'edit' })).toEqual({
      kind: 'edit',
      name: undefined,
    });
    expect(parseAgentSubcommand({ value: 'edit my-agent' })).toEqual({
      kind: 'edit',
      name: 'my-agent',
    });
  });

  it('matches verbs case-insensitively', () => {
    expect(parseAgentSubcommand({ value: 'CREATE foo' })).toEqual({
      kind: 'create',
      name: 'foo',
    });
    expect(parseAgentSubcommand({ value: 'Edit bar' })).toEqual({
      kind: 'edit',
      name: 'bar',
    });
    expect(parseAgentSubcommand({ value: 'SWAP baz' })).toEqual({
      kind: 'swap',
      name: 'baz',
    });
  });

  it('prefers agentName over value when both are present', () => {
    expect(
      parseAgentSubcommand({ agentName: 'alpha', value: 'swap beta' })
    ).toEqual({ kind: 'swap', name: 'alpha' });
  });
});
