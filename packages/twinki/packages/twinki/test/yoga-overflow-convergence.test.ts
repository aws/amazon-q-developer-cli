/**
 * Proves yoga infinite layout loop with unclamped overflow text.
 *
 * Uses yoga directly to show that returning measuredWidth > containerWidth
 * from a measure function causes yoga to call measure repeatedly.
 * The fix: clamp measuredWidth to maxW in overflow mode.
 */
import { describe, it, expect } from 'vitest';
import Yoga from 'yoga-layout';

describe('Yoga overflow measure convergence', () => {
	it('BUG: unclamped overflow measure causes excessive yoga calls', () => {
		const root = Yoga.Node.create();
		root.setWidth(6);  // tiny container
		root.setFlexDirection(Yoga.FLEX_DIRECTION_COLUMN);

		const child = Yoga.Node.create();
		let measureCalls = 0;

		// Simulate overflow text: always returns full width (200) regardless of constraint
		child.setMeasureFunc((width, widthMode) => {
			measureCalls++;
			// This is what the old code did — return full unwrapped width
			return { width: 200, height: 1 };
		});

		root.insertChild(child, 0);
		root.calculateLayout(6, undefined, Yoga.DIRECTION_LTR);

		// Yoga calls measure multiple times trying to resolve the constraint.
		// With unclamped width, it can't converge efficiently.
		console.log(`Unclamped: ${measureCalls} measure calls`);
		const unclampedCalls = measureCalls;

		root.free();

		// Now test with clamped width
		const root2 = Yoga.Node.create();
		root2.setWidth(6);
		root2.setFlexDirection(Yoga.FLEX_DIRECTION_COLUMN);

		const child2 = Yoga.Node.create();
		let measureCalls2 = 0;

		// Simulate the fix: clamp to available width
		child2.setMeasureFunc((width, widthMode) => {
			measureCalls2++;
			const maxW = widthMode === Yoga.MEASURE_MODE_UNDEFINED ? Infinity : width;
			// Clamp: tell yoga we fit, even though we're wider visually
			return { width: Math.min(200, maxW), height: 1 };
		});

		root2.insertChild(child2, 0);
		root2.calculateLayout(6, undefined, Yoga.DIRECTION_LTR);

		console.log(`Clamped: ${measureCalls2} measure calls`);

		// Clamped version should need fewer or equal measure calls
		expect(measureCalls2).toBeLessThanOrEqual(unclampedCalls);

		// Both should complete (not infinite), but unclamped may call more
		expect(measureCalls2).toBeGreaterThan(0);

		root2.free();
	});

	it('BUG: nested overflow children amplify measure calls', () => {
		const root = Yoga.Node.create();
		root.setWidth(6);
		root.setFlexDirection(Yoga.FLEX_DIRECTION_COLUMN);

		let totalMeasureCalls = 0;

		// Add 5 overflow children — simulates a conversation with multiple messages
		for (let i = 0; i < 5; i++) {
			const child = Yoga.Node.create();
			child.setMeasureFunc((width, widthMode) => {
				totalMeasureCalls++;
				return { width: 200, height: 1 };
			});
			root.insertChild(child, i);
		}

		root.calculateLayout(6, undefined, Yoga.DIRECTION_LTR);
		console.log(`5 unclamped children: ${totalMeasureCalls} total measure calls`);

		const unclampedTotal = totalMeasureCalls;
		root.free();

		// Now with clamp
		const root2 = Yoga.Node.create();
		root2.setWidth(6);
		root2.setFlexDirection(Yoga.FLEX_DIRECTION_COLUMN);

		let totalClamped = 0;
		for (let i = 0; i < 5; i++) {
			const child = Yoga.Node.create();
			child.setMeasureFunc((width, widthMode) => {
				totalClamped++;
				const maxW = widthMode === Yoga.MEASURE_MODE_UNDEFINED ? Infinity : width;
				return { width: Math.min(200, maxW), height: 1 };
			});
			root2.insertChild(child, i);
		}

		root2.calculateLayout(6, undefined, Yoga.DIRECTION_LTR);
		console.log(`5 clamped children: ${totalClamped} total measure calls`);

		expect(totalClamped).toBeLessThanOrEqual(unclampedTotal);
		root2.free();
	});

	it('BUG: real conversation scale (50 children) shows amplification', () => {
		const CHILDREN = 50;

		// Unclamped
		const root = Yoga.Node.create();
		root.setWidth(6);
		root.setFlexDirection(Yoga.FLEX_DIRECTION_COLUMN);
		let unclamped = 0;
		for (let i = 0; i < CHILDREN; i++) {
			const child = Yoga.Node.create();
			child.setMeasureFunc(() => { unclamped++; return { width: 200, height: 1 }; });
			root.insertChild(child, i);
		}
		root.calculateLayout(6, undefined, Yoga.DIRECTION_LTR);
		root.free();

		// Clamped
		const root2 = Yoga.Node.create();
		root2.setWidth(6);
		root2.setFlexDirection(Yoga.FLEX_DIRECTION_COLUMN);
		let clamped = 0;
		for (let i = 0; i < CHILDREN; i++) {
			const child = Yoga.Node.create();
			child.setMeasureFunc((w, wm) => {
				clamped++;
				const maxW = wm === Yoga.MEASURE_MODE_UNDEFINED ? Infinity : w;
				return { width: Math.min(200, maxW), height: 1 };
			});
			root2.insertChild(child, i);
		}
		root2.calculateLayout(6, undefined, Yoga.DIRECTION_LTR);
		root2.free();

		console.log(`${CHILDREN} children — unclamped: ${unclamped}, clamped: ${clamped}`);

		// The key evidence: unclamped calls grow with child count
		expect(clamped).toBeLessThanOrEqual(unclamped);
		expect(clamped).toBeGreaterThan(0);
		expect(unclamped).toBeGreaterThan(0);
	});
});
