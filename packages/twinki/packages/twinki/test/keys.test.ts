import { describe, it, expect, beforeEach, afterEach } from 'vitest';
import { matchesKey, parseKey, setKittyProtocolActive } from '../src/input/keys.js';
import { parseInputData } from '../src/hooks/useInput.js';

describe('matchesKey', () => {
	afterEach(() => {
		setKittyProtocolActive(false);
	});

	describe('legacy mode', () => {
		it('matches escape', () => {
			expect(matchesKey('\x1b', 'escape')).toBe(true);
		});

		it('matches enter', () => {
			expect(matchesKey('\r', 'enter')).toBe(true);
		});

		it('matches tab', () => {
			expect(matchesKey('\t', 'tab')).toBe(true);
		});

		it('matches shift+tab', () => {
			expect(matchesKey('\x1b[Z', 'shift+tab')).toBe(true);
		});

		it('matches space', () => {
			expect(matchesKey(' ', 'space')).toBe(true);
		});

		it('matches backspace', () => {
			expect(matchesKey('\x7f', 'backspace')).toBe(true);
			expect(matchesKey('\x08', 'backspace')).toBe(true);
		});

		it('matches arrow keys', () => {
			expect(matchesKey('\x1b[A', 'up')).toBe(true);
			expect(matchesKey('\x1b[B', 'down')).toBe(true);
			expect(matchesKey('\x1b[C', 'right')).toBe(true);
			expect(matchesKey('\x1b[D', 'left')).toBe(true);
		});

		it('matches SS3 arrow keys', () => {
			expect(matchesKey('\x1bOA', 'up')).toBe(true);
			expect(matchesKey('\x1bOB', 'down')).toBe(true);
		});

		it('matches ctrl+c', () => {
			expect(matchesKey('\x03', 'ctrl+c')).toBe(true);
		});

		it('matches ctrl+a', () => {
			expect(matchesKey('\x01', 'ctrl+a')).toBe(true);
		});

		it('matches ctrl+z', () => {
			expect(matchesKey('\x1a', 'ctrl+z')).toBe(true);
		});

		it('matches single letters', () => {
			expect(matchesKey('a', 'a')).toBe(true);
			expect(matchesKey('z', 'z')).toBe(true);
		});

		it('matches shift+letter as uppercase', () => {
			expect(matchesKey('A', 'shift+a')).toBe(true);
		});

		it('matches alt+backspace', () => {
			expect(matchesKey('\x1b\x7f', 'alt+backspace')).toBe(true);
		});

		it('matches function keys', () => {
			expect(matchesKey('\x1bOP', 'f1')).toBe(true);
			expect(matchesKey('\x1bOQ', 'f2')).toBe(true);
		});

		it('matches delete', () => {
			expect(matchesKey('\x1b[3~', 'delete')).toBe(true);
		});

		it('matches home/end', () => {
			expect(matchesKey('\x1b[H', 'home')).toBe(true);
			expect(matchesKey('\x1b[F', 'end')).toBe(true);
		});

		it('matches pageUp/pageDown', () => {
			expect(matchesKey('\x1b[5~', 'pageUp')).toBe(true);
			expect(matchesKey('\x1b[6~', 'pageDown')).toBe(true);
		});

		it('does not match wrong key', () => {
			expect(matchesKey('a', 'b')).toBe(false);
			expect(matchesKey('\x1b[A', 'down')).toBe(false);
		});
	});

	describe('Kitty protocol', () => {
		beforeEach(() => {
			setKittyProtocolActive(true);
		});

		it('matches CSI u format', () => {
			expect(matchesKey('\x1b[97u', 'a')).toBe(true);
		});

		it('matches CSI u with ctrl modifier', () => {
			expect(matchesKey('\x1b[97;5u', 'ctrl+a')).toBe(true);
		});

		it('matches CSI u with shift modifier', () => {
			expect(matchesKey('\x1b[97;2u', 'shift+a')).toBe(true);
		});

		it('matches enter in Kitty', () => {
			expect(matchesKey('\x1b[13u', 'enter')).toBe(true);
		});

		it('matches escape in Kitty', () => {
			expect(matchesKey('\x1b[27u', 'escape')).toBe(true);
		});

		it('matches space in Kitty', () => {
			expect(matchesKey('\x1b[32u', 'space')).toBe(true);
		});
	});
});

describe('parseKey', () => {
	afterEach(() => {
		setKittyProtocolActive(false);
	});

	it('parses escape', () => {
		expect(parseKey('\x1b')).toBe('escape');
	});

	it('parses enter', () => {
		expect(parseKey('\r')).toBe('enter');
	});

	it('parses tab', () => {
		expect(parseKey('\t')).toBe('tab');
	});

	it('parses space', () => {
		expect(parseKey(' ')).toBe('space');
	});

	it('parses backspace', () => {
		expect(parseKey('\x7f')).toBe('backspace');
	});

	it('parses arrow keys', () => {
		expect(parseKey('\x1b[A')).toBe('up');
		expect(parseKey('\x1b[B')).toBe('down');
		expect(parseKey('\x1b[C')).toBe('right');
		expect(parseKey('\x1b[D')).toBe('left');
	});

	it('parses ctrl+letter', () => {
		expect(parseKey('\x03')).toBe('ctrl+c');
		expect(parseKey('\x01')).toBe('ctrl+a');
	});

	it('parses single characters', () => {
		expect(parseKey('a')).toBe('a');
		expect(parseKey('z')).toBe('z');
	});

	it('parses alt+key', () => {
		expect(parseKey('\x1ba')).toBe('alt+a');
	});

	it('parses shift+tab', () => {
		expect(parseKey('\x1b[Z')).toBe('shift+tab');
	});

	it('returns undefined for unknown sequences', () => {
		expect(parseKey('\x1b[999Z')).toBeUndefined();
	});

	describe('Kitty protocol', () => {
		beforeEach(() => {
			setKittyProtocolActive(true);
		});

		it('parses CSI u sequences', () => {
			expect(parseKey('\x1b[97u')).toBe('a');
		});

		it('parses CSI u with modifiers', () => {
			expect(parseKey('\x1b[97;5u')).toBe('ctrl+a');
		});

		it('parses enter in Kitty', () => {
			expect(parseKey('\x1b[13u')).toBe('enter');
		});
	});
});

describe('parseKey - non-Latin characters (Kitty)', () => {
	beforeEach(() => setKittyProtocolActive(true));
	afterEach(() => setKittyProtocolActive(false));

	it('parses CJK character (你 = U+4F60)', () => {
		// CSI 20320 u  (0x4F60 = 20320)
		expect(parseKey('\x1b[20320u')).toBe('你');
	});

	it('parses Cyrillic character (Д = U+0414)', () => {
		expect(parseKey('\x1b[1044u')).toBe('Д');
	});

	it('does not return printable for control codepoints', () => {
		// BEL (0x07) should not be treated as printable
		expect(parseKey('\x1b[7u')).toBeUndefined();
	});
});

describe('parseInputData - non-Latin characters (Kitty)', () => {
	beforeEach(() => setKittyProtocolActive(true));
	afterEach(() => setKittyProtocolActive(false));

	it('extracts CJK character as input', () => {
		const { input, key } = parseInputData('\x1b[20320u');
		expect(input).toBe('你');
		expect(key.return).toBe(false);
	});

	it('does not extract input for special keys', () => {
		const { input, key } = parseInputData('\x1b[13u');
		expect(input).toBe('');
		expect(key.return).toBe(true);
	});

	it('extracts Latin letter with modifier', () => {
		const { input, key } = parseInputData('\x1b[97;5u');
		expect(input).toBe('a');
		expect(key.ctrl).toBe(true);
	});
});
