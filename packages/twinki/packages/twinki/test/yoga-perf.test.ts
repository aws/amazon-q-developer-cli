import { describe, it } from 'vitest';
import { createYogaNode, Yoga } from '../src/layout/yoga.js';

describe('Yoga Layout Performance', () => {
	it('layout calculation performance for large trees', () => {
		const sizes = [100, 500, 1000];
		
		for (const nodeCount of sizes) {
			const root = createYogaNode();
			root.setFlexDirection(Yoga.FLEX_DIRECTION_COLUMN);
			root.setWidth(80);
			
			// Create a tree of nodes
			for (let i = 0; i < nodeCount; i++) {
				const child = createYogaNode();
				child.setHeight(1);
				root.insertChild(child, i);
			}
			
			const start = performance.now();
			root.calculateLayout(80, undefined, Yoga.DIRECTION_LTR);
			const elapsed = performance.now() - start;
			
			console.log(`Yoga layout ${nodeCount} nodes: ${elapsed.toFixed(4)}ms`);
			
			// Cleanup
			for (let i = nodeCount - 1; i >= 0; i--) {
				const child = root.getChild(i);
				root.removeChild(child);
				child.free();
			}
			root.free();
		}
	});
	
	it('repeated layout calculations (cache effectiveness)', () => {
		const root = createYogaNode();
		root.setFlexDirection(Yoga.FLEX_DIRECTION_COLUMN);
		root.setWidth(80);
		
		// Create moderate tree
		const nodeCount = 100;
		for (let i = 0; i < nodeCount; i++) {
			const child = createYogaNode();
			child.setHeight(1);
			root.insertChild(child, i);
		}
		
		const iterations = 100;
		const start = performance.now();
		
		for (let i = 0; i < iterations; i++) {
			root.calculateLayout(80, undefined, Yoga.DIRECTION_LTR);
		}
		
		const elapsed = performance.now() - start;
		const perCall = elapsed / iterations;
		
		console.log(`Repeated layout calculations: ${elapsed.toFixed(2)}ms total, ${perCall.toFixed(4)}ms per call`);
		
		// Cleanup
		for (let i = nodeCount - 1; i >= 0; i--) {
			const child = root.getChild(i);
			root.removeChild(child);
			child.free();
		}
		root.free();
	});
});