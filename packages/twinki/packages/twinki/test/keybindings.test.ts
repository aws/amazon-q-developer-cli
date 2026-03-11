import { describe, it, expect } from 'vitest';
import { EditorKeybindingsManager, DEFAULT_EDITOR_KEYBINDINGS, getEditorKeybindings } from '../src/input/keybindings.js';

describe('EditorKeybindingsManager', () => {
	it('should match default keybindings', () => {
		const kb = new EditorKeybindingsManager();
		
		expect(kb.matches('\x1b[A', 'cursorUp')).toBe(true);
		expect(kb.matches('\x1b[B', 'cursorDown')).toBe(true);
		expect(kb.matches('\x1b[D', 'cursorLeft')).toBe(true);
		expect(kb.matches('\x1b[C', 'cursorRight')).toBe(true);
		expect(kb.matches('\x01', 'cursorLineStart')).toBe(true); // Ctrl+A
		expect(kb.matches('\x05', 'cursorLineEnd')).toBe(true); // Ctrl+E
	});

	it('should match multiple keys for same action', () => {
		const kb = new EditorKeybindingsManager();
		
		// cursorLeft: ['left', 'ctrl+b']
		expect(kb.matches('\x1b[D', 'cursorLeft')).toBe(true);
		expect(kb.matches('\x02', 'cursorLeft')).toBe(true); // Ctrl+B
	});

	it('should support custom keybindings', () => {
		const kb = new EditorKeybindingsManager({
			submit: 'ctrl+s',
			undo: ['ctrl+z', 'ctrl+u'],
		});
		
		expect(kb.matches('\x13', 'submit')).toBe(true); // Ctrl+S
		expect(kb.matches('\r', 'submit')).toBe(false); // Enter no longer bound
		expect(kb.matches('\x1a', 'undo')).toBe(true); // Ctrl+Z
		expect(kb.matches('\x15', 'undo')).toBe(true); // Ctrl+U
	});

	it('should get keys for action', () => {
		const kb = new EditorKeybindingsManager();
		
		const leftKeys = kb.getKeys('cursorLeft');
		expect(leftKeys).toContain('left');
		expect(leftKeys).toContain('ctrl+b');
	});

	it('should update config', () => {
		const kb = new EditorKeybindingsManager();
		
		expect(kb.matches('\r', 'submit')).toBe(true);
		
		kb.setConfig({ submit: 'ctrl+enter' });
		expect(kb.matches('\r', 'submit')).toBe(false);
		expect(kb.matches('\x1b[13;5u', 'submit')).toBe(true); // Ctrl+Enter
	});

	it('should provide global instance', () => {
		const kb1 = getEditorKeybindings();
		const kb2 = getEditorKeybindings();
		
		expect(kb1).toBe(kb2);
	});

	it('should have all required actions in defaults', () => {
		const actions = Object.keys(DEFAULT_EDITOR_KEYBINDINGS);
		
		expect(actions).toContain('cursorUp');
		expect(actions).toContain('submit');
		expect(actions).toContain('undo');
		expect(actions).toContain('yank');
		expect(actions).toContain('deleteWordBackward');
	});
});
