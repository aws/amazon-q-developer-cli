import { describe, it, expect } from 'vitest';
import { wordWrapLine } from '../src/utils/word-wrap.js';

describe('wordWrapLine', () => {
	it('should return single chunk for short lines', () => {
		const chunks = wordWrapLine('hello', 20);
		expect(chunks).toEqual([{ text: 'hello', startIndex: 0, endIndex: 5 }]);
	});

	it('should wrap at word boundaries', () => {
		const chunks = wordWrapLine('hello world foo', 11);
		expect(chunks.length).toBe(2);
		expect(chunks[0]!.text).toBe('hello ');
		expect(chunks[1]!.text).toBe('world foo');
	});

	it('should force-break long words', () => {
		const chunks = wordWrapLine('abcdefghij', 5);
		expect(chunks.length).toBe(2);
		expect(chunks[0]!.text).toBe('abcde');
		expect(chunks[1]!.text).toBe('fghij');
	});

	it('should handle empty line', () => {
		const chunks = wordWrapLine('', 10);
		expect(chunks).toEqual([{ text: '', startIndex: 0, endIndex: 0 }]);
	});

	it('should track start/end indices', () => {
		const chunks = wordWrapLine('aaa bbb ccc', 7);
		expect(chunks[0]!.startIndex).toBe(0);
		expect(chunks[0]!.endIndex).toBe(4); // wrap at "bbb"
		expect(chunks[1]!.startIndex).toBe(4);
		expect(chunks[1]!.endIndex).toBe(11);
	});

	it('should handle multiple spaces', () => {
		const chunks = wordWrapLine('a  b', 10);
		expect(chunks.length).toBe(1);
		expect(chunks[0]!.text).toBe('a  b');
	});

	it('should handle zero width', () => {
		const chunks = wordWrapLine('hello', 0);
		expect(chunks).toEqual([{ text: '', startIndex: 0, endIndex: 0 }]);
	});
});
