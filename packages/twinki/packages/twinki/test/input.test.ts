import { describe, it, expect, beforeEach } from 'vitest';
import { Input } from '../src/components/Input.js';

describe('Input', () => {
	let input: Input;

	beforeEach(() => {
		input = new Input();
		input.focused = true;
	});

	it('should render empty input with cursor', () => {
		const lines = input.render(40);
		expect(lines.length).toBe(1);
		expect(lines[0]).toContain('> ');
		expect(lines[0]).toContain('\x1b[7m'); // inverse video cursor
	});

	it('should accept character input', () => {
		input.handleInput('h');
		input.handleInput('i');
		expect(input.getValue()).toBe('hi');
	});

	it('should handle backspace', () => {
		input.handleInput('abc');
		input.handleInput('\x7f'); // backspace
		expect(input.getValue()).toBe('ab');
	});

	it('should handle forward delete', () => {
		input.handleInput('abc');
		input.handleInput('\x01'); // Ctrl+A (home)
		input.handleInput('\x1b[3~'); // delete
		expect(input.getValue()).toBe('bc');
	});

	it('should handle cursor movement', () => {
		input.handleInput('abc');
		input.handleInput('\x1b[D'); // left
		input.handleInput('X');
		expect(input.getValue()).toBe('abXc');
	});

	it('should handle home/end', () => {
		input.handleInput('abc');
		input.handleInput('\x01'); // Ctrl+A (home)
		input.handleInput('X');
		expect(input.getValue()).toBe('Xabc');
	});

	it('should handle undo', () => {
		input.handleInput('hello');
		input.handleInput(' '); // space triggers new undo unit
		input.handleInput('world');
		// Undo 'world' - space was part of the undo snapshot before 'world'
		input.handleInput('\x1b[45;5u'); // Ctrl+- (undo) via CSI-u
		expect(input.getValue()).toBe('hello');
	});

	it('should handle kill to end of line (Ctrl+K)', () => {
		input.handleInput('hello world');
		input.handleInput('\x01'); // home
		input.handleInput('\x1b[C'); // right 5 times
		input.handleInput('\x1b[C');
		input.handleInput('\x1b[C');
		input.handleInput('\x1b[C');
		input.handleInput('\x1b[C');
		input.handleInput('\x0b'); // Ctrl+K
		expect(input.getValue()).toBe('hello');
	});

	it('should handle yank (Ctrl+Y)', () => {
		input.handleInput('hello world');
		input.handleInput('\x0b'); // Ctrl+K (kill to end - but cursor is at end, nothing to kill)
		// Let's do it properly: go home, kill to end, then yank
		input.handleInput('\x01'); // home
		input.handleInput('\x0b'); // Ctrl+K
		expect(input.getValue()).toBe('');
		input.handleInput('\x19'); // Ctrl+Y (yank)
		expect(input.getValue()).toBe('hello world');
	});

	it('should handle submit', () => {
		let submitted = '';
		input.onSubmit = (value) => { submitted = value; };
		input.handleInput('test');
		input.handleInput('\r'); // enter
		expect(submitted).toBe('test');
	});

	it('should handle escape', () => {
		let escaped = false;
		input.onEscape = () => { escaped = true; };
		input.handleInput('\x1b'); // escape
		expect(escaped).toBe(true);
	});

	it('should handle setValue', () => {
		input.setValue('preset');
		expect(input.getValue()).toBe('preset');
	});

	it('should show placeholder when empty and unfocused', () => {
		input.focused = false;
		input.placeholder = 'Type here...';
		const lines = input.render(40);
		expect(lines[0]).toContain('Type here...');
		expect(lines[0]).toContain('\x1b[2m'); // dim
	});

	it('should handle bracketed paste', () => {
		input.handleInput('\x1b[200~pasted text\x1b[201~');
		expect(input.getValue()).toBe('pasted text');
	});

	it('should strip newlines from paste', () => {
		input.handleInput('\x1b[200~line1\nline2\r\nline3\x1b[201~');
		expect(input.getValue()).toBe('line1line2line3');
	});

	it('should call onChange on value changes', () => {
		const changes: string[] = [];
		input.onChange = (value) => { changes.push(value); };
		input.handleInput('a');
		input.handleInput('b');
		expect(changes).toEqual(['a', 'ab']);
	});

	it('should reject control characters', () => {
		input.handleInput('\x03'); // Ctrl+C (handled by escape)
		input.handleInput('\x00'); // null
		// Only escape handler fires, no character inserted
		expect(input.getValue()).toBe('');
	});
});
