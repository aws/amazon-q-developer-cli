import { describe, it, expect, beforeEach } from 'vitest';
import { Editor } from '../src/components/Editor.js';

describe('Editor', () => {
	let editor: Editor;

	beforeEach(() => {
		editor = new Editor({ terminalRows: 20 });
		editor.focused = true;
	});

	it('should start empty', () => {
		expect(editor.getText()).toBe('');
		expect(editor.getCursor()).toEqual({ line: 0, col: 0 });
	});

	it('should accept character input', () => {
		editor.handleInput('h');
		editor.handleInput('i');
		expect(editor.getText()).toBe('hi');
		expect(editor.getCursor()).toEqual({ line: 0, col: 2 });
	});

	it('should handle new line', () => {
		editor.handleInput('hello');
		editor.handleInput('\x1b[13;2u'); // Shift+Enter (newline)
		editor.handleInput('world');
		expect(editor.getLines()).toEqual(['hello', 'world']);
	});

	it('should handle backspace within line', () => {
		editor.handleInput('abc');
		editor.handleInput('\x7f'); // backspace
		expect(editor.getText()).toBe('ab');
	});

	it('should handle backspace at line start (merge)', () => {
		editor.setText('hello\nworld');
		// Cursor is at end of 'world'. Move to start of 'world'
		editor.handleInput('\x01'); // Ctrl+A (home)
		editor.handleInput('\x7f'); // backspace
		expect(editor.getText()).toBe('helloworld');
	});

	it('should handle undo', () => {
		editor.handleInput('hello');
		editor.handleInput(' ');
		editor.handleInput('world');
		editor.handleInput('\x1b[45;5u'); // Ctrl+- (undo)
		expect(editor.getText()).toBe('hello');
	});

	it('should handle kill to end of line', () => {
		editor.handleInput('hello world');
		editor.handleInput('\x01'); // home
		editor.handleInput('\x0b'); // Ctrl+K
		expect(editor.getText()).toBe('');
	});

	it('should handle yank', () => {
		editor.handleInput('hello');
		editor.handleInput('\x01'); // home
		editor.handleInput('\x0b'); // Ctrl+K
		editor.handleInput('\x19'); // Ctrl+Y
		expect(editor.getText()).toBe('hello');
	});

	it('should handle submit', () => {
		let submitted = '';
		editor.onSubmit = (v) => { submitted = v; };
		editor.handleInput('test');
		editor.handleInput('\r'); // enter
		expect(submitted).toBe('test');
		expect(editor.getText()).toBe('');
	});

	it('should handle setText', () => {
		editor.setText('preset\ntext');
		expect(editor.getLines()).toEqual(['preset', 'text']);
	});

	it('should render with borders', () => {
		editor.handleInput('hello');
		const lines = editor.render(40);
		expect(lines.length).toBeGreaterThanOrEqual(3); // top border + content + bottom border
		expect(lines[0]).toContain('─');
		expect(lines[lines.length - 1]).toContain('─');
	});

	it('should handle bracketed paste', () => {
		editor.handleInput('\x1b[200~pasted text\x1b[201~');
		expect(editor.getText()).toBe('pasted text');
	});

	it('should create paste markers for large pastes', () => {
		const longText = Array.from({ length: 15 }, (_, i) => `line ${i}`).join('\n');
		editor.handleInput(`\x1b[200~${longText}\x1b[201~`);
		expect(editor.getText()).toContain('[paste #1');
		expect(editor.getExpandedText()).toBe(longText);
	});

	it('should handle history navigation', () => {
		editor.addToHistory('first');
		editor.addToHistory('second');
		editor.handleInput('\x1b[A'); // up (empty editor → history)
		expect(editor.getText()).toBe('second');
		editor.handleInput('\x1b[A'); // up
		expect(editor.getText()).toBe('first');
		editor.handleInput('\x1b[B'); // down
		expect(editor.getText()).toBe('second');
	});

	it('should handle cursor movement', () => {
		editor.handleInput('abc');
		editor.handleInput('\x1b[D'); // left
		editor.handleInput('X');
		expect(editor.getText()).toBe('abXc');
	});

	it('should call onChange', () => {
		const changes: string[] = [];
		editor.onChange = (v) => { changes.push(v); };
		editor.handleInput('a');
		editor.handleInput('b');
		expect(changes).toEqual(['a', 'ab']);
	});

	it('should handle scroll indicators in render', () => {
		editor = new Editor({ terminalRows: 10 }); // maxVisible = max(5, floor(10*0.3)) = 5
		editor.focused = true;
		// Add many lines
		for (let i = 0; i < 10; i++) {
			editor.handleInput(`line ${i}`);
			if (i < 9) editor.handleInput('\x1b[13;2u'); // Shift+Enter
		}
		const lines = editor.render(40);
		// Should have scroll indicator
		expect(lines[0]).toContain('↑');
	});
});

describe('Editor line numbers', () => {
	let editor: Editor;

	beforeEach(() => {
		editor = new Editor({ terminalRows: 20 });
		editor.focused = true;
		editor.lineNumbers = true;
	});

	it('renders line numbers in gutter', () => {
		editor.handleInput('line one');
		editor.handleInput('\x1b[13;2u');
		editor.handleInput('line two');
		editor.handleInput('\x1b[13;2u');
		editor.handleInput('line three');
		const output = editor.render(40);
		// Strip ANSI for checking
		const clean = output.map(l => l.replace(/\x1b\[[^m]*m/g, ''));
		const hasLineNum1 = clean.some(l => l.includes('1 │'));
		const hasLineNum2 = clean.some(l => l.includes('2 │'));
		const hasLineNum3 = clean.some(l => l.includes('3 │'));
		expect(hasLineNum1).toBe(true);
		expect(hasLineNum2).toBe(true);
		expect(hasLineNum3).toBe(true);
	});

	it('does not render line numbers when disabled', () => {
		editor.lineNumbers = false;
		editor.handleInput('hello');
		const output = editor.render(40);
		const clean = output.map(l => l.replace(/\x1b\[[^m]*m/g, ''));
		const hasGutter = clean.some(l => l.includes('│'));
		expect(hasGutter).toBe(false);
	});
});

describe('Editor syntax highlighting', () => {
	let editor: Editor;

	beforeEach(() => {
		editor = new Editor({ terminalRows: 20 });
		editor.focused = true;
	});

	it('uses highlighted lines when set', () => {
		editor.handleInput('const x = 1');
		const hlMap = new Map<number, string>();
		hlMap.set(0, '\x1b[38;2;255;0;0mconst\x1b[0m \x1b[38;2;0;255;0mx\x1b[0m = 1');
		editor.setHighlightedLines(hlMap);
		const output = editor.render(40);
		// Should contain the red color code from highlighting
		const hasHighlight = output.some(l => l.includes('\x1b[38;2;255;0;0m'));
		expect(hasHighlight).toBe(true);
	});

	it('falls back to plain text when no highlight available', () => {
		editor.handleInput('hello');
		editor.setHighlightedLines(new Map());
		const output = editor.render(40);
		const clean = output.join('').replace(/\x1b\[[^m]*m/g, '');
		expect(clean).toContain('hello');
	});
});
