/**
 * Vitest setup: auto-dump last frame after every test that used a TestTerminal.
 */
import { afterEach, beforeEach } from 'vitest';
import { _getActiveTerminals, _clearActiveTerminals, dumpLastFrame, testDir } from './helpers.js';

// The renderer shows the terminal's own cursor under a multiplexer, so a suite
// run from inside tmux would emit different bytes than CI. Tests that want that
// path set these vars themselves.
beforeEach(() => {
	for (const key of ['TMUX', 'ZELLIJ', 'TWINKI_HARDWARE_CURSOR']) {
		delete process.env[key];
	}
});

afterEach((ctx) => {
	const terminals = _getActiveTerminals();
	if (terminals.length === 0) return;

	const suite = ctx.task.suite?.name ?? 'unknown';
	const name = ctx.task.name;
	const dir = testDir(suite, name);

	for (const terminal of terminals) {
		dumpLastFrame(terminal, dir);
	}

	_clearActiveTerminals();
});
