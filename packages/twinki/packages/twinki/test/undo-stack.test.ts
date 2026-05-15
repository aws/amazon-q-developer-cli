import { describe, it, expect } from 'vitest';
import { UndoStack } from '../src/utils/undo-stack.js';

describe('UndoStack', () => {
	it('should push and pop states', () => {
		const stack = new UndoStack<{ value: string }>();
		stack.push({ value: 'a' });
		stack.push({ value: 'b' });
		
		expect(stack.length).toBe(2);
		expect(stack.pop()).toEqual({ value: 'b' });
		expect(stack.pop()).toEqual({ value: 'a' });
		expect(stack.pop()).toBeUndefined();
	});

	it('should deep clone on push', () => {
		const stack = new UndoStack<{ nested: { value: string } }>();
		const state = { nested: { value: 'original' } };
		
		stack.push(state);
		state.nested.value = 'mutated';
		
		const popped = stack.pop();
		expect(popped?.nested.value).toBe('original');
	});

	it('should clear all snapshots', () => {
		const stack = new UndoStack<number>();
		stack.push(1);
		stack.push(2);
		stack.push(3);
		
		stack.clear();
		expect(stack.length).toBe(0);
		expect(stack.pop()).toBeUndefined();
	});

	it('should handle empty stack', () => {
		const stack = new UndoStack<string>();
		expect(stack.length).toBe(0);
		expect(stack.pop()).toBeUndefined();
	});

	it('should cap at default 200 entries', () => {
		const stack = new UndoStack<number>();
		for (let i = 0; i < 250; i++) {
			stack.push(i);
		}
		expect(stack.length).toBe(200);
		// Most recent entry is preserved
		expect(stack.pop()).toBe(249);
	});

	it('should respect custom max size', () => {
		const stack = new UndoStack<number>(10);
		for (let i = 0; i < 15; i++) {
			stack.push(i);
		}
		expect(stack.length).toBe(10);
		// Oldest (0-4) evicted, newest (14) at top
		expect(stack.pop()).toBe(14);
		expect(stack.pop()).toBe(13);
	});
});
