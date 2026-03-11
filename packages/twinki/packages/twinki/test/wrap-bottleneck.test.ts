import { describe, it } from 'vitest';
import { wrapTextWithAnsi } from '../src/utils/wrap-ansi.js';
import { visibleWidth } from '../src/utils/visible-width.js';

describe('Text Wrapping Bottleneck Analysis', () => {
	it('wrapTextWithAnsi vs simple split performance comparison', () => {
		const testTexts = [
			'Short line',
			'Medium length line with some words that need wrapping at boundaries',
			'Very long line with lots of text that will definitely need to be wrapped across multiple lines in the terminal output and should stress test the wrapping algorithm performance characteristics when dealing with substantial amounts of content that exceeds the available width',
			'Line with \x1b[31mANSI\x1b[0m codes \x1b[1mbold\x1b[0m and \x1b[4munderline\x1b[0m formatting',
			'Unicode: 🚀 emoji and こんにちは Japanese text with wide characters',
		];
		
		const width = 80;
		const iterations = 200;
		
		// Test wrapTextWithAnsi
		const wrapStart = performance.now();
		for (let i = 0; i < iterations; i++) {
			for (const text of testTexts) {
				wrapTextWithAnsi(text, width);
			}
		}
		const wrapElapsed = performance.now() - wrapStart;
		
		// Test simple split (baseline)
		const splitStart = performance.now();
		for (let i = 0; i < iterations; i++) {
			for (const text of testTexts) {
				text.split('\n');
			}
		}
		const splitElapsed = performance.now() - splitStart;
		
		// Test visibleWidth calls
		const widthStart = performance.now();
		for (let i = 0; i < iterations; i++) {
			for (const text of testTexts) {
				visibleWidth(text);
			}
		}
		const widthElapsed = performance.now() - widthStart;
		
		const totalCalls = iterations * testTexts.length;
		console.log(`wrapTextWithAnsi: ${wrapElapsed.toFixed(2)}ms total, ${(wrapElapsed/totalCalls).toFixed(4)}ms per call`);
		console.log(`simple split: ${splitElapsed.toFixed(2)}ms total, ${(splitElapsed/totalCalls).toFixed(4)}ms per call`);
		console.log(`visibleWidth: ${widthElapsed.toFixed(2)}ms total, ${(widthElapsed/totalCalls).toFixed(4)}ms per call`);
		console.log(`wrapTextWithAnsi is ${(wrapElapsed/splitElapsed).toFixed(1)}x slower than simple split`);
		console.log(`wrapTextWithAnsi is ${(wrapElapsed/widthElapsed).toFixed(1)}x slower than visibleWidth`);
	});
	
	it('analyze wrap performance by text characteristics', () => {
		const tests = [
			{ name: 'Plain ASCII', text: 'A'.repeat(500) },
			{ name: 'With spaces', text: ('Word '.repeat(100)).trim() },
			{ name: 'With ANSI codes', text: '\x1b[31m' + 'Red '.repeat(100) + '\x1b[0m' },
			{ name: 'With Unicode', text: '🚀'.repeat(100) },
			{ name: 'Mixed content', text: 'Normal \x1b[31mred\x1b[0m 🚀 emoji こんにちは '.repeat(20) },
		];
		
		const width = 80;
		const iterations = 100;
		
		for (const test of tests) {
			const start = performance.now();
			for (let i = 0; i < iterations; i++) {
				wrapTextWithAnsi(test.text, width);
			}
			const elapsed = performance.now() - start;
			const perCall = elapsed / iterations;
			
			console.log(`${test.name}: ${elapsed.toFixed(2)}ms total, ${perCall.toFixed(4)}ms per call`);
		}
	});
});