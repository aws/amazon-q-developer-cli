import { describe, it, expect, vi, beforeEach, afterEach } from 'vitest';
import { ProcessTerminal } from '../src/terminal/process-terminal.js';
import { isKittyProtocolActive, setKittyProtocolActive } from '../src/input/keys.js';

/**
 * Regression tests for the Ctrl+Z suspend fix.
 *
 * On suspend the enhanced keyboard protocol must be turned OFF at the
 * terminal AND the shared `kittyProtocolActive` parser flag cleared, so the
 * parent shell receives legacy control bytes rather than CSI-u sequences.
 * On resume both must be turned back on together.
 */
describe('ProcessTerminal suspend/resume keyboard', () => {
	let writeSpy: ReturnType<typeof vi.spyOn>;

	beforeEach(() => {
		writeSpy = vi.spyOn(process.stdout, 'write').mockReturnValue(true);
		setKittyProtocolActive(false);
	});

	afterEach(() => {
		writeSpy.mockRestore();
		setKittyProtocolActive(false);
	});

	function written(): string {
		return writeSpy.mock.calls.map((c) => String(c[0])).join('');
	}

	it('suspend pops the Kitty protocol and clears the parser flag; resume restores both', () => {
		const terminal = new ProcessTerminal();
		// Simulate an active Kitty protocol, as start() would establish on a
		// Kitty-capable terminal.
		(terminal as unknown as { enableKittyProtocol(): void }).enableKittyProtocol();
		expect(terminal.kittyProtocolActive).toBe(true);
		expect(isKittyProtocolActive()).toBe(true);
		writeSpy.mockClear();

		terminal.suspendKeyboard();
		expect(written()).toContain('\x1b[<u'); // pop Kitty keyboard
		expect(terminal.kittyProtocolActive).toBe(false);
		expect(isKittyProtocolActive()).toBe(false);
		writeSpy.mockClear();

		terminal.resumeKeyboard();
		expect(written()).toContain('\x1b[>1u'); // re-push Kitty keyboard
		expect(terminal.kittyProtocolActive).toBe(true);
		expect(isKittyProtocolActive()).toBe(true);
	});

	it('suspend/resume are no-ops when no enhanced keyboard mode is active', () => {
		const terminal = new ProcessTerminal();
		terminal.suspendKeyboard();
		expect(written()).not.toContain('\x1b[<u');
		writeSpy.mockClear();
		terminal.resumeKeyboard();
		expect(written()).not.toContain('\x1b[>1u');
		expect(terminal.kittyProtocolActive).toBe(false);
	});

	it('suspend/resume restore modifyOtherKeys without enabling the Kitty protocol', () => {
		const terminal = new ProcessTerminal();
		// Simulate an unknown terminal that fell back to modifyOtherKeys with
		// no Kitty response, as queryAndEnableKittyProtocol() would.
		(terminal as unknown as { enableModifyOtherKeys(): void }).enableModifyOtherKeys();
		expect(terminal.kittyProtocolActive).toBe(false);
		writeSpy.mockClear();

		terminal.suspendKeyboard();
		expect(written()).not.toContain('\x1b[<u'); // no Kitty protocol to pop
		expect(written()).toContain('\x1b[>4;0m'); // modifyOtherKeys disabled
		expect(terminal.kittyProtocolActive).toBe(false);
		expect(isKittyProtocolActive()).toBe(false);
		writeSpy.mockClear();

		terminal.resumeKeyboard();
		expect(written()).toContain('\x1b[>4;1m'); // modifyOtherKeys re-enabled
		expect(written()).not.toContain('\x1b[>1u'); // Kitty protocol NOT enabled
		expect(terminal.kittyProtocolActive).toBe(false);
		expect(isKittyProtocolActive()).toBe(false);
	});
});
