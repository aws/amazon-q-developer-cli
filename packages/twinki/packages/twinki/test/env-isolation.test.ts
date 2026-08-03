import { describe, expect, it } from 'vitest';

/**
 * Pins the hermetic-runner contract: the renderer shows the terminal's own
 * cursor under a multiplexer, so a suite that inherits these variables emits
 * different bytes than CI. Fails if the setup file stops clearing them.
 */
describe('test environment', () => {
	it('leaves no multiplexer markers in the environment', () => {
		expect(process.env.TMUX).toBeUndefined();
		expect(process.env.ZELLIJ).toBeUndefined();
		expect(process.env.TWINKI_HARDWARE_CURSOR).toBeUndefined();
	});
});
