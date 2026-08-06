import { describe, it, expect } from 'bun:test';
import { isCommandVisibleInUiMode } from '../command-menu-utils';
import type { AvailableCommand } from '../../../types/commands';

// Drift guard for the single visibility gate: liteOnly commands bind lite-only
// rendering hooks, so they must stay hidden in tui but visible in lite.
describe('isCommandVisibleInUiMode', () => {
  const plain: AvailableCommand = { name: '/help', description: '' };
  const liteOnly: AvailableCommand = {
    name: '/verbosity',
    description: '',
    meta: { liteOnly: true },
  };
  const tuiOnly: AvailableCommand = {
    name: '/lite',
    description: '',
    meta: { tuiOnly: true },
  };
  const hidden: AvailableCommand = {
    name: '/workflow-run',
    description: '',
    meta: { hidden: true },
  };

  it('shows a plain command in both modes', () => {
    expect(isCommandVisibleInUiMode(plain, 'lite')).toBe(true);
    expect(isCommandVisibleInUiMode(plain, 'tui')).toBe(true);
  });

  it('shows a liteOnly command in lite but hides it in tui', () => {
    expect(isCommandVisibleInUiMode(liteOnly, 'lite')).toBe(true);
    expect(isCommandVisibleInUiMode(liteOnly, 'tui')).toBe(false);
  });

  it('shows a tuiOnly command in tui but hides it in lite', () => {
    expect(isCommandVisibleInUiMode(tuiOnly, 'tui')).toBe(true);
    expect(isCommandVisibleInUiMode(tuiOnly, 'lite')).toBe(false);
  });

  it('hides compatibility commands in both modes', () => {
    expect(isCommandVisibleInUiMode(hidden, 'lite')).toBe(false);
    expect(isCommandVisibleInUiMode(hidden, 'tui')).toBe(false);
  });
});
