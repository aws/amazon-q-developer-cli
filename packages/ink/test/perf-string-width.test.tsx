/**
 * Regression tests for stringWidth/widestLine caching.
 *
 * Run with: cd packages/ink && bun test test/perf-string-width.test.tsx
 */
import {describe, test, expect} from 'bun:test';
import EventEmitter from 'node:events';
import React from 'react';
import {render, Box, Text} from '../src/index.js';
import {cachedStringWidth, cachedWidestLine} from '../src/cached-string-width.js';

function createStdout(columns = 100) {
	const stdout = new EventEmitter() as any;
	stdout.columns = columns;
	let last = '';
	stdout.write = (data: string) => { last = data; return true; };
	stdout.get = () => last;
	return stdout;
}

function renderToStr(node: React.JSX.Element, columns = 100): string {
	const stdout = createStdout(columns);
	render(node, {stdout, debug: true});
	return stdout.get();
}

describe('cachedStringWidth', () => {
	test('ASCII', () => {
		expect(cachedStringWidth('hello')).toBe(5);
		expect(cachedStringWidth('hello')).toBe(5); // cache hit
	});

	test('emoji', () => {
		expect(cachedStringWidth('👋')).toBe(2);
		expect(cachedStringWidth('hello 👋')).toBe(8);
	});

	test('empty string', () => {
		expect(cachedStringWidth('')).toBe(0);
	});
});

describe('cachedWidestLine', () => {
	test('multi-line', () => {
		expect(cachedWidestLine('short\na longer line\nhi')).toBe(13);
	});

	test('single line', () => {
		expect(cachedWidestLine('hello')).toBe(5);
	});

	test('empty', () => {
		expect(cachedWidestLine('')).toBe(0);
	});
});

describe('render performance regression', () => {
	test('large ASCII text renders under 2s', () => {
		const largeText = Array.from({length: 1000}, (_, i) =>
			`Line ${String(i).padStart(4, '0')}: ${'x'.repeat(70)}`,
		).join('\n');

		const start = performance.now();
		const output = renderToStr(
			<Box width={80}>
				<Text>{largeText}</Text>
			</Box>,
		);
		const elapsed = performance.now() - start;

		expect(output).toContain('Line 0000');
		expect(output).toContain('Line 0999');
		expect(elapsed).toBeLessThan(2000);
	});

	test('emoji text renders under 2s', () => {
		const emojiText = Array.from({length: 200}, (_, i) =>
			`👋 Line ${i}: Hello world 🌍`,
		).join('\n');

		const start = performance.now();
		const output = renderToStr(
			<Box width={80}>
				<Text>{emojiText}</Text>
			</Box>,
		);
		const elapsed = performance.now() - start;

		expect(output).toContain('👋 Line 0');
		expect(output).toContain('👋 Line 199');
		expect(elapsed).toBeLessThan(2000);
	});

	test('repeated renders benefit from cache', () => {
		const text = 'The quick brown fox jumps over the lazy dog 🦊';

		const start = performance.now();
		for (let i = 0; i < 100; i++) {
			renderToStr(
				<Box width={80}>
					<Text>{text}</Text>
				</Box>,
			);
		}
		const elapsed = performance.now() - start;

		expect(elapsed).toBeLessThan(2000);
	});
});
