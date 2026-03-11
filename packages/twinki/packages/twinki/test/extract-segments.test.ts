import { describe, it, expect } from 'vitest';
import { extractSegments } from '../src/utils/extract-segments.js';

describe('extractSegments', () => {
	it('extracts before segment', () => {
		const result = extractSegments('hello world', 5, 6, 5);
		expect(result.before).toBe('hello');
		expect(result.beforeWidth).toBe(5);
	});

	it('extracts after segment', () => {
		const result = extractSegments('hello world', 5, 6, 5);
		expect(result.after).toBe('world');
		expect(result.afterWidth).toBe(5);
	});

	it('handles ANSI codes in before region', () => {
		const line = '\x1b[31mhello\x1b[0m world';
		const result = extractSegments(line, 5, 6, 5);
		expect(result.beforeWidth).toBe(5);
	});

	it('inherits styling into after segment', () => {
		const line = '\x1b[1mhello world\x1b[0m';
		const result = extractSegments(line, 5, 6, 5);
		// After segment should inherit bold from before the overlay
		expect(result.after).toContain('\x1b[');
	});

	it('returns empty for zero afterLen', () => {
		const result = extractSegments('hello', 3, 3, 0);
		expect(result.after).toBe('');
		expect(result.afterWidth).toBe(0);
	});
});
