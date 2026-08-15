import { describe, it, expect } from 'vitest';
import { renderTree } from '../src/renderer/tree-renderer.js';
import { visibleWidth } from '../src/utils/visible-width.js';
import { wrapTextWithAnsi } from '../src/utils/wrap-ansi.js';
import { createNode, createTextNode } from '../src/reconciler/node-factory.js';
import { createYogaNode, Yoga } from '../src/layout/yoga.js';
import type { RootContainer } from '../src/reconciler/types.js';
import { TUI } from '../src/renderer/tui.js';
import { ProcessTerminal } from '../src/terminal/process-terminal.js';

function createLargeDocument(lines: number, lineLength = 120): RootContainer {
	const yogaNode = createYogaNode();
	yogaNode.setFlexDirection(Yoga.FLEX_DIRECTION_COLUMN);
	
	const container: RootContainer = {
		yogaNode,
		children: [],
		onRender: () => {},
	};

	for (let i = 0; i < lines; i++) {
		const textNode = createNode('twinki-text', { wrap: 'wrap' });
		const content = createTextNode(`Line ${i + 1}: ${'Lorem ipsum dolor sit amet, consectetur adipiscing elit. '.repeat(Math.ceil(lineLength / 60))}`);
		textNode.children.push(content);
		content.parent = textNode;
		container.children.push(textNode);
		textNode.parent = null;
	}

	return container;
}

function createComplexDocument(lines: number): RootContainer {
	const yogaNode = createYogaNode();
	yogaNode.setFlexDirection(Yoga.FLEX_DIRECTION_COLUMN);
	
	const container: RootContainer = {
		yogaNode,
		children: [],
		onRender: () => {},
	};

	for (let i = 0; i < lines; i++) {
		const textNode = createNode('twinki-text', { wrap: 'wrap' });
		// Mix of ANSI codes, Unicode, and long text
		const ansiText = `\x1b[3${i % 8}m\x1b[4${(i + 1) % 8}mLine ${i + 1}\x1b[0m: `;
		const unicodeText = '🚀📝💻🎯⚡️🔥✨🌟'.repeat(2);
		const longText = 'Lorem ipsum dolor sit amet, consectetur adipiscing elit. '.repeat(3);
		const content = createTextNode(ansiText + unicodeText + ' ' + longText);
		textNode.children.push(content);
		content.parent = textNode;
		container.children.push(textNode);
		textNode.parent = null;
	}

	return container;
}

describe('Comprehensive Performance Analysis', () => {
	it('BOTTLENECK TEST: Large document rendering (2000 lines)', () => {
		const doc = createLargeDocument(2000);
		const width = 80;
		
		const start = performance.now();
		const result = renderTree(doc, width);
		const elapsed = performance.now() - start;
		
		console.log(`🔍 LARGE DOC (2000 lines): ${elapsed.toFixed(2)}ms`);
		expect(result.liveLines.length).toBeGreaterThan(0);
	});

	it('BOTTLENECK TEST: Complex content with ANSI + Unicode (1000 lines)', () => {
		const doc = createComplexDocument(1000);
		const width = 80;
		
		const start = performance.now();
		const result = renderTree(doc, width);
		const elapsed = performance.now() - start;
		
		console.log(`🔍 COMPLEX CONTENT (1000 lines): ${elapsed.toFixed(2)}ms`);
		expect(result.liveLines.length).toBeGreaterThan(0);
	});

	it('BOTTLENECK TEST: visibleWidth with various content types', () => {
		const testCases = [
			{ name: 'ASCII', content: 'Hello world simple text', iterations: 50000 },
			{ name: 'ANSI', content: '\x1b[31mRed\x1b[0m \x1b[32mGreen\x1b[0m \x1b[34mBlue\x1b[0m', iterations: 20000 },
			{ name: 'Unicode', content: '🚀📝💻 こんにちは 世界', iterations: 10000 },
			{ name: 'Long', content: 'A'.repeat(200), iterations: 10000 },
			{ name: 'Mixed', content: '\x1b[31m🚀 Hello\x1b[0m ' + 'A'.repeat(100), iterations: 5000 },
		];
		
		for (const testCase of testCases) {
			const start = performance.now();
			for (let i = 0; i < testCase.iterations; i++) {
				visibleWidth(testCase.content);
			}
			const elapsed = performance.now() - start;
			const perCall = elapsed / testCase.iterations;
			
			console.log(`🔍 visibleWidth ${testCase.name}: ${elapsed.toFixed(2)}ms total, ${(perCall * 1000).toFixed(3)}μs per call`);
		}
	});

	it('BOTTLENECK TEST: Text wrapping with very long lines', () => {
		const testCases = [
			{ name: '1K chars', content: 'A'.repeat(1000), width: 80 },
			{ name: '5K chars', content: 'Lorem ipsum dolor sit amet. '.repeat(200), width: 80 },
			{ name: '10K chars', content: 'Mixed content with spaces and punctuation! '.repeat(250), width: 80 },
			{ name: 'ANSI 2K', content: '\x1b[31m' + 'Red text content. '.repeat(120) + '\x1b[0m', width: 80 },
		];
		
		for (const testCase of testCases) {
			const iterations = 100;
			const start = performance.now();
			for (let i = 0; i < iterations; i++) {
				wrapTextWithAnsi(testCase.content, testCase.width);
			}
			const elapsed = performance.now() - start;
			const perCall = elapsed / iterations;
			
			console.log(`🔍 Wrap ${testCase.name}: ${elapsed.toFixed(2)}ms total, ${perCall.toFixed(3)}ms per call`);
		}
	});

	it('BOTTLENECK TEST: Differential rendering with large content changes', () => {
		const sizes = [500, 1000, 2000];
		
		for (const size of sizes) {
			const doc1 = createLargeDocument(size);
			const doc2 = createLargeDocument(size);
			
			// Modify multiple lines to simulate real changes
			for (let i = 0; i < size; i += 10) {
				const textNode = doc2.children[i] as any;
				if (textNode) {
					textNode.children[0].textContent = `MODIFIED Line ${i + 1}: This content has been changed for differential testing`;
				}
			}
			
			const width = 80;
			const start = performance.now();
			
			const result1 = renderTree(doc1, width);
			const result2 = renderTree(doc2, width);
			
			// Simulate line-by-line diff computation (from TUI._doRenderInner)
			let diffCount = 0;
			const maxLen = Math.max(result1.liveLines.length, result2.liveLines.length);
			for (let i = 0; i < maxLen; i++) {
				const line1 = result1.liveLines[i] ?? '';
				const line2 = result2.liveLines[i] ?? '';
				if (line1 !== line2) diffCount++;
			}
			
			const elapsed = performance.now() - start;
			
			console.log(`🔍 Diff ${size} lines: ${elapsed.toFixed(2)}ms, ${diffCount} changes, ${(elapsed/size).toFixed(4)}ms per line`);
		}
	});

	it('BOTTLENECK TEST: Static content accumulation and spread', () => {
		const staticSizes = [1000, 5000, 10000, 20000];
		
		for (const staticSize of staticSizes) {
			// Simulate accumulated static output from TUI
			const accumulatedStatic = Array(staticSize).fill('Static line with some content that might be long');
			const liveContent = Array(100).fill('Live content line');
			
			const iterations = 100;
			const start = performance.now();
			
			for (let i = 0; i < iterations; i++) {
				// This is the expensive operation from TUI._doRenderInner
				const combined = [...accumulatedStatic, ...liveContent];
				// Simulate some processing
				combined.length; // Access to prevent optimization
			}
			
			const elapsed = performance.now() - start;
			const perOp = elapsed / iterations;
			
			console.log(`🔍 Static spread ${staticSize} items: ${elapsed.toFixed(2)}ms total, ${perOp.toFixed(3)}ms per operation`);
		}
	});

	it('BOTTLENECK TEST: Yoga layout computation overhead', () => {
		const nodeCounts = [100, 500, 1000];
		
		for (const nodeCount of nodeCounts) {
			const start = performance.now();
			
			// Create a complex layout tree
			const rootYoga = createYogaNode();
			rootYoga.setFlexDirection(Yoga.FLEX_DIRECTION_COLUMN);
			rootYoga.setWidth(80);
			rootYoga.setHeight(24);
			
			const nodes = [];
			for (let i = 0; i < nodeCount; i++) {
				const node = createYogaNode();
				node.setFlexDirection(Yoga.FLEX_DIRECTION_ROW);
				node.setFlexWrap(Yoga.WRAP_WRAP);
				rootYoga.insertChild(node, i);
				nodes.push(node);
			}
			
			// Force layout calculation
			rootYoga.calculateLayout();
			
			// Clean up
			for (const node of nodes) {
				node.free();
			}
			rootYoga.free();
			
			const elapsed = performance.now() - start;
			
			console.log(`🔍 Yoga layout ${nodeCount} nodes: ${elapsed.toFixed(2)}ms, ${(elapsed/nodeCount).toFixed(4)}ms per node`);
		}
	});

	it('BOTTLENECK TEST: String operations in hot paths', () => {
		const testStrings = Array(1000).fill(0).map((_, i) => 
			`Line ${i}: Lorem ipsum dolor sit amet, consectetur adipiscing elit. Sed do eiusmod tempor incididunt ut labore et dolore magna aliqua.`
		);
		
		// Test string concatenation (used in rendering)
		const concatStart = performance.now();
		let result = '';
		for (const str of testStrings) {
			result += str + '\n';
		}
		const concatElapsed = performance.now() - concatStart;
		
		// Test array join (alternative approach)
		const joinStart = performance.now();
		const joined = testStrings.join('\n');
		const joinElapsed = performance.now() - joinStart;
		
		// Test slice operations (used in differential rendering)
		const sliceStart = performance.now();
		for (let i = 0; i < 10000; i++) {
			const str = testStrings[i % testStrings.length];
			str.slice(0, 50);
			str.slice(10, 60);
		}
		const sliceElapsed = performance.now() - sliceStart;
		
		console.log(`🔍 String concat: ${concatElapsed.toFixed(2)}ms`);
		console.log(`🔍 String join: ${joinElapsed.toFixed(2)}ms`);
		console.log(`🔍 String slice (10K ops): ${sliceElapsed.toFixed(2)}ms`);
	});
});