import { describe, it, expect } from 'vitest';
import { renderTree } from '../src/renderer/tree-renderer.js';
import { visibleWidth } from '../src/utils/visible-width.js';
import { wrapTextWithAnsi } from '../src/utils/wrap-ansi.js';
import { createNode, createTextNode } from '../src/reconciler/node-factory.js';
import { createYogaNode, Yoga } from '../src/layout/yoga.js';
import type { RootContainer } from '../src/reconciler/types.js';

function createTestDocument(lines: number): RootContainer {
	const yogaNode = createYogaNode();
	yogaNode.setFlexDirection(Yoga.FLEX_DIRECTION_COLUMN);
	
	const container: RootContainer = {
		yogaNode,
		children: [],
		onRender: () => {},
	};

	for (let i = 0; i < lines; i++) {
		const textNode = createNode('twinki-text', { wrap: 'wrap' });
		const content = createTextNode(`Line ${i + 1}: Lorem ipsum dolor sit amet, consectetur adipiscing elit. Sed do eiusmod tempor incididunt ut labore et dolore magna aliqua.`);
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
		// Mix of ANSI codes, Unicode, and long text - this is the expensive case
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

function createLongLineDocument(): RootContainer {
	const yogaNode = createYogaNode();
	yogaNode.setFlexDirection(Yoga.FLEX_DIRECTION_COLUMN);
	
	const container: RootContainer = {
		yogaNode,
		children: [],
		onRender: () => {},
	};

	const longLine = 'A'.repeat(1000) + ' ' + 'B'.repeat(1000) + ' ' + 'C'.repeat(1000);
	const textNode = createNode('twinki-text', { wrap: 'wrap' });
	const content = createTextNode(longLine);
	textNode.children.push(content);
	content.parent = textNode;
	container.children.push(textNode);
	textNode.parent = null;

	return container;
}

function createStaticDocument(staticItems: number): RootContainer {
	const yogaNode = createYogaNode();
	yogaNode.setFlexDirection(Yoga.FLEX_DIRECTION_COLUMN);
	
	const container: RootContainer = {
		yogaNode,
		children: [],
		onRender: () => {},
	};

	const staticNode = createNode('twinki-static', {});
	for (let i = 0; i < staticItems; i++) {
		const textNode = createNode('twinki-text', {});
		const content = createTextNode(`Static item ${i + 1}`);
		textNode.children.push(content);
		content.parent = textNode;
		staticNode.children.push(textNode);
		textNode.parent = staticNode;
	}
	container.children.push(staticNode);
	staticNode.parent = null;

	return container;
}

describe('Performance Benchmarks - TOP BOTTLENECKS IDENTIFIED', () => {
	// BOTTLENECK #1: Complex content with ANSI + Unicode (117ms for 1000 lines)
	it('BOTTLENECK #1: Complex ANSI+Unicode content rendering', () => {
		console.log('\n=== TOP BOTTLENECK #1: Complex ANSI+Unicode Content ===');
		
		const sizes = [100, 500, 1000];
		for (const size of sizes) {
			const doc = createComplexDocument(size);
			const width = 80;
			
			const start = performance.now();
			const result = renderTree(doc, width);
			const elapsed = performance.now() - start;
			
			console.log(`${size}-line complex content: ${elapsed.toFixed(2)}ms (${(elapsed/size).toFixed(3)}ms per line)`);
			expect(result.liveLines.length).toBeGreaterThan(0);
		}
	});

	// BOTTLENECK #2: ANSI text wrapping (53.70ms for 100 iterations of 2K chars)
	it('BOTTLENECK #2: ANSI text wrapping performance', () => {
		console.log('\n=== TOP BOTTLENECK #2: ANSI Text Wrapping ===');
		
		const testCases = [
			{ name: '1K ANSI', content: '\x1b[31m' + 'Red text content. '.repeat(60) + '\x1b[0m', iterations: 100 },
			{ name: '2K ANSI', content: '\x1b[31m' + 'Red text content. '.repeat(120) + '\x1b[0m', iterations: 100 },
			{ name: '5K ANSI', content: '\x1b[31m' + 'Red text content. '.repeat(300) + '\x1b[0m', iterations: 50 },
		];
		
		for (const testCase of testCases) {
			const start = performance.now();
			for (let i = 0; i < testCase.iterations; i++) {
				wrapTextWithAnsi(testCase.content, 80);
			}
			const elapsed = performance.now() - start;
			const perCall = elapsed / testCase.iterations;
			
			console.log(`${testCase.name}: ${elapsed.toFixed(2)}ms total, ${perCall.toFixed(3)}ms per call`);
		}
	});

	// BOTTLENECK #3: Static content array spread (16.65ms for 20K items)
	it('BOTTLENECK #3: Static content accumulation spread', () => {
		console.log('\n=== TOP BOTTLENECK #3: Static Content Array Spread ===');
		
		const staticSizes = [5000, 10000, 20000, 50000];
		
		for (const staticSize of staticSizes) {
			const accumulatedStatic = Array(staticSize).fill('Static line with some content that might be long');
			const liveContent = Array(100).fill('Live content line');
			
			const iterations = 100;
			const start = performance.now();
			
			for (let i = 0; i < iterations; i++) {
				// This is the expensive operation from TUI._doRenderInner
				const combined = [...accumulatedStatic, ...liveContent];
				combined.length; // Prevent optimization
			}
			
			const elapsed = performance.now() - start;
			const perOp = elapsed / iterations;
			
			console.log(`${staticSize} items spread: ${elapsed.toFixed(2)}ms total, ${perOp.toFixed(3)}ms per operation`);
		}
	});

	// Standard benchmarks for comparison
	it('renderNode for 100-line document', () => {
		const doc = createTestDocument(100);
		const width = 80;
		
		const start = performance.now();
		const result = renderTree(doc, width);
		const elapsed = performance.now() - start;
		
		console.log(`100-line renderNode: ${elapsed.toFixed(2)}ms`);
		expect(result.liveLines.length).toBeGreaterThan(0);
		expect(elapsed).toBeLessThan(100);
	});

	it('renderNode for 500-line document', () => {
		const doc = createTestDocument(500);
		const width = 80;
		
		const start = performance.now();
		const result = renderTree(doc, width);
		const elapsed = performance.now() - start;
		
		console.log(`500-line renderNode: ${elapsed.toFixed(2)}ms`);
		expect(result.liveLines.length).toBeGreaterThan(0);
		expect(elapsed).toBeLessThan(500);
	});

	it('renderNode for 1000-line document', () => {
		const doc = createTestDocument(1000);
		const width = 80;
		
		const start = performance.now();
		const result = renderTree(doc, width);
		const elapsed = performance.now() - start;
		
		console.log(`1000-line renderNode: ${elapsed.toFixed(2)}ms`);
		expect(result.liveLines.length).toBeGreaterThan(0);
		expect(elapsed).toBeLessThan(1000);
	});

	it('visibleWidth hot path (called per line per render)', () => {
		const testStrings = [
			'Hello world',
			'Line with ANSI \x1b[31mred\x1b[0m text',
			'Unicode: 🚀 emoji and こんにちは',
			'A'.repeat(100),
		];
		
		const iterations = 10000;
		const start = performance.now();
		
		for (let i = 0; i < iterations; i++) {
			for (const str of testStrings) {
				visibleWidth(str);
			}
		}
		
		const elapsed = performance.now() - start;
		const perCall = elapsed / (iterations * testStrings.length);
		
		console.log(`visibleWidth: ${elapsed.toFixed(2)}ms total, ${perCall.toFixed(4)}ms per call`);
		expect(perCall).toBeLessThan(0.01);
	});

	it('text wrapping performance with long lines', () => {
		const doc = createLongLineDocument();
		const width = 80;
		
		const start = performance.now();
		const result = renderTree(doc, width);
		const elapsed = performance.now() - start;
		
		console.log(`Long line wrapping: ${elapsed.toFixed(2)}ms`);
		expect(result.liveLines.length).toBeGreaterThan(10);
		expect(elapsed).toBeLessThan(50);
	});

	it('differential rendering: time to compute diff for large content', () => {
		const doc1 = createTestDocument(100);
		const doc2 = createTestDocument(100);
		
		// Modify one line in doc2
		const textNode = doc2.children[50] as any;
		textNode.children[0].textContent = 'MODIFIED: This line has been changed for diff testing';
		
		const width = 80;
		
		const start = performance.now();
		const result1 = renderTree(doc1, width);
		const result2 = renderTree(doc2, width);
		
		// Simulate diff computation
		let diffCount = 0;
		const maxLen = Math.max(result1.liveLines.length, result2.liveLines.length);
		for (let i = 0; i < maxLen; i++) {
			const line1 = result1.liveLines[i] ?? '';
			const line2 = result2.liveLines[i] ?? '';
			if (line1 !== line2) diffCount++;
		}
		
		const elapsed = performance.now() - start;
		
		console.log(`Differential rendering: ${elapsed.toFixed(2)}ms, ${diffCount} diffs found`);
		expect(diffCount).toBeGreaterThan(0);
		expect(elapsed).toBeLessThan(100);
	});

	it('static content prepend cost (accumulatedStaticOutput spread)', () => {
		const staticDoc = createStaticDocument(1000);
		const width = 80;
		
		// Simulate accumulated static output
		const accumulatedStatic = Array(500).fill('Previous static line');
		
		const start = performance.now();
		const result = renderTree(staticDoc, width);
		
		// Simulate the spread operation from TUI._doRenderInner
		const newLines = [...accumulatedStatic, ...result.liveLines];
		
		const elapsed = performance.now() - start;
		
		console.log(`Static content prepend: ${elapsed.toFixed(2)}ms, ${newLines.length} total lines`);
		expect(newLines.length).toBeGreaterThan(1000);
		expect(elapsed).toBeLessThan(50);
	});
});