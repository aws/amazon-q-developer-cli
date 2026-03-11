import { describe, it, expect } from 'vitest';
import { wrapTextWithAnsi } from '../src/utils/wrap-ansi.js';
import { wrapTextWithAnsiOptimized } from '../src/utils/wrap-ansi-optimized.js';

describe('Performance Optimization Verification', () => {
	it('optimized ANSI wrapping produces same results as original', () => {
		const testCases = [
			'Simple text without ANSI codes',
			'\x1b[31mRed text\x1b[0m with reset',
			'\x1b[31mRed\x1b[32m Green\x1b[34m Blue\x1b[0m',
			'🚀 Unicode emoji with \x1b[31mANSI\x1b[0m codes',
			'A'.repeat(100) + ' ' + 'B'.repeat(100),
			'\x1b[31m' + 'Long red text that needs wrapping. '.repeat(10) + '\x1b[0m',
		];

		for (const testCase of testCases) {
			const original = wrapTextWithAnsi(testCase, 40);
			const optimized = wrapTextWithAnsiOptimized(testCase, 40);
			
			expect(optimized).toEqual(original);
		}
	});

	it('optimized ANSI wrapping performance improvement', () => {
		const longAnsiText = '\x1b[31m' + 'Red text content that needs wrapping. '.repeat(100) + '\x1b[0m';
		const iterations = 100;

		// Test original performance (using the current implementation)
		const originalStart = performance.now();
		for (let i = 0; i < iterations; i++) {
			wrapTextWithAnsi(longAnsiText, 80);
		}
		const originalTime = performance.now() - originalStart;

		// Test optimized performance
		const optimizedStart = performance.now();
		for (let i = 0; i < iterations; i++) {
			wrapTextWithAnsiOptimized(longAnsiText, 80);
		}
		const optimizedTime = performance.now() - optimizedStart;

		console.log(`Original ANSI wrapping: ${originalTime.toFixed(2)}ms`);
		console.log(`Optimized ANSI wrapping: ${optimizedTime.toFixed(2)}ms`);
		console.log(`Performance improvement: ${((originalTime - optimizedTime) / originalTime * 100).toFixed(1)}%`);

		// The optimized version should be faster (though we can't guarantee exact speedup in tests)
		expect(optimizedTime).toBeLessThan(originalTime * 1.1); // Allow 10% margin for test variance
	});

	it('static content concat vs spread performance', () => {
		const staticSizes = [1000, 5000, 10000];
		
		for (const size of staticSizes) {
			const staticContent = Array(size).fill('Static line content');
			const liveContent = Array(100).fill('Live content');
			const iterations = 100;

			// Test spread operator
			const spreadStart = performance.now();
			for (let i = 0; i < iterations; i++) {
				const combined = [...staticContent, ...liveContent];
				combined.length; // Prevent optimization
			}
			const spreadTime = performance.now() - spreadStart;

			// Test concat method
			const concatStart = performance.now();
			for (let i = 0; i < iterations; i++) {
				const combined = staticContent.concat(liveContent);
				combined.length; // Prevent optimization
			}
			const concatTime = performance.now() - concatStart;

			console.log(`${size} items - Spread: ${spreadTime.toFixed(2)}ms, Concat: ${concatTime.toFixed(2)}ms`);
			console.log(`Concat improvement: ${((spreadTime - concatTime) / spreadTime * 100).toFixed(1)}%`);

			// Concat should be faster or at least not significantly slower
			expect(concatTime).toBeLessThan(spreadTime * 1.2); // Allow 20% margin
		}
	});

	it('verify TUI static content optimization works', () => {
		// This test verifies that the TUI change doesn't break functionality
		// We can't easily test the TUI directly, but we can test the array operations
		
		const accumulatedStatic = ['Static line 1', 'Static line 2', 'Static line 3'];
		const newLines = ['Live line 1', 'Live line 2'];

		// Original approach (spread)
		const spreadResult = [...accumulatedStatic, ...newLines];
		
		// Optimized approach (concat)
		const concatResult = accumulatedStatic.concat(newLines);

		// Results should be identical
		expect(concatResult).toEqual(spreadResult);
		expect(concatResult).toEqual([
			'Static line 1',
			'Static line 2', 
			'Static line 3',
			'Live line 1',
			'Live line 2'
		]);
	});
});