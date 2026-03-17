import { describe, it, expect, beforeAll } from 'vitest';
import { resetCapabilitiesCache } from '../src/terminal/capabilities.js';
import { colorToAnsi, colorize, _resetChalk } from '../src/utils/color-parser.js';

// Force truecolor detection so tests get deterministic ANSI codes
beforeAll(() => {
	process.env.COLORTERM = 'truecolor';
	resetCapabilitiesCache();
	_resetChalk();
});

describe('colorToAnsi', () => {
	describe('named colors', () => {
		it('foreground', () => {
			expect(colorToAnsi('red', false)).toBe('31');
			expect(colorToAnsi('green', false)).toBe('32');
			expect(colorToAnsi('blue', false)).toBe('34');
			expect(colorToAnsi('white', false)).toBe('37');
			expect(colorToAnsi('cyan', false)).toBe('36');
		});

		it('background', () => {
			expect(colorToAnsi('red', true)).toBe('41');
			expect(colorToAnsi('green', true)).toBe('42');
			expect(colorToAnsi('blue', true)).toBe('44');
		});

		it('bright variants', () => {
			expect(colorToAnsi('redBright', false)).toBe('91');
			expect(colorToAnsi('greenBright', false)).toBe('92');
			expect(colorToAnsi('yellowBright', false)).toBe('93');
		});

		it('gray/grey aliases', () => {
			expect(colorToAnsi('gray', false)).toBe('90');
			expect(colorToAnsi('grey', false)).toBe('90');
		});
	});

	describe('hex colors', () => {
		it('standard #rrggbb', () => {
			expect(colorToAnsi('#ff0000', false)).toBe('38;2;255;0;0');
			expect(colorToAnsi('#00ff00', true)).toBe('48;2;0;255;0');
			expect(colorToAnsi('#0000ff', false)).toBe('38;2;0;0;255');
		});

		it('double hash ##rrggbb (theme edge case)', () => {
			expect(colorToAnsi('##262626', true)).toBe('48;2;38;38;38');
			expect(colorToAnsi('##ff0000', false)).toBe('38;2;255;0;0');
		});

		it('shorthand #rgb', () => {
			expect(colorToAnsi('#f00', false)).toBe('38;2;255;0;0');
			expect(colorToAnsi('#0f0', true)).toBe('48;2;0;255;0');
		});
	});

	describe('rgb()', () => {
		it('standard format', () => {
			expect(colorToAnsi('rgb(255, 0, 0)', false)).toBe('38;2;255;0;0');
			expect(colorToAnsi('rgb(0,255,0)', true)).toBe('48;2;0;255;0');
		});

		it('with optional spaces', () => {
			expect(colorToAnsi('rgb(128,64,32)', false)).toMatch(/38;2;128;64;32/);
			expect(colorToAnsi('rgb( 128, 64, 32)', false)).toMatch(/38;2;128;64;32/);
		});
	});

	describe('ansi256()', () => {
		it('standard format', () => {
			expect(colorToAnsi('ansi256(196)', false)).toBe('38;5;196');
			expect(colorToAnsi('ansi256(21)', true)).toBe('48;5;21');
		});
	});

	describe('invalid colors', () => {
		it('returns empty string', () => {
			expect(colorToAnsi('', false)).toBe('');
			expect(colorToAnsi('notacolor', false)).toBe('');
			expect(colorToAnsi('rgb()', false)).toBe('');
			expect(colorToAnsi('ansi256()', false)).toBe('');
		});
	});
});

describe('colorize', () => {
	it('wraps text with foreground color', () => {
		const result = colorize('hello', 'red', 'foreground');
		expect(result).toContain('\x1b[31m');
		expect(result).toContain('hello');
		expect(result).toContain('\x1b[39m');
	});

	it('wraps text with background color', () => {
		const result = colorize('hello', 'blue', 'background');
		expect(result).toContain('\x1b[44m');
		expect(result).toContain('hello');
	});

	it('handles hex colors', () => {
		const result = colorize('test', '#ff0000', 'foreground');
		expect(result).toContain('38;2;255;0;0');
	});

	it('handles double-hash hex', () => {
		const result = colorize('test', '##262626', 'background');
		expect(result).toContain('48;2;38;38;38');
	});

	it('returns unchanged string for undefined color', () => {
		expect(colorize('hello', undefined, 'foreground')).toBe('hello');
	});

	it('returns unchanged string for invalid color', () => {
		expect(colorize('hello', 'notacolor', 'foreground')).toBe('hello');
	});
});
