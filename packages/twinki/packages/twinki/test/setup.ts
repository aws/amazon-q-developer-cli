/**
 * Vitest setup: auto-dump last frame after every test that used a TestTerminal.
 */
import { afterEach } from 'vitest';
import { _getActiveTerminals, _clearActiveTerminals, dumpLastFrame, testDir } from './helpers.js';

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
