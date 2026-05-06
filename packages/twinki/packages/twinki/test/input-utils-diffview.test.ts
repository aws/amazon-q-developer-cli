/**
 * Tests for input key parsing (Kitty protocol, key matching, modifiers),
 * DiffView component rendering, and stdin-buffer edge cases.
 */
import { describe, it, expect, beforeEach, afterEach, vi } from 'vitest';
import { matchesKey, parseKey, setKittyProtocolActive, isKeyRelease, isKeyRepeat } from '../src/input/keys.js';
import { StdinBuffer } from '../src/input/stdin-buffer.js';
import { AnsiCodeTracker, extractAnsiCode } from '../src/utils/ansi.js';
import { wrapTextWithAnsi } from '../src/utils/wrap-ansi.js';
import { applyYogaProps, createYogaNode, getBorderChars } from '../src/layout/yoga.js';
import { halfBlock, stamp, renderGrid, createGrid, solidBg, radialGlow, COLOR_RGB } from '../src/animation/index.js';

// ============================================================
// 1. keys.ts — Kitty protocol, modifiers, uncovered branches
// ============================================================

describe('keys.ts coverage', () => {
	afterEach(() => setKittyProtocolActive(false));

	describe('isKeyRelease', () => {
		it('detects all release variants', () => {
			expect(isKeyRelease('\x1b[97;1:3u')).toBe(true);
			expect(isKeyRelease('\x1b[3;1:3~')).toBe(true);
			expect(isKeyRelease('\x1b[1;1:3A')).toBe(true);
			expect(isKeyRelease('\x1b[1;1:3B')).toBe(true);
			expect(isKeyRelease('\x1b[1;1:3C')).toBe(true);
			expect(isKeyRelease('\x1b[1;1:3D')).toBe(true);
			expect(isKeyRelease('\x1b[1;1:3H')).toBe(true);
			expect(isKeyRelease('\x1b[1;1:3F')).toBe(true);
			expect(isKeyRelease('a')).toBe(false);
		});
	});

	describe('isKeyRepeat', () => {
		it('detects all repeat variants', () => {
			expect(isKeyRepeat('\x1b[97;1:2u')).toBe(true);
			expect(isKeyRepeat('\x1b[3;1:2~')).toBe(true);
			expect(isKeyRepeat('\x1b[1;1:2A')).toBe(true);
			expect(isKeyRepeat('\x1b[1;1:2D')).toBe(true);
			expect(isKeyRepeat('\x1b[1;1:2H')).toBe(true);
			expect(isKeyRepeat('\x1b[1;1:2F')).toBe(true);
			expect(isKeyRepeat('\x1b[97u')).toBe(false);
		});
	});

	describe('Kitty CSI u — modifier combos', () => {
		beforeEach(() => setKittyProtocolActive(true));

		it('ctrl+shift+a', () => expect(matchesKey('\x1b[97;6u', 'ctrl+shift+a')).toBe(true));
		it('alt+a', () => expect(matchesKey('\x1b[97;3u', 'alt+a')).toBe(true));
		it('ctrl+alt+a', () => expect(matchesKey('\x1b[97;7u', 'ctrl+alt+a')).toBe(true));
		it('shift+space', () => expect(matchesKey('\x1b[32;2u', 'shift+space')).toBe(true));
		it('ctrl+space', () => expect(matchesKey('\x1b[32;5u', 'ctrl+space')).toBe(true));
		it('ctrl+tab', () => expect(matchesKey('\x1b[9;5u', 'ctrl+tab')).toBe(true));
		it('alt+tab', () => expect(matchesKey('\x1b[9;3u', 'alt+tab')).toBe(true));
		it('ctrl+backspace', () => expect(matchesKey('\x1b[127;5u', 'ctrl+backspace')).toBe(true));
		it('shift+backspace', () => expect(matchesKey('\x1b[127;2u', 'shift+backspace')).toBe(true));
		it('ctrl+enter', () => expect(matchesKey('\x1b[13;5u', 'ctrl+enter')).toBe(true));
		it('numpad enter', () => expect(matchesKey('\x1b[57414u', 'enter')).toBe(true));
		it('shift+numpad enter', () => expect(matchesKey('\x1b[57414;2u', 'shift+enter')).toBe(true));
	});

	describe('Kitty arrows with modifiers', () => {
		beforeEach(() => setKittyProtocolActive(true));
		it('ctrl+up', () => expect(matchesKey('\x1b[1;5A', 'ctrl+up')).toBe(true));
		it('shift+down', () => expect(matchesKey('\x1b[1;2B', 'shift+down')).toBe(true));
		it('alt+left', () => expect(matchesKey('\x1b[1;3D', 'alt+left')).toBe(true));
		it('ctrl+right', () => expect(matchesKey('\x1b[1;5C', 'ctrl+right')).toBe(true));
	});

	describe('Kitty functional keys with modifiers', () => {
		beforeEach(() => setKittyProtocolActive(true));
		it('shift+delete', () => expect(matchesKey('\x1b[3;2~', 'shift+delete')).toBe(true));
		it('ctrl+insert', () => expect(matchesKey('\x1b[2;5~', 'ctrl+insert')).toBe(true));
		it('shift+home via H', () => expect(matchesKey('\x1b[1;2H', 'shift+home')).toBe(true));
		it('ctrl+end via F', () => expect(matchesKey('\x1b[1;5F', 'ctrl+end')).toBe(true));
		it('shift+pageUp', () => expect(matchesKey('\x1b[5;2~', 'shift+pageUp')).toBe(true));
		it('ctrl+pageDown', () => expect(matchesKey('\x1b[6;5~', 'ctrl+pageDown')).toBe(true));
	});

	describe('legacy modifier sequences', () => {
		it('shift+up/down', () => {
			expect(matchesKey('\x1b[a', 'shift+up')).toBe(true);
			expect(matchesKey('\x1b[b', 'shift+down')).toBe(true);
		});
		it('ctrl+up/down', () => {
			expect(matchesKey('\x1bOa', 'ctrl+up')).toBe(true);
			expect(matchesKey('\x1bOb', 'ctrl+down')).toBe(true);
		});
		it('shift+insert/delete', () => {
			expect(matchesKey('\x1b[2$', 'shift+insert')).toBe(true);
			expect(matchesKey('\x1b[3^', 'ctrl+delete')).toBe(true);
		});
		it('shift+home/end', () => {
			expect(matchesKey('\x1b[7$', 'shift+home')).toBe(true);
			expect(matchesKey('\x1b[8^', 'ctrl+end')).toBe(true);
		});
		it('shift+pageUp/pageDown', () => {
			expect(matchesKey('\x1b[5$', 'shift+pageUp')).toBe(true);
			expect(matchesKey('\x1b[6^', 'ctrl+pageDown')).toBe(true);
		});
		it('shift/ctrl+clear', () => {
			expect(matchesKey('\x1b[e', 'shift+clear')).toBe(true);
			expect(matchesKey('\x1bOe', 'ctrl+clear')).toBe(true);
		});
	});

	describe('legacy ctrl+space, alt+space, alt+letter, ctrl+alt', () => {
		it('ctrl+space NUL', () => expect(matchesKey('\x00', 'ctrl+space')).toBe(true));
		it('alt+space', () => expect(matchesKey('\x1b ', 'alt+space')).toBe(true));
		it('alt+a/z', () => {
			expect(matchesKey('\x1ba', 'alt+a')).toBe(true);
			expect(matchesKey('\x1bz', 'alt+z')).toBe(true);
		});
		it('ctrl+alt+a', () => expect(matchesKey('\x1b\x01', 'ctrl+alt+a')).toBe(true));
	});

	describe('ctrl+symbol keys', () => {
		it('ctrl+_ and ctrl+-', () => {
			expect(matchesKey('\x1f', 'ctrl+_')).toBe(true);
			expect(matchesKey('\x1f', 'ctrl+-')).toBe(true);
		});
		it('ctrl+[', () => expect(matchesKey('\x1b', 'ctrl+[')).toBe(true));
	});

	describe('function keys f5-f12', () => {
		it('f5-f12', () => {
			expect(matchesKey('\x1b[15~', 'f5')).toBe(true);
			expect(matchesKey('\x1b[17~', 'f6')).toBe(true);
			expect(matchesKey('\x1b[18~', 'f7')).toBe(true);
			expect(matchesKey('\x1b[19~', 'f8')).toBe(true);
			expect(matchesKey('\x1b[20~', 'f9')).toBe(true);
			expect(matchesKey('\x1b[21~', 'f10')).toBe(true);
			expect(matchesKey('\x1b[23~', 'f11')).toBe(true);
			expect(matchesKey('\x1b[24~', 'f12')).toBe(true);
		});
		it('ctrl+f1 not supported', () => expect(matchesKey('\x1bOP', 'ctrl+f1')).toBe(false));
	});

	describe('matchesKey edge cases', () => {
		it('invalid keyId', () => expect(matchesKey('a', '' as any)).toBe(false));
		it('escape with modifier', () => expect(matchesKey('\x1b', 'ctrl+escape' as any)).toBe(false));
		it('symbol keys', () => {
			expect(matchesKey('/', '/')).toBe(true);
			expect(matchesKey('.', '.')).toBe(true);
		});
		it('alt+backspace via \\x08', () => expect(matchesKey('\x1b\x08', 'alt+backspace')).toBe(true));
		it('enter via SS3 M', () => expect(matchesKey('\x1bOM', 'enter')).toBe(true));
		it('\\n as enter in legacy', () => {
			setKittyProtocolActive(false);
			expect(matchesKey('\n', 'enter')).toBe(true);
		});
		it('\\n NOT enter in Kitty', () => {
			setKittyProtocolActive(true);
			expect(matchesKey('\n', 'enter')).toBe(false);
		});
	});

	describe('shift+enter / alt+enter mode-dependent', () => {
		it('\\x1b\\r as shift+enter in Kitty', () => {
			setKittyProtocolActive(true);
			expect(matchesKey('\x1b\r', 'shift+enter')).toBe(true);
			expect(matchesKey('\n', 'shift+enter')).toBe(true);
		});
		it('\\x1b\\r as alt+enter in legacy', () => {
			setKittyProtocolActive(false);
			expect(matchesKey('\x1b\r', 'alt+enter')).toBe(true);
		});
		it('alt+enter via Kitty CSI u', () => {
			setKittyProtocolActive(true);
			expect(matchesKey('\x1b[13;3u', 'alt+enter')).toBe(true);
		});
	});

	describe('parseKey — Kitty extended', () => {
		beforeEach(() => setKittyProtocolActive(true));
		it('shift+a', () => expect(parseKey('\x1b[97;2u')).toBe('shift+a'));
		it('alt+a', () => expect(parseKey('\x1b[97;3u')).toBe('alt+a'));
		it('ctrl+shift+a', () => expect(parseKey('\x1b[97;6u')).toBe('shift+ctrl+a'));
		it('space/backspace/tab', () => {
			expect(parseKey('\x1b[32u')).toBe('space');
			expect(parseKey('\x1b[127u')).toBe('backspace');
			expect(parseKey('\x1b[9u')).toBe('tab');
		});
		it('symbol key /', () => expect(parseKey('\x1b[47u')).toBe('/'));
		it('arrow with modifier', () => {
			expect(parseKey('\x1b[1;5A')).toBe('ctrl+up');
			expect(parseKey('\x1b[1;3D')).toBe('alt+left');
		});
		it('functional with modifier', () => {
			expect(parseKey('\x1b[3;2~')).toBe('shift+delete');
			expect(parseKey('\x1b[5;5~')).toBe('ctrl+pageUp');
		});
		it('home/end with modifier', () => {
			expect(parseKey('\x1b[1;2H')).toBe('shift+home');
			expect(parseKey('\x1b[1;5F')).toBe('ctrl+end');
		});
	});

	describe('parseKey — legacy extras', () => {
		it('alt+backspace', () => {
			expect(parseKey('\x1b\x7f')).toBe('alt+backspace');
			expect(parseKey('\x1b\x08')).toBe('alt+backspace');
		});
		it('ctrl+_', () => expect(parseKey('\x1f')).toBe('ctrl+_'));
		it('f1-f4 SS3', () => {
			expect(parseKey('\x1bOP')).toBe('f1');
			expect(parseKey('\x1bOS')).toBe('f4');
		});
		it('modifyOtherKeys', () => {
			expect(parseKey('\x1b[27;5;32~')).toBe('ctrl+space');
			expect(parseKey('\x1b[27;3;9~')).toBe('alt+tab');
			expect(parseKey('\x1b[27;5;127~')).toBe('ctrl+backspace');
			expect(parseKey('\x1b[27;5;27~')).toBe('ctrl+escape');
			expect(parseKey('\x1b[27;5;97~')).toBe('ctrl+a');
		});
	});

	describe('Kitty base layout key fallback', () => {
		beforeEach(() => setKittyProtocolActive(true));
		it('Cyrillic with Latin base matches ctrl+c', () => {
			expect(matchesKey('\x1b[1089::99;5u', 'ctrl+c')).toBe(true);
		});
		it('Latin codepoint ignores base layout key', () => {
			expect(matchesKey('\x1b[97::118u', 'a')).toBe(true);
			expect(matchesKey('\x1b[97::118u', 'v')).toBe(false);
		});
	});
});

// ============================================================
// 2. stdin-buffer.ts — chunked input, paste, edge cases
// ============================================================

describe('StdinBuffer coverage', () => {
	let buffer: StdinBuffer;
	beforeEach(() => { buffer = new StdinBuffer({ timeout: 10 }); });
	afterEach(() => { buffer.destroy(); });

	it('emits empty string for empty input with empty buffer', () => {
		const h = vi.fn();
		buffer.on('data', h);
		buffer.process('');
		expect(h).toHaveBeenCalledWith('');
	});

	it('handles Buffer with high byte (>127)', () => {
		const h = vi.fn();
		buffer.on('data', h);
		buffer.process(Buffer.from([0xE1]));
		expect(h).toHaveBeenCalledWith('\x1ba');
	});

	it('handles Buffer normally for multi-byte', () => {
		const h = vi.fn();
		buffer.on('data', h);
		buffer.process(Buffer.from('hello'));
		expect(h).toHaveBeenCalledWith('hello');
	});

	it('emits control characters individually', () => {
		const h = vi.fn();
		buffer.on('data', h);
		buffer.process('\x01\x02');
		expect(h).toHaveBeenCalledWith('\x01');
		expect(h).toHaveBeenCalledWith('\x02');
	});

	it('handles paste with content before paste start', () => {
		const dh = vi.fn(), ph = vi.fn();
		buffer.on('data', dh);
		buffer.on('paste', ph);
		buffer.process('abc\x1b[200~pasted\x1b[201~');
		expect(dh).toHaveBeenCalledWith('abc');
		expect(ph).toHaveBeenCalledWith('pasted');
	});

	it('handles paste with remaining data after paste end', () => {
		const dh = vi.fn(), ph = vi.fn();
		buffer.on('data', dh);
		buffer.on('paste', ph);
		buffer.process('\x1b[200~pasted\x1b[201~xyz');
		expect(ph).toHaveBeenCalledWith('pasted');
		expect(dh).toHaveBeenCalledWith('xyz');
	});

	it('handles paste end in second chunk', () => {
		const ph = vi.fn();
		buffer.on('paste', ph);
		buffer.process('\x1b[200~hello');
		buffer.process(' world\x1b[201~');
		expect(ph).toHaveBeenCalledWith('hello world');
	});

	it('flush returns empty when buffer empty', () => {
		expect(buffer.flush()).toEqual([]);
	});

	it('clear resets paste mode', () => {
		buffer.process('\x1b[200~partial');
		buffer.clear();
		expect(buffer.getBuffer()).toBe('');
		const h = vi.fn();
		buffer.on('data', h);
		buffer.process('normal');
		expect(h).toHaveBeenCalledWith('normal');
	});

	it('handles OSC sequence', () => {
		const h = vi.fn();
		buffer.on('data', h);
		buffer.process('\x1b]8;;https://example.com\x07');
		expect(h).toHaveBeenCalledTimes(1);
	});

	it('handles DCS sequence', () => {
		const h = vi.fn();
		buffer.on('data', h);
		buffer.process('\x1bPdata\x1b\\');
		expect(h).toHaveBeenCalledTimes(1);
	});

	it('handles APC sequence', () => {
		const h = vi.fn();
		buffer.on('data', h);
		buffer.process('\x1b_data\x1b\\');
		expect(h).toHaveBeenCalledTimes(1);
	});

	it('handles SGR mouse sequence', () => {
		const h = vi.fn();
		buffer.on('data', h);
		buffer.process('\x1b[<0;10;20M');
		expect(h).toHaveBeenCalledTimes(1);
	});

	it('handles old-style mouse (ESC[M + 3 bytes)', () => {
		const h = vi.fn();
		buffer.on('data', h);
		buffer.process('\x1b[M !!');
		expect(h).toHaveBeenCalledTimes(1);
	});

	it('handles meta key sequence', () => {
		const h = vi.fn();
		buffer.on('data', h);
		buffer.process('\x1bx');
		expect(h).toHaveBeenCalledWith('\x1bx');
	});
});

// ============================================================
// 3. DiffView.tsx — component rendering via reconciler
// ============================================================

describe('DiffView coverage', () => {
	it('renders diff with changes', async () => {
		const React = await import('react');
		const { DiffView } = await import('../src/components/DiffView.js');
		const { render } = await import('../src/reconciler/render.js');
		const pkg = await import('@xterm/headless');
		const { Terminal: XtermTerminal } = pkg.default;

		const term = createMinimalTerm(XtermTerminal, 120, 20);
		const inst = render(
			React.createElement(DiffView, { values: ['line1\nline2\nline3', 'line1\nmodified\nline3\nline4'] }),
			{ terminal: term as any, exitOnCtrlC: false },
		);
		await new Promise(r => setTimeout(r, 100));
		await term.flush();
		const vp = term.getViewport();
		expect(vp.join('\n').length).toBeGreaterThan(0);
		inst.unmount();
	});

	it('renders identical content', async () => {
		const React = await import('react');
		const { DiffView } = await import('../src/components/DiffView.js');
		const { render } = await import('../src/reconciler/render.js');
		const pkg = await import('@xterm/headless');
		const { Terminal: XtermTerminal } = pkg.default;

		const term = createMinimalTerm(XtermTerminal, 80, 10);
		const inst = render(
			React.createElement(DiffView, { values: ['same', 'same'] }),
			{ terminal: term as any, exitOnCtrlC: false },
		);
		await new Promise(r => setTimeout(r, 50));
		await term.flush();
		inst.unmount();
	});

	it('renders with horizontal layout', async () => {
		const React = await import('react');
		const { DiffView } = await import('../src/components/DiffView.js');
		const { render } = await import('../src/reconciler/render.js');
		const pkg = await import('@xterm/headless');
		const { Terminal: XtermTerminal } = pkg.default;

		const term = createMinimalTerm(XtermTerminal, 140, 15);
		const inst = render(
			React.createElement(DiffView, { values: ['old line', 'new line'], layout: 'horizontal' }),
			{ terminal: term as any, exitOnCtrlC: false },
		);
		await new Promise(r => setTimeout(r, 100));
		await term.flush();
		inst.unmount();
	});

	it('renders with DiffSide objects (startLine)', async () => {
		const React = await import('react');
		const { DiffView } = await import('../src/components/DiffView.js');
		const { render } = await import('../src/reconciler/render.js');
		const pkg = await import('@xterm/headless');
		const { Terminal: XtermTerminal } = pkg.default;

		const term = createMinimalTerm(XtermTerminal, 80, 15);
		const inst = render(
			React.createElement(DiffView, {
				values: [
					{ content: 'old', startLine: 10 },
					{ content: 'new', startLine: 10 },
				],
			}),
			{ terminal: term as any, exitOnCtrlC: false },
		);
		await new Promise(r => setTimeout(r, 100));
		await term.flush();
		inst.unmount();
	});
});

// Minimal terminal factory for component tests
function createMinimalTerm(XtermTerminal: any, cols: number, rows: number) {
	const xterm = new XtermTerminal({ cols, rows, allowProposedApi: true });
	let inputHandler: ((data: string) => void) | undefined;
	return {
		xterm,
		get kittyProtocolActive() { return false; },
		get columns() { return cols; },
		get rows() { return rows; },
		start(onInput: (data: string) => void) { inputHandler = onInput; },
		stop() {},
		async drainInput() {},
		write(data: string) { xterm.write(data); },
		sendInput(data: string) { inputHandler?.(data); },
		moveBy(n: number) { if (n > 0) xterm.write(`\x1b[${n}B`); else if (n < 0) xterm.write(`\x1b[${-n}A`); },
		hideCursor() { xterm.write('\x1b[?25l'); },
		showCursor() { xterm.write('\x1b[?25h'); },
		clearLine() { xterm.write('\x1b[K'); },
		clearFromCursor() { xterm.write('\x1b[J'); },
		clearScreen() { xterm.write('\x1b[2J\x1b[H'); },
		setTitle() {},
		enableMouse() {},
		disableMouse() {},
		async flush() { return new Promise<void>(r => xterm.write('', r)); },
		getViewport(): string[] {
			const buf = xterm.buffer.active;
			const lines: string[] = [];
			for (let i = 0; i < rows; i++) {
				const line = buf.getLine(buf.viewportY + i);
				lines.push(line ? line.translateToString(true) : '');
			}
			return lines;
		},
	};
}

// ============================================================
// 4. Input.ts — uncovered methods
// ============================================================

describe('Input component coverage', () => {
	let input: any;
	beforeEach(async () => {
		const { Input } = await import('../src/components/Input.js');
		input = new Input();
		input.focused = true;
	});

	it('word movement backwards (alt+b)', () => {
		input.handleInput('hello world');
		input.handleInput('\x1bb');
		input.handleInput('X');
		expect(input.getValue()).toBe('hello Xworld');
	});

	it('word movement forwards (alt+f)', () => {
		input.handleInput('hello world');
		input.handleInput('\x01');
		input.handleInput('\x1bf');
		input.handleInput('X');
		expect(input.getValue()).toBe('helloX world');
	});

	it('delete word backwards (alt+backspace)', () => {
		input.handleInput('hello world');
		input.handleInput('\x1b\x7f');
		expect(input.getValue()).toBe('hello ');
	});

	it('delete word forward (alt+d)', () => {
		input.handleInput('hello world');
		input.handleInput('\x01');
		input.handleInput('\x1bd');
		expect(input.getValue()).toBe(' world');
	});

	it('delete to line start (ctrl+u)', () => {
		input.handleInput('hello world');
		input.handleInput('\x15');
		expect(input.getValue()).toBe('');
	});

	it('cursor right at end is no-op', () => {
		input.handleInput('ab');
		input.handleInput('\x1b[C');
		expect(input.getValue()).toBe('ab');
	});

	it('cursor left at start is no-op', () => {
		input.handleInput('ab');
		input.handleInput('\x01');
		input.handleInput('\x1b[D');
		expect(input.getValue()).toBe('ab');
	});

	it('forward delete at end is no-op', () => {
		input.handleInput('ab');
		input.handleInput('\x1b[3~');
		expect(input.getValue()).toBe('ab');
	});

	it('backspace at start is no-op', () => {
		input.handleInput('ab');
		input.handleInput('\x01');
		input.handleInput('\x7f');
		expect(input.getValue()).toBe('ab');
	});

	it('delete to line start at start is no-op', () => {
		input.handleInput('ab');
		input.handleInput('\x01');
		input.handleInput('\x15');
		expect(input.getValue()).toBe('ab');
	});

	it('delete to line end at end is no-op', () => {
		input.handleInput('ab');
		input.handleInput('\x0b');
		expect(input.getValue()).toBe('ab');
	});

	it('renders with horizontal scrolling', () => {
		input.handleInput('a'.repeat(100));
		const lines = input.render(40);
		expect(lines.length).toBe(1);
	});

	it('renders cursor in middle of long text', () => {
		input.handleInput('a'.repeat(100));
		for (let i = 0; i < 50; i++) input.handleInput('\x1b[D');
		expect(input.render(40).length).toBe(1);
	});

	it('renders cursor near start of long text', () => {
		input.handleInput('a'.repeat(100));
		input.handleInput('\x01');
		for (let i = 0; i < 5; i++) input.handleInput('\x1b[C');
		expect(input.render(40).length).toBe(1);
	});

	it('render returns just prompt for zero width', () => {
		expect(input.render(2)[0]).toBe('> ');
	});

	it('invalidate is a no-op', () => {
		expect(() => input.invalidate()).not.toThrow();
	});

	it('handles split bracketed paste', () => {
		input.handleInput('\x1b[200~first');
		input.handleInput(' part\x1b[201~');
		expect(input.getValue()).toContain('first part');
	});
});

// ============================================================
// 5. Markdown.tsx — tables, blockquotes, nested lists, hr, links
// ============================================================

describe('Markdown coverage', () => {
	it('renders all block types', async () => {
		const React = await import('react');
		const { Markdown } = await import('../src/components/Markdown.js');
		const { render } = await import('../src/reconciler/render.js');
		const pkg = await import('@xterm/headless');
		const { Terminal: XtermTerminal } = pkg.default;

		const md = `# Heading\n\n| Col1 | Col2 |\n|------|------|\n| a | b |\n\n> blockquote\n\n---\n\n- item 1\n  - nested\n- item 2\n\n**bold** *italic* ~~strike~~ \`code\` [link](url)\n\n![image](img.png)\n`;

		const term = createMinimalTerm(XtermTerminal, 80, 20);
		const inst = render(
			React.createElement(Markdown, { children: md }),
			{ terminal: term as any, exitOnCtrlC: false },
		);
		await new Promise(r => setTimeout(r, 50));
		await term.flush();
		const vp = term.getViewport();
		expect(vp.join('\n')).toContain('Heading');
		inst.unmount();
	});
});

// ============================================================
// 6. ansi.ts — uncovered tracker states
// ============================================================

describe('AnsiCodeTracker coverage', () => {
	it('tracks dim/blink/inverse/hidden/strikethrough', () => {
		for (const [code, name] of [[2,'dim'],[5,'blink'],[7,'inverse'],[8,'hidden'],[9,'strike']] as const) {
			const t = new AnsiCodeTracker();
			t.process(`\x1b[${code}m`);
			expect(t.getActiveCodes()).toContain(String(code));
		}
	});

	it('resets individual attributes', () => {
		const pairs = [[1,22],[3,23],[5,25],[7,27],[8,28],[9,29]] as const;
		for (const [on, off] of pairs) {
			const t = new AnsiCodeTracker();
			t.process(`\x1b[${on}m`);
			t.process(`\x1b[${off}m`);
			expect(t.hasActiveCodes()).toBe(false);
		}
	});

	it('code 21 resets bold', () => {
		const t = new AnsiCodeTracker();
		t.process('\x1b[1m');
		t.process('\x1b[21m');
		expect(t.getActiveCodes()).not.toContain('1');
	});

	it('resets fg/bg with 39/49', () => {
		const t = new AnsiCodeTracker();
		t.process('\x1b[31m');
		t.process('\x1b[39m');
		expect(t.hasActiveCodes()).toBe(false);
		t.process('\x1b[41m');
		t.process('\x1b[49m');
		expect(t.hasActiveCodes()).toBe(false);
	});

	it('tracks bright fg/bg colors', () => {
		const t = new AnsiCodeTracker();
		t.process('\x1b[91m');
		expect(t.getActiveCodes()).toContain('91');
		t.clear();
		t.process('\x1b[101m');
		expect(t.getActiveCodes()).toContain('101');
	});

	it('tracks bg 256-color and RGB', () => {
		const t = new AnsiCodeTracker();
		t.process('\x1b[48;5;196m');
		expect(t.getActiveCodes()).toContain('48;5;196');
		t.clear();
		t.process('\x1b[48;2;0;255;0m');
		expect(t.getActiveCodes()).toContain('48;2;0;255;0');
	});

	it('ignores non-SGR codes', () => {
		const t = new AnsiCodeTracker();
		t.process('\x1b[2K');
		expect(t.hasActiveCodes()).toBe(false);
	});

	it('extractAnsiCode handles APC/OSC with ST', () => {
		const apc = '\x1b_data\x1b\\';
		expect(extractAnsiCode(apc, 0)).toEqual({ code: apc, length: apc.length });
		const osc = '\x1b]8;;url\x1b\\';
		expect(extractAnsiCode(osc, 0)).toEqual({ code: osc, length: osc.length });
	});

	it('extractAnsiCode returns null for incomplete', () => {
		expect(extractAnsiCode('\x1b]8;;url', 0)).toBeNull();
		expect(extractAnsiCode('\x1b_data', 0)).toBeNull();
		expect(extractAnsiCode('\x1bX', 0)).toBeNull();
	});
});

// ============================================================
// 7. wrap-ansi.ts — ANSI mid-word, underline reset, ASCII fast path
// ============================================================

describe('wrapTextWithAnsi coverage', () => {
	it('wraps ANSI mid-word', () => {
		const result = wrapTextWithAnsi('\x1b[31mhelloworld\x1b[0m', 5);
		expect(result.length).toBe(2);
	});

	it('multi-line with ANSI', () => {
		const result = wrapTextWithAnsi('\x1b[31mline1\x1b[0m\n\x1b[32mline2\x1b[0m', 80);
		expect(result.length).toBe(2);
	});

	it('empty string', () => expect(wrapTextWithAnsi('', 80)).toEqual(['']));

	it('ASCII fast path wrapping', () => {
		const result = wrapTextWithAnsi('hello world this is a test', 10);
		expect(result.length).toBeGreaterThan(1);
	});

	it('ASCII long word breaking', () => {
		const result = wrapTextWithAnsi('abcdefghijklmnop', 5);
		expect(result[0]).toBe('abcde');
	});

	it('preserves underline reset at line end', () => {
		const result = wrapTextWithAnsi('\x1b[4m' + 'a'.repeat(20) + '\x1b[0m', 10);
		expect(result.length).toBe(2);
		expect(result[0]).toContain('\x1b[24m');
	});
});

// ============================================================
// 8. yoga.ts — layout props coverage
// ============================================================

describe('yoga.ts coverage', () => {
	it('percentage width/height', () => {
		const n = createYogaNode();
		applyYogaProps(n, { width: '50%', height: '100%' } as any);
		n.free();
	});

	it('flex properties', () => {
		const n = createYogaNode();
		applyYogaProps(n, { flexDirection: 'column-reverse', flexGrow: 1, flexShrink: 0, flexBasis: 100, flexWrap: 'wrap' } as any);
		n.free();
	});

	it('gap properties', () => {
		const n = createYogaNode();
		applyYogaProps(n, { gap: 4, columnGap: 2, rowGap: 3 } as any);
		n.free();
	});

	it('alignment properties', () => {
		const n = createYogaNode();
		applyYogaProps(n, { alignItems: 'center', alignSelf: 'flex-end', justifyContent: 'space-between' } as any);
		n.free();
	});

	it('padding/margin edges', () => {
		const n = createYogaNode();
		applyYogaProps(n, { paddingTop: 1, paddingBottom: 2, paddingLeft: 3, paddingRight: 4, marginTop: 1, marginBottom: 2, marginLeft: 3, marginRight: 4 } as any);
		n.free();
	});

	it('paddingX/Y and marginX/Y', () => {
		const n = createYogaNode();
		applyYogaProps(n, { paddingX: 2, paddingY: 1, marginX: 2, marginY: 1 } as any);
		n.free();
	});

	it('overflow hidden, display none, borderStyle', () => {
		const n = createYogaNode();
		applyYogaProps(n, { overflow: 'hidden' } as any);
		applyYogaProps(n, { display: 'none' } as any);
		applyYogaProps(n, { borderStyle: 'single' } as any);
		n.free();
	});

	it('minWidth/minHeight', () => {
		const n = createYogaNode();
		applyYogaProps(n, { minWidth: 10, minHeight: 5 } as any);
		n.free();
	});

	it('row-reverse, wrap-reverse', () => {
		const n = createYogaNode();
		applyYogaProps(n, { flexDirection: 'row-reverse', flexWrap: 'wrap-reverse' } as any);
		n.free();
	});

	it('justifyContent variants', () => {
		const n = createYogaNode();
		for (const jc of ['center', 'flex-end', 'space-around', 'space-evenly']) {
			applyYogaProps(n, { justifyContent: jc } as any);
		}
		n.free();
	});

	it('getBorderChars all styles', () => {
		expect(getBorderChars('round').topLeft).toBe('╭');
		expect(getBorderChars('double').topLeft).toBe('╔');
		expect(getBorderChars('bold').topLeft).toBe('┏');
		expect(getBorderChars('classic').topLeft).toBe('+');
		expect(getBorderChars('singleDouble').topLeft).toBe('╓');
		expect(getBorderChars('doubleSingle').topLeft).toBe('╒');
		expect(getBorderChars('unknown').topLeft).toBe('┌');
	});
});

// ============================================================
// 9. animation — createGrid, solidBg, radialGlow
// ============================================================

describe('animation coverage', () => {
	it('createGrid', () => {
		const g = createGrid(4, 4);
		expect(g.length).toBe(4);
		expect(g[0]![0]).toBe('.');
	});

	it('solidBg', () => {
		const bg = solidBg(2, 2, [255, 0, 0]);
		expect(bg[0]![0]).toEqual([255, 0, 0]);
	});

	it('radialGlow creates gradient', () => {
		const bg = radialGlow(10, 10, 5, 5, [0, 0, 0], [255, 255, 255], 5);
		expect(bg.length).toBe(10);
		const center = bg[5]![5]!;
		const corner = bg[0]![0]!;
		expect(center[0]).toBeGreaterThan(corner[0]);
	});

	it('renderGrid odd rows', () => {
		const grid = [['W', 'K'], ['K', 'W'], ['W', 'W']];
		expect(renderGrid(grid).split('\n').length).toBe(2);
	});

	it('stamp negative y', () => {
		const g = createGrid(4, 2);
		stamp(g, ['WW', 'KK', 'WW'], 0, -1);
		expect(g[0]![0]).toBe('K');
	});
});

// ============================================================
// 10. reconciler — insertBefore, removeChild, commitUpdate
// ============================================================

describe('reconciler tree-ops coverage', () => {
	it('reorder and remove triggers insertBefore/removeChild', async () => {
		const React = await import('react');
		const { render } = await import('../src/reconciler/render.js');
		const { Text } = await import('../src/components/Text.js');
		const { Box } = await import('../src/components/Box.js');
		const pkg = await import('@xterm/headless');
		const { Terminal: XtermTerminal } = pkg.default;

		const term = createMinimalTerm(XtermTerminal, 40, 10);

		function ListApp({ items }: { items: string[] }) {
			return React.createElement(Box, { flexDirection: 'column' },
				...items.map(item => React.createElement(Text, { key: item }, item)),
			);
		}

		const inst = render(
			React.createElement(ListApp, { items: ['A', 'B', 'C'] }),
			{ terminal: term as any, exitOnCtrlC: false },
		);
		await new Promise(r => setTimeout(r, 30));

		inst.rerender(React.createElement(ListApp, { items: ['C', 'A', 'B'] }));
		await new Promise(r => setTimeout(r, 30));

		inst.rerender(React.createElement(ListApp, { items: ['C', 'B'] }));
		await new Promise(r => setTimeout(r, 30));

		inst.clear();
		await new Promise(r => setTimeout(r, 30));

		const metrics = inst.getMetrics();
		expect(metrics.renderCount).toBeGreaterThan(0);
		expect(metrics.yogaNodeCount).toBeGreaterThan(0);

		inst.unmount();
	});
});
