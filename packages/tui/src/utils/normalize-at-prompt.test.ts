import { describe, it, expect } from 'bun:test';
import { normalizeAtPrompt } from './normalize-at-prompt';
import type { SlashCommand } from '../stores/app-store';

const promptCmd = (name: string): SlashCommand => ({
  name,
  description: '',
  source: 'backend',
  meta: { type: 'prompt' },
});

const actionCmd = (name: string): SlashCommand => ({
  name,
  description: '',
  source: 'backend',
  meta: { type: 'action' },
});

const commands: SlashCommand[] = [
  promptCmd('/research'),
  promptCmd('/plan'),
  actionCmd('/help'),
];

describe('normalizeAtPrompt', () => {
  it('converts @name to /name when name matches a prompt command', () => {
    expect(normalizeAtPrompt('@research topic', commands)).toBe(
      '/research topic'
    );
  });

  it('preserves arguments after the prompt name', () => {
    expect(normalizeAtPrompt('@plan build a CLI', commands)).toBe(
      '/plan build a CLI'
    );
  });

  it('does not convert when name matches a non-prompt command', () => {
    expect(normalizeAtPrompt('@help', commands)).toBe('@help');
  });

  it('does not convert when name is unknown', () => {
    expect(normalizeAtPrompt('@unknown foo', commands)).toBe('@unknown foo');
  });

  it('returns non-@ input unchanged', () => {
    expect(normalizeAtPrompt('hello world', commands)).toBe('hello world');
  });

  it('returns empty string unchanged', () => {
    expect(normalizeAtPrompt('', commands)).toBe('');
  });

  it('handles bare @ with no name', () => {
    expect(normalizeAtPrompt('@', commands)).toBe('@');
  });

  it('handles @ followed by a space', () => {
    expect(normalizeAtPrompt('@ something', commands)).toBe('@ something');
  });

  it('works with an empty commands list', () => {
    expect(normalizeAtPrompt('@research topic', [])).toBe('@research topic');
  });
});
