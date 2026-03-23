import { describe, it, expect, vi, beforeEach, afterEach } from 'vitest';
import { StdinBuffer } from '../src/input/stdin-buffer.js';

describe('StdinBuffer', () => {
	let buffer: StdinBuffer;

	beforeEach(() => {
		buffer = new StdinBuffer({ timeout: 10 });
	});

	afterEach(() => {
		buffer.destroy();
	});

	it('emits single characters immediately', () => {
		const handler = vi.fn();
		buffer.on('data', handler);
		buffer.process('a');
		expect(handler).toHaveBeenCalledWith('a');
	});

	it('emits complete CSI sequences', () => {
		const handler = vi.fn();
		buffer.on('data', handler);
		buffer.process('\x1b[A'); // Up arrow
		expect(handler).toHaveBeenCalledWith('\x1b[A');
	});

	it('emits complete SS3 sequences', () => {
		const handler = vi.fn();
		buffer.on('data', handler);
		buffer.process('\x1bOA'); // Up arrow (SS3)
		expect(handler).toHaveBeenCalledWith('\x1bOA');
	});

	it('buffers incomplete CSI sequences', () => {
		const handler = vi.fn();
		buffer.on('data', handler);
		buffer.process('\x1b[');
		expect(handler).not.toHaveBeenCalled();
		expect(buffer.getBuffer()).toBe('\x1b[');
	});

	it('completes sequence when rest arrives', () => {
		const handler = vi.fn();
		buffer.on('data', handler);
		buffer.process('\x1b[');
		buffer.process('A');
		expect(handler).toHaveBeenCalledWith('\x1b[A');
	});

	it('emits plain text as a single chunk', () => {
		const handler = vi.fn();
		buffer.on('data', handler);
		buffer.process('abc');
		expect(handler).toHaveBeenCalledTimes(1);
		expect(handler).toHaveBeenCalledWith('abc');
	});

	it('handles bracketed paste', () => {
		const dataHandler = vi.fn();
		const pasteHandler = vi.fn();
		buffer.on('data', dataHandler);
		buffer.on('paste', pasteHandler);
		buffer.process('\x1b[200~pasted text\x1b[201~');
		expect(pasteHandler).toHaveBeenCalledWith('pasted text');
		expect(dataHandler).not.toHaveBeenCalled();
	});

	it('handles bracketed paste split across chunks', () => {
		const pasteHandler = vi.fn();
		buffer.on('paste', pasteHandler);
		buffer.process('\x1b[200~pasted');
		buffer.process(' text\x1b[201~');
		expect(pasteHandler).toHaveBeenCalledWith('pasted text');
	});

	it('flushes incomplete sequences on timeout', async () => {
		const handler = vi.fn();
		buffer.on('data', handler);
		buffer.process('\x1b');
		expect(handler).not.toHaveBeenCalled();
		await new Promise(resolve => setTimeout(resolve, 20));
		expect(handler).toHaveBeenCalledWith('\x1b');
	});

	it('flush() returns remaining buffer', () => {
		buffer.process('\x1b[');
		const flushed = buffer.flush();
		expect(flushed).toEqual(['\x1b[']);
		expect(buffer.getBuffer()).toBe('');
	});

	it('clear() empties buffer', () => {
		buffer.process('\x1b[');
		buffer.clear();
		expect(buffer.getBuffer()).toBe('');
	});

	it('handles Kitty protocol CSI u sequences', () => {
		const handler = vi.fn();
		buffer.on('data', handler);
		buffer.process('\x1b[97u'); // 'a' key
		expect(handler).toHaveBeenCalledWith('\x1b[97u');
	});

	it('handles Kitty protocol with modifiers', () => {
		const handler = vi.fn();
		buffer.on('data', handler);
		buffer.process('\x1b[97;5u'); // Ctrl+a
		expect(handler).toHaveBeenCalledWith('\x1b[97;5u');
	});
});
