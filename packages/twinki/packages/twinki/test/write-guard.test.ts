import { describe, it, expect, vi, beforeEach, afterEach } from 'vitest';
import { CURSOR_MARKER } from '../src/renderer/component.js';

describe('ProcessTerminal.write APC guard', () => {
	let writeSpy: ReturnType<typeof vi.spyOn>;

	beforeEach(() => {
		writeSpy = vi.spyOn(process.stdout, 'write').mockReturnValue(true);
	});

	afterEach(() => {
		writeSpy.mockRestore();
	});

	// Import dynamically so the spy is in place
	async function getTerminal() {
		const { ProcessTerminal } = await import('../src/terminal/process-terminal.js');
		return new ProcessTerminal();
	}

	it('strips CURSOR_MARKER from write buffer', async () => {
		const term = await getTerminal();
		term.write(`hello${CURSOR_MARKER}world`);
		expect(writeSpy).toHaveBeenCalledWith('helloworld');
	});

	it('strips multiple APC sequences', async () => {
		const term = await getTerminal();
		term.write(`a${CURSOR_MARKER}b${CURSOR_MARKER}c`);
		expect(writeSpy).toHaveBeenCalledWith('abc');
	});

	it('preserves normal ANSI escape sequences', async () => {
		const term = await getTerminal();
		const data = '\x1b[31mred\x1b[0m';
		term.write(data);
		expect(writeSpy).toHaveBeenCalledWith(data);
	});

	it('preserves ST-terminated APC (Kitty graphics)', async () => {
		const term = await getTerminal();
		const kitty = '\x1b_Gf=32;data\x1b\\';
		term.write(kitty);
		expect(writeSpy).toHaveBeenCalledWith(kitty);
	});

	it('leaves unterminated APC in place', async () => {
		const term = await getTerminal();
		const data = 'test\x1b_no-bel-terminator';
		term.write(data);
		expect(writeSpy).toHaveBeenCalledWith(data);
	});
});
