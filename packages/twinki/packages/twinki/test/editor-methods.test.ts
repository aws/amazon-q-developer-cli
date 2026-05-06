import { describe, it, expect, beforeEach } from 'vitest';
import { Editor, type AutocompleteProvider } from '../src/components/Editor.js';
import type { SelectItem } from '../src/components/SelectList.js';

// Key sequences
const UP = '\x1b[A';
const DOWN = '\x1b[B';
const LEFT = '\x1b[D';
const RIGHT = '\x1b[C';
const SHIFT_ENTER = '\x1b[13;2u';
const CTRL_K = '\x0b';
const CTRL_U = '\x15';
const CTRL_A = '\x01';
const CTRL_E = '\x05';
const CTRL_Y = '\x19';
const ALT_Y = '\x1by';
const CTRL_W = '\x17';
const ALT_D = '\x1bd';
const CTRL_D = '\x04';
const BACKSPACE = '\x7f';
const TAB = '\t';
const ENTER = '\r';
const ESCAPE = '\x1b';
const UNDO = '\x1b[45;5u';
const PAGE_UP = '\x1b[5~';
const PAGE_DOWN = '\x1b[6~';
const ALT_LEFT = '\x1b[1;3D';
const ALT_RIGHT = '\x1b[1;3C';
const CTRL_CLOSE_BRACKET = '\x1d';
const CTRL_ALT_CLOSE_BRACKET = '\x1b\x1d';

function makeEditor(rows = 20): Editor {
	const e = new Editor({ terminalRows: rows });
	e.focused = true;
	return e;
}

describe('Editor coverage - word movement', () => {
	let editor: Editor;
	beforeEach(() => { editor = makeEditor(); });

	it('moveWordForwards skips word then whitespace', () => {
		editor.setText('hello world');
		editor.handleInput(CTRL_A);
		editor.handleInput(ALT_RIGHT);
		expect(editor.getCursor().col).toBe(5);
	});

	it('moveWordBackwards from mid-word', () => {
		editor.setText('hello world');
		editor.handleInput(ALT_LEFT);
		expect(editor.getCursor().col).toBe(6);
	});

	it('moveWordForwards wraps to next line', () => {
		editor.setText('ab\ncd');
		editor.handleInput(CTRL_A);
		editor.handleInput(UP);
		editor.handleInput(CTRL_E);
		editor.handleInput(ALT_RIGHT);
		expect(editor.getCursor()).toEqual({ line: 1, col: 0 });
	});

	it('moveWordBackwards wraps to prev line', () => {
		editor.setText('ab\ncd');
		editor.handleInput(CTRL_A);
		editor.handleInput(ALT_LEFT);
		expect(editor.getCursor().line).toBe(0);
	});

	it('moveWordForwards over punctuation', () => {
		editor.setText('a.b');
		editor.handleInput(CTRL_A);
		editor.handleInput(ALT_RIGHT);
		expect(editor.getCursor().col).toBe(1);
	});

	it('moveWordBackwards over punctuation', () => {
		editor.setText('a.b');
		editor.handleInput(ALT_LEFT);
		expect(editor.getCursor().col).toBe(2);
	});
});

describe('Editor coverage - delete word', () => {
	let editor: Editor;
	beforeEach(() => { editor = makeEditor(); });

	it('deleteWordBackwards removes word', () => {
		editor.handleInput('hello world');
		editor.handleInput(CTRL_W);
		expect(editor.getText()).toBe('hello ');
	});

	it('deleteWordBackwards at line start merges lines', () => {
		editor.setText('ab\ncd');
		editor.handleInput(CTRL_A);
		editor.handleInput(CTRL_W);
		expect(editor.getText()).toBe('abcd');
	});

	it('deleteWordForward removes word ahead', () => {
		editor.setText('hello world');
		editor.handleInput(CTRL_A);
		editor.handleInput(ALT_D);
		expect(editor.getText()).toBe(' world');
	});

	it('deleteWordForward at line end merges lines', () => {
		editor.setText('ab\ncd');
		editor.handleInput(CTRL_A);
		editor.handleInput(UP);
		editor.handleInput(CTRL_E);
		editor.handleInput(ALT_D);
		expect(editor.getText()).toBe('abcd');
	});
});

describe('Editor coverage - deleteToEndOfLine / deleteToStartOfLine', () => {
	let editor: Editor;
	beforeEach(() => { editor = makeEditor(); });

	it('deleteToEndOfLine at end merges with next line', () => {
		editor.setText('ab\ncd');
		editor.handleInput(CTRL_A);
		editor.handleInput(UP);
		editor.handleInput(CTRL_E);
		editor.handleInput(CTRL_K);
		expect(editor.getText()).toBe('abcd');
	});

	it('deleteToStartOfLine removes text before cursor', () => {
		editor.handleInput('hello');
		editor.handleInput(CTRL_U);
		expect(editor.getText()).toBe('');
	});

	it('deleteToStartOfLine at col 0 merges with prev line', () => {
		editor.setText('ab\ncd');
		editor.handleInput(CTRL_A);
		editor.handleInput(CTRL_U);
		expect(editor.getText()).toBe('abcd');
	});
});

describe('Editor coverage - handleForwardDelete', () => {
	let editor: Editor;
	beforeEach(() => { editor = makeEditor(); });

	it('deletes char ahead', () => {
		editor.setText('abc');
		editor.handleInput(CTRL_A);
		editor.handleInput(CTRL_D);
		expect(editor.getText()).toBe('bc');
	});

	it('merges lines when at end of line', () => {
		editor.setText('ab\ncd');
		editor.handleInput(CTRL_A);
		editor.handleInput(UP);
		editor.handleInput(CTRL_E);
		editor.handleInput(CTRL_D);
		expect(editor.getText()).toBe('abcd');
	});
});

describe('Editor coverage - addNewLine', () => {
	let editor: Editor;
	beforeEach(() => { editor = makeEditor(); });

	it('splits line at cursor', () => {
		editor.handleInput('abcd');
		editor.handleInput(LEFT);
		editor.handleInput(LEFT);
		editor.handleInput(SHIFT_ENTER);
		expect(editor.getLines()).toEqual(['ab', 'cd']);
	});
});

describe('Editor coverage - handlePaste', () => {
	let editor: Editor;
	beforeEach(() => { editor = makeEditor(); });

	it('single-line paste inserts chars', () => {
		editor.handleInput('\x1b[200~hi\x1b[201~');
		expect(editor.getText()).toBe('hi');
	});

	it('multi-line paste under threshold inserts directly', () => {
		editor.handleInput('\x1b[200~a\nb\nc\x1b[201~');
		expect(editor.getLines()).toEqual(['a', 'b', 'c']);
	});

	it('large paste creates marker', () => {
		const big = Array.from({ length: 15 }, (_, i) => `L${i}`).join('\n');
		editor.handleInput(`\x1b[200~${big}\x1b[201~`);
		expect(editor.getText()).toContain('[paste #1');
		expect(editor.getExpandedText()).toBe(big);
	});

	it('paste with tabs converts to spaces', () => {
		editor.handleInput('\x1b[200~a\tb\x1b[201~');
		expect(editor.getText()).toBe('a    b');
	});
});

describe('Editor coverage - navigateHistory', () => {
	let editor: Editor;
	beforeEach(() => { editor = makeEditor(); });

	it('no history does nothing', () => {
		editor.handleInput(UP);
		expect(editor.getText()).toBe('');
	});

	it('navigates up and down through history', () => {
		editor.addToHistory('first');
		editor.addToHistory('second');
		editor.handleInput(UP);
		expect(editor.getText()).toBe('second');
		editor.handleInput(UP);
		expect(editor.getText()).toBe('first');
		editor.handleInput(UP); // beyond history
		expect(editor.getText()).toBe('first');
		editor.handleInput(DOWN);
		expect(editor.getText()).toBe('second');
		editor.handleInput(DOWN);
		expect(editor.getText()).toBe('');
	});

	it('deduplicates consecutive history entries', () => {
		editor.addToHistory('same');
		editor.addToHistory('same');
		editor.handleInput(UP);
		expect(editor.getText()).toBe('same');
		editor.handleInput(UP);
		expect(editor.getText()).toBe('same');
	});

	it('ignores blank history entries', () => {
		editor.addToHistory('   ');
		editor.handleInput(UP);
		expect(editor.getText()).toBe('');
	});

	it('down arrow at last visual line navigates history forward', () => {
		editor.addToHistory('entry');
		editor.handleInput(UP);
		expect(editor.getText()).toBe('entry');
		editor.handleInput(DOWN);
		expect(editor.getText()).toBe('');
	});
});

describe('Editor coverage - jumpToChar', () => {
	let editor: Editor;
	beforeEach(() => { editor = makeEditor(); });

	it('jumps forward to char', () => {
		editor.handleInput('abcxdef');
		editor.handleInput(CTRL_A);
		editor.handleInput(CTRL_CLOSE_BRACKET); // jumpForward mode
		editor.handleInput('x');
		expect(editor.getCursor().col).toBe(3);
	});

	it('jumps backward to char', () => {
		editor.handleInput('abcxdef');
		editor.handleInput(CTRL_ALT_CLOSE_BRACKET); // jumpBackward mode
		editor.handleInput('x');
		expect(editor.getCursor().col).toBe(3);
	});

	it('cancels jump mode on second jump key', () => {
		editor.handleInput('abc');
		editor.handleInput(CTRL_CLOSE_BRACKET);
		editor.handleInput(CTRL_CLOSE_BRACKET); // cancel
		expect(editor.getCursor().col).toBe(3); // unchanged
	});

	it('jump forward across lines', () => {
		editor.setText('ab\nxd');
		editor.handleInput(CTRL_A);
		editor.handleInput(UP);
		editor.handleInput(CTRL_CLOSE_BRACKET);
		editor.handleInput('x');
		expect(editor.getCursor()).toEqual({ line: 1, col: 0 });
	});
});

describe('Editor coverage - pageScroll', () => {
	let editor: Editor;
	beforeEach(() => { editor = makeEditor(20); });

	it('pageDown moves cursor down', () => {
		let text = '';
		for (let i = 0; i < 30; i++) text += `line${i}\n`;
		editor.setText(text.trim());
		editor.handleInput(CTRL_A);
		for (let i = 0; i < editor.getCursor().line; i++) editor.handleInput(UP);
		editor.handleInput(CTRL_A);
		// Go to first line
		editor.setText(text.trim());
		editor.handleInput(CTRL_A);
		editor.handleInput(UP); editor.handleInput(UP); editor.handleInput(UP);
		const before = editor.getCursor().line;
		editor.handleInput(PAGE_DOWN);
		expect(editor.getCursor().line).toBeGreaterThan(before);
	});

	it('pageUp moves cursor up', () => {
		let text = '';
		for (let i = 0; i < 30; i++) text += `line${i}\n`;
		editor.setText(text.trim());
		const before = editor.getCursor().line;
		editor.handleInput(PAGE_UP);
		expect(editor.getCursor().line).toBeLessThan(before);
	});
});

describe('Editor coverage - yank and yankPop', () => {
	let editor: Editor;
	beforeEach(() => { editor = makeEditor(); });

	it('yank inserts killed text', () => {
		editor.handleInput('hello world');
		editor.handleInput(CTRL_A);
		editor.handleInput(CTRL_K); // kill "hello world"
		editor.handleInput(CTRL_Y); // yank
		expect(editor.getText()).toBe('hello world');
	});

	it('yank does nothing with empty kill ring', () => {
		editor.handleInput('abc');
		editor.handleInput(CTRL_Y);
		expect(editor.getText()).toBe('abc');
	});

	it('yankPop cycles through kill ring', () => {
		editor.handleInput('first');
		editor.handleInput(CTRL_A);
		editor.handleInput(CTRL_K);
		editor.handleInput('second');
		editor.handleInput(CTRL_A);
		editor.handleInput(CTRL_K);
		editor.handleInput(CTRL_Y); // yanks "second"
		expect(editor.getText()).toBe('second');
		editor.handleInput(ALT_Y); // yankPop → "first"
		expect(editor.getText()).toBe('first');
	});

	it('yankPop does nothing if last action was not yank', () => {
		editor.handleInput('text');
		editor.handleInput(CTRL_A);
		editor.handleInput(CTRL_K);
		editor.handleInput(CTRL_Y);
		editor.handleInput('x'); // breaks yank chain
		editor.handleInput(ALT_Y); // should do nothing
		expect(editor.getText()).toContain('x');
	});

	it('yank multi-line text', () => {
		editor.setText('line1\nline2');
		editor.handleInput(CTRL_A);
		editor.handleInput(UP);
		editor.handleInput(CTRL_A);
		editor.handleInput(CTRL_K); // kill line1
		editor.handleInput(CTRL_K); // kill newline (merge)
		editor.handleInput(CTRL_Y); // yank back
		expect(editor.getText()).toContain('line1');
	});
});

describe('Editor coverage - autocomplete', () => {
	let editor: Editor;
	const mockProvider: AutocompleteProvider = {
		getSuggestions(lines, cursorLine, cursorCol) {
			const line = lines[cursorLine] || '';
			const prefix = line.slice(0, cursorCol);
			if (prefix.length === 0) return null;
			return {
				items: [
					{ value: 'hello', label: 'hello' },
					{ value: 'help', label: 'help' },
				],
				prefix,
			};
		},
		applyCompletion(lines, cursorLine, cursorCol, item, prefix) {
			const line = lines[cursorLine] || '';
			const newLine = line.slice(0, cursorCol - prefix.length) + item.value + line.slice(cursorCol);
			return { lines: [...lines.slice(0, cursorLine), newLine, ...lines.slice(cursorLine + 1)], cursorLine, cursorCol: cursorCol - prefix.length + item.value.length };
		},
	};

	beforeEach(() => {
		editor = makeEditor();
		editor.setAutocompleteProvider(mockProvider);
	});

	it('tab triggers autocomplete', () => {
		editor.handleInput('h');
		editor.handleInput(TAB);
		expect(editor.isShowingAutocomplete()).toBe(true);
	});

	it('escape cancels autocomplete', () => {
		editor.handleInput('h');
		editor.handleInput(TAB);
		editor.handleInput(ESCAPE);
		expect(editor.isShowingAutocomplete()).toBe(false);
	});

	it('tab without provider does nothing', () => {
		const e2 = makeEditor();
		e2.handleInput('h');
		e2.handleInput(TAB);
		expect(e2.isShowingAutocomplete()).toBe(false);
	});

	it('tab with no suggestions does nothing', () => {
		editor.handleInput(TAB); // empty line → null suggestions
		expect(editor.isShowingAutocomplete()).toBe(false);
	});

	it('confirm autocomplete applies completion', () => {
		editor.handleInput('h');
		editor.handleInput(TAB);
		editor.handleInput(ENTER); // confirm
		expect(editor.getText()).toBe('hello');
		expect(editor.isShowingAutocomplete()).toBe(false);
	});

	it('typing during autocomplete updates suggestions', () => {
		editor.handleInput('h');
		editor.handleInput(TAB);
		expect(editor.isShowingAutocomplete()).toBe(true);
		editor.handleInput('e');
		// updateAutocomplete is called internally
		expect(editor.isShowingAutocomplete()).toBe(true);
	});

	it('backspace during autocomplete updates suggestions', () => {
		editor.handleInput('he');
		editor.handleInput(TAB);
		editor.handleInput(BACKSPACE);
		expect(editor.isShowingAutocomplete()).toBe(true);
	});

	it('forward delete during autocomplete updates', () => {
		editor.handleInput('he');
		editor.handleInput(LEFT);
		editor.handleInput(TAB);
		editor.handleInput(CTRL_D);
		// autocomplete should still be active or cancelled depending on suggestions
	});

	it('navigate autocomplete with arrows', () => {
		editor.handleInput('h');
		editor.handleInput(TAB);
		editor.handleInput(DOWN); // move to second item
		editor.handleInput(ENTER); // confirm
		expect(editor.getText()).toBe('help');
	});

	it('tab confirms autocomplete', () => {
		editor.handleInput('h');
		editor.handleInput(TAB);
		editor.handleInput(TAB); // confirm with tab
		expect(editor.getText()).toBe('hello');
	});

	it('render shows autocomplete list', () => {
		editor.handleInput('h');
		editor.handleInput(TAB);
		const lines = editor.render(40);
		expect(lines.length).toBeGreaterThan(3); // borders + content + autocomplete
	});
});

describe('Editor coverage - visual line navigation', () => {
	let editor: Editor;
	beforeEach(() => { editor = makeEditor(20); });

	it('up arrow on first visual line goes to line start', () => {
		editor.handleInput('hello');
		editor.handleInput(UP);
		expect(editor.getCursor().col).toBe(0);
	});

	it('down arrow on last visual line goes to line end', () => {
		editor.handleInput('hello');
		editor.handleInput(CTRL_A);
		editor.handleInput(DOWN);
		expect(editor.getCursor().col).toBe(5);
	});

	it('vertical movement across wrapped lines', () => {
		// Create a line long enough to wrap at width 20
		const longLine = 'a'.repeat(50);
		editor.handleInput(longLine);
		editor.render(20); // force layout width
		editor.handleInput(CTRL_A);
		editor.handleInput(DOWN);
		expect(editor.getCursor().col).toBeGreaterThan(0);
	});
});

describe('Editor coverage - utility methods', () => {
	it('isEditorEmpty', () => {
		const editor = makeEditor();
		expect(editor.getText()).toBe('');
		editor.handleInput(UP); // triggers isEditorEmpty → navigateHistory
	});

	it('setTerminalRows', () => {
		const editor = makeEditor();
		editor.setTerminalRows(30);
		// No error thrown
	});

	it('insertTextAtCursor', () => {
		const editor = makeEditor();
		editor.handleInput('ab');
		editor.handleInput(LEFT);
		editor.insertTextAtCursor('X');
		expect(editor.getText()).toBe('aXb');
	});

	it('insertTextAtCursor with empty string', () => {
		const editor = makeEditor();
		editor.insertTextAtCursor('');
		expect(editor.getText()).toBe('');
	});

	it('insertTextAtCursor multi-line', () => {
		const editor = makeEditor();
		editor.handleInput('ab');
		editor.handleInput(CTRL_A);
		editor.insertTextAtCursor('x\ny\nz');
		expect(editor.getLines()).toEqual(['x', 'y', 'zab']);
	});

	it('getExpandedText with no pastes returns text', () => {
		const editor = makeEditor();
		editor.handleInput('hello');
		expect(editor.getExpandedText()).toBe('hello');
	});

	it('setText triggers onChange', () => {
		const editor = makeEditor();
		const changes: string[] = [];
		editor.onChange = (v) => changes.push(v);
		editor.setText('new');
		expect(changes).toContain('new');
	});

	it('undo restores previous state', () => {
		const editor = makeEditor();
		editor.handleInput('hello');
		editor.handleInput(' ');
		editor.handleInput('world');
		editor.handleInput(UNDO);
		expect(editor.getText()).toBe('hello');
	});

	it('undo with nothing to undo', () => {
		const editor = makeEditor();
		editor.handleInput(UNDO);
		expect(editor.getText()).toBe('');
	});

	it('moveCursor right wraps to next line', () => {
		const editor = makeEditor();
		editor.setText('ab\ncd');
		editor.handleInput(CTRL_A);
		editor.handleInput(UP);
		editor.handleInput(CTRL_E);
		editor.handleInput(RIGHT);
		expect(editor.getCursor()).toEqual({ line: 1, col: 0 });
	});

	it('moveCursor left wraps to prev line', () => {
		const editor = makeEditor();
		editor.setText('ab\ncd');
		editor.handleInput(CTRL_A);
		editor.handleInput(LEFT);
		expect(editor.getCursor()).toEqual({ line: 0, col: 2 });
	});

	it('disableSubmit prevents submit', () => {
		const editor = makeEditor();
		let submitted = false;
		editor.onSubmit = () => { submitted = true; };
		editor.disableSubmit = true;
		editor.handleInput('test');
		editor.handleInput(ENTER);
		expect(submitted).toBe(false);
		expect(editor.getText()).toBe('test');
	});

	it('backslash-enter creates newline instead of submit', () => {
		const editor = makeEditor();
		let submitted = false;
		editor.onSubmit = () => { submitted = true; };
		editor.handleInput('hello\\');
		editor.handleInput(ENTER);
		expect(submitted).toBe(false);
		expect(editor.getLines().length).toBe(2);
	});
});

describe('Editor coverage - decodeKittyPrintable', () => {
	it('inserts kitty-encoded printable char', () => {
		const editor = makeEditor();
		// \x1b[65;2u = codepoint 65 (A) with shift modifier → shifted key
		// Simple: \x1b[97u = codepoint 97 = 'a'
		editor.handleInput('\x1b[97u');
		expect(editor.getText()).toBe('a');
	});

	it('ignores kitty sequence with ctrl modifier', () => {
		const editor = makeEditor();
		editor.handleInput('x');
		// \x1b[97;5u = 'a' with ctrl (modifier 5 → 4 = ctrl bit)
		editor.handleInput('\x1b[97;5u');
		expect(editor.getText()).toBe('x'); // ctrl+a is home, not insert
	});

	it('uses shifted key when shift modifier present', () => {
		const editor = makeEditor();
		// \x1b[97:65;2u = codepoint 97, shifted key 65 (A), modifier 2 (shift)
		editor.handleInput('\x1b[97:65;2u');
		expect(editor.getText()).toBe('A');
	});
});

describe('Editor coverage - kill accumulation', () => {
	it('consecutive kills accumulate in kill ring', () => {
		const editor = makeEditor();
		editor.setText('hello\nworld');
		editor.handleInput(CTRL_A);
		editor.handleInput(UP);
		editor.handleInput(CTRL_A);
		editor.handleInput(CTRL_K); // kill "hello"
		editor.handleInput(CTRL_K); // kill newline
		editor.handleInput(CTRL_Y); // yank accumulated
		expect(editor.getText()).toContain('hello');
	});
});

describe('Editor coverage - shift+space', () => {
	it('inserts space on shift+space', () => {
		const editor = makeEditor();
		editor.handleInput('\x1b[32;2u'); // shift+space kitty
		expect(editor.getText()).toBe(' ');
	});
});
