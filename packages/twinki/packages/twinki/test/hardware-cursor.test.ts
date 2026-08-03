import { afterEach, describe, expect, it } from 'vitest';
import { isHardwareCursorEnabled } from '../src/renderer/hardware-cursor.js';
import { TestTerminal } from './helpers.js';
import { TUI } from '../src/renderer/tui.js';

const KEYS = ['TMUX', 'ZELLIJ', 'TWINKI_HARDWARE_CURSOR'] as const;
const saved = new Map<string, string | undefined>(
  KEYS.map((key) => [key, process.env[key]])
);

afterEach(() => {
  for (const [key, value] of saved) {
    if (value === undefined) delete process.env[key];
    else process.env[key] = value;
  }
});

describe('isHardwareCursorEnabled', () => {
  it('is off in a plain terminal', () => {
    for (const key of KEYS) delete process.env[key];
    expect(isHardwareCursorEnabled()).toBe(false);
  });

  it('is on inside tmux or zellij', () => {
    for (const key of KEYS) delete process.env[key];
    process.env.TMUX = '/tmp/tmux-1000/default,1,0';
    expect(isHardwareCursorEnabled()).toBe(true);
    delete process.env.TMUX;
    process.env.ZELLIJ = '0';
    expect(isHardwareCursorEnabled()).toBe(true);
  });

  it('lets TWINKI_HARDWARE_CURSOR override in both directions', () => {
    for (const key of KEYS) delete process.env[key];
    process.env.TWINKI_HARDWARE_CURSOR = '1';
    expect(isHardwareCursorEnabled()).toBe(true);
    process.env.TMUX = '/tmp/tmux-1000/default,1,0';
    process.env.TWINKI_HARDWARE_CURSOR = '0';
    expect(isHardwareCursorEnabled()).toBe(false);
  });
});

/**
 * The environment helper is only the renderer's starting point: a caller may
 * override it at construction or change it later. Anything that paints a cursor
 * has to follow the resolved state, so it is exposed and observable.
 */
describe('resolved hardware cursor state', () => {
	const KEYS = ['TMUX', 'ZELLIJ', 'TWINKI_HARDWARE_CURSOR'] as const;

	it('reports the environment default when no override is given', () => {
		for (const key of KEYS) delete process.env[key];
		expect(new TUI(new TestTerminal(20, 5)).hardwareCursorVisible).toBe(false);
		process.env.TMUX = '/tmp/tmux-1000/default,1,0';
		expect(new TUI(new TestTerminal(20, 5)).hardwareCursorVisible).toBe(true);
	});

	it('reports a constructor override in both directions', () => {
		for (const key of KEYS) delete process.env[key];
		process.env.TMUX = '/tmp/tmux-1000/default,1,0';
		expect(
			new TUI(new TestTerminal(20, 5), { showHardwareCursor: false })
				.hardwareCursorVisible,
		).toBe(false);
		delete process.env.TMUX;
		expect(
			new TUI(new TestTerminal(20, 5), { showHardwareCursor: true })
				.hardwareCursorVisible,
		).toBe(true);
	});

	it('notifies subscribers when the state changes, and only then', () => {
		for (const key of KEYS) delete process.env[key];
		const tui = new TUI(new TestTerminal(20, 5));
		let changes = 0;
		const unsubscribe = tui.onHardwareCursorChange(() => changes++);

		tui.setShowHardwareCursor(true);
		expect([tui.hardwareCursorVisible, changes]).toEqual([true, 1]);

		tui.setShowHardwareCursor(true);
		expect(changes).toBe(1);

		tui.setShowHardwareCursor(false);
		expect([tui.hardwareCursorVisible, changes]).toEqual([false, 2]);

		unsubscribe();
		tui.setShowHardwareCursor(true);
		expect(changes).toBe(2);
	});
});
