import { describe, it, expect } from 'vitest';
import { KillRing } from '../src/utils/kill-ring.js';

describe('KillRing', () => {
	it('should push and peek entries', () => {
		const ring = new KillRing();
		ring.push('first', { prepend: false });
		ring.push('second', { prepend: false });
		
		expect(ring.length).toBe(2);
		expect(ring.peek()).toBe('second');
	});

	it('should accumulate forward deletions', () => {
		const ring = new KillRing();
		ring.push('hello', { prepend: false });
		ring.push(' world', { prepend: false, accumulate: true });
		
		expect(ring.length).toBe(1);
		expect(ring.peek()).toBe('hello world');
	});

	it('should accumulate backward deletions', () => {
		const ring = new KillRing();
		ring.push('world', { prepend: false });
		ring.push('hello ', { prepend: true, accumulate: true });
		
		expect(ring.length).toBe(1);
		expect(ring.peek()).toBe('hello world');
	});

	it('should rotate entries for yank-pop', () => {
		const ring = new KillRing();
		ring.push('first', { prepend: false });
		ring.push('second', { prepend: false });
		ring.push('third', { prepend: false });
		
		expect(ring.peek()).toBe('third');
		ring.rotate();
		expect(ring.peek()).toBe('second');
		ring.rotate();
		expect(ring.peek()).toBe('first');
	});

	it('should ignore empty text', () => {
		const ring = new KillRing();
		ring.push('', { prepend: false });
		
		expect(ring.length).toBe(0);
	});

	it('should handle rotate on empty ring', () => {
		const ring = new KillRing();
		ring.rotate();
		expect(ring.length).toBe(0);
	});

	it('should handle rotate on single entry', () => {
		const ring = new KillRing();
		ring.push('only', { prepend: false });
		ring.rotate();
		
		expect(ring.length).toBe(1);
		expect(ring.peek()).toBe('only');
	});
});
