import { describe, it, expect, vi, beforeEach, afterEach } from 'vitest';
import { setKittyProtocolActive, isKittyProtocolActive } from '../src/input/keys.js';

/**
 * The Kitty keyboard protocol enable (`CSI > flags u`) is a stack PUSH, not an
 * idempotent mode set: every emission adds an entry to the terminal's
 * keyboard-mode stack, and each `CSI < u` pops exactly one. If the terminal
 * writes more pushes than pops over its lifetime, leftover entries keep the
 * enhanced protocol active in the parent shell after exit — Ctrl+C arrives as
 * `CSI 99;5u` instead of 0x03 and the shell becomes uninterruptible.
 *
 * These tests replay the escape-sequence stream that ProcessTerminal writes
 * and assert the stack is balanced: pushes == pops across every lifecycle,
 * including unchanged-dims resize re-asserts.
 */

const KITTY_PUSH = '\x1b[>1u';
const KITTY_POP = '\x1b[<u';
const KITTY_SET = '\x1b[=1;1u';

/** Replays kitty stack operations from a raw write stream. */
function replayKittyStack(raw: string): { pushes: number; pops: number; depth: number } {
	let pushes = 0;
	let pops = 0;
	let depth = 0;
	const re = /\x1b\[([><])(\d*)(?:;\d+)*u/g;
	for (const m of raw.matchAll(re)) {
		if (m[1] === '>') {
			pushes++;
			depth++;
		} else {
			const count = m[2] ? Number.parseInt(m[2], 10) : 1;
			pops += count;
			depth = Math.max(0, depth - count);
		}
	}
	return { pushes, pops, depth };
}

describe('ProcessTerminal kitty keyboard stack balance', () => {
	const envKeys = ['TERM', 'TERM_PROGRAM', 'COLORTERM', 'KITTY_WINDOW_ID'];
	const saved: Record<string, string | undefined> = {};
	let writeSpy: ReturnType<typeof vi.spyOn>;
	const terminals: Array<{ stop(): void }> = [];

	beforeEach(() => {
		for (const k of envKeys) {
			saved[k] = process.env[k];
			delete process.env[k];
		}
		writeSpy = vi.spyOn(process.stdout, 'write').mockReturnValue(true);
		setKittyProtocolActive(false);
	});

	afterEach(() => {
		// Stop leftover terminals so their stdout resize handlers can't fire
		// into a later test, even when an assertion aborted before stop().
		for (const term of terminals.splice(0)) {
			try {
				term.stop();
			} catch {
				// already stopped
			}
		}
		writeSpy.mockRestore();
		for (const k of envKeys) {
			if (saved[k] !== undefined) process.env[k] = saved[k];
			else delete process.env[k];
		}
		setKittyProtocolActive(false);
	});

	function written(): string {
		return writeSpy.mock.calls.map((c) => String(c[0])).join('');
	}

	async function newTerminal() {
		const { ProcessTerminal } = await import('../src/terminal/process-terminal.js');
		const term = new ProcessTerminal();
		terminals.push(term);
		return term;
	}

	it('start → N resizes → stop leaves the stack balanced with a single push', async () => {
		process.env.TERM_PROGRAM = 'iTerm.app'; // known Kitty terminal
		const term = await newTerminal();
		term.start(
			() => {},
			() => {},
		);
		// Same-geometry resizes re-assert modes; none of them may push again.
		process.stdout.emit('resize');
		process.stdout.emit('resize');
		process.stdout.emit('resize');
		term.stop();

		const { pushes, pops, depth } = replayKittyStack(written());
		expect(pushes).toBe(1);
		expect(pops).toBe(1);
		expect(depth).toBe(0);
	});

	it('resize re-assert restores kitty flags via the idempotent set form', async () => {
		process.env.TERM_PROGRAM = 'iTerm.app';
		const term = await newTerminal();
		term.start(
			() => {},
			() => {},
		);
		writeSpy.mockClear();
		process.stdout.emit('resize');
		expect(written()).toContain(KITTY_SET);
		expect(written()).not.toContain(KITTY_PUSH);
		term.stop();
	});

	it('suspend/resume cycles stay balanced', async () => {
		process.env.TERM_PROGRAM = 'iTerm.app';
		const term = await newTerminal();
		term.start(
			() => {},
			() => {},
		);
		term.suspendKeyboard();
		term.resumeKeyboard();
		term.suspendKeyboard();
		term.resumeKeyboard();
		term.stop();

		const { pushes, pops, depth } = replayKittyStack(written());
		expect(pushes).toBe(pops);
		expect(depth).toBe(0);
	});

	it('resetKeyboardModes pops the outstanding push, clears the parser flag, and is idempotent', async () => {
		process.env.TERM_PROGRAM = 'iTerm.app';
		const term = await newTerminal();
		term.start(
			() => {},
			() => {},
		);
		expect(term.kittyProtocolActive).toBe(true);
		writeSpy.mockClear();

		term.resetKeyboardModes();
		expect(written()).toContain(KITTY_POP);
		expect(term.kittyProtocolActive).toBe(false);
		expect(isKittyProtocolActive()).toBe(false);

		writeSpy.mockClear();
		term.resetKeyboardModes();
		expect(written()).not.toContain(KITTY_POP);

		// stop() after a reset must not pop again either.
		writeSpy.mockClear();
		term.stop();
		expect(written()).not.toContain(KITTY_POP);
	});

	it('modifyOtherKeys fallback never touches the kitty stack across resize re-asserts', async () => {
		// No known-Kitty env → startup enables modifyOtherKeys fallback.
		const term = await newTerminal();
		term.start(
			() => {},
			() => {},
		);
		process.stdout.emit('resize');
		term.stop();

		const { pushes, depth } = replayKittyStack(written());
		expect(pushes).toBe(0);
		expect(depth).toBe(0);
	});
});
