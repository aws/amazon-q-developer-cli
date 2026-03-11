import { getEditorKeybindings } from '../input/keybindings.js';
import { matchesKey } from '../input/keys.js';
import { KillRing } from '../utils/kill-ring.js';
import type { Component, Focusable } from '../renderer/component.js';
import { CURSOR_MARKER } from '../renderer/component.js';
import { UndoStack } from '../utils/undo-stack.js';
import { getSegmenter, isPunctuationChar, isWhitespaceChar, visibleWidth } from '../utils/visible-width.js';
import { sliceByColumn } from '../utils/slice.js';
import { wordWrapLine } from '../utils/word-wrap.js';
import { SelectList, type SelectItem, type SelectListTheme } from './SelectList.js';

const segmenter = getSegmenter();

// Kitty CSI-u sequences for printable keys
const KITTY_CSI_U_REGEX = /^\x1b\[(\d+)(?::(\d*))?(?::(\d+))?(?:;(\d+))?(?::(\d+))?u$/;
const KITTY_MOD_SHIFT = 1;
const KITTY_MOD_ALT = 2;
const KITTY_MOD_CTRL = 4;

function decodeKittyPrintable(data: string): string | undefined {
	const match = data.match(KITTY_CSI_U_REGEX);
	if (!match) return undefined;
	const codepoint = Number.parseInt(match[1] ?? '', 10);
	if (!Number.isFinite(codepoint)) return undefined;
	const shiftedKey = match[2] && match[2].length > 0 ? Number.parseInt(match[2], 10) : undefined;
	const modValue = match[4] ? Number.parseInt(match[4], 10) : 1;
	const modifier = Number.isFinite(modValue) ? modValue - 1 : 0;
	if (modifier & (KITTY_MOD_ALT | KITTY_MOD_CTRL)) return undefined;
	let effectiveCodepoint = codepoint;
	if (modifier & KITTY_MOD_SHIFT && typeof shiftedKey === 'number') effectiveCodepoint = shiftedKey;
	if (!Number.isFinite(effectiveCodepoint) || effectiveCodepoint < 32) return undefined;
	try { return String.fromCodePoint(effectiveCodepoint); } catch { return undefined; }
}

interface EditorState {
	lines: string[];
	cursorLine: number;
	cursorCol: number;
}

interface LayoutLine {
	text: string;
	hasCursor: boolean;
	cursorPos?: number;
	logicalLine?: number;
}

export interface AutocompleteProvider {
	getSuggestions(lines: string[], cursorLine: number, cursorCol: number): {
		items: SelectItem[];
		prefix: string;
	} | null;
	applyCompletion(lines: string[], cursorLine: number, cursorCol: number, item: SelectItem, prefix: string): {
		lines: string[];
		cursorLine: number;
		cursorCol: number;
	};
}

export interface EditorTheme {
	borderColor: (str: string) => string;
	selectList: SelectListTheme;
}

const DEFAULT_EDITOR_THEME: EditorTheme = {
	borderColor: (t) => `\x1b[2m${t}\x1b[22m`,
	selectList: {
		selectedPrefix: (t) => `\x1b[1m${t}\x1b[22m`,
		selectedText: (t) => `\x1b[1m${t}\x1b[22m`,
		description: (t) => `\x1b[2m${t}\x1b[22m`,
		scrollInfo: (t) => `\x1b[2m${t}\x1b[22m`,
		noMatch: (t) => `\x1b[2m${t}\x1b[22m`,
	},
};

export class Editor implements Component, Focusable {
	private state: EditorState = { lines: [''], cursorLine: 0, cursorCol: 0 };
	focused: boolean = false;

	private theme: EditorTheme;
	private paddingX: number = 0;
	private lastWidth: number = 80;
	private scrollOffset: number = 0;
	private terminalRows: number = 24;

	private autocompleteProvider?: AutocompleteProvider;
	private autocompleteList?: SelectList;
	private autocompleteState: 'regular' | null = null;
	private autocompletePrefix: string = '';
	private autocompleteMaxVisible: number = 5;

	private pastes: Map<number, string> = new Map();
	private pasteCounter: number = 0;
	private pasteBuffer: string = '';
	private isInPaste: boolean = false;

	private history: string[] = [];
	private historyIndex: number = -1;
	private killRing = new KillRing();
	private lastAction: 'kill' | 'yank' | 'type-word' | null = null;
	private jumpMode: 'forward' | 'backward' | null = null;
	private preferredVisualCol: number | null = null;
	private undoStack = new UndoStack<EditorState>();

	public onSubmit?: (text: string) => void;
	public onChange?: (text: string) => void;
	public disableSubmit: boolean = false;
	public borderColor: (str: string) => string;
	public lineNumbers: boolean = false;

	private highlightedLines: Map<number, string> = new Map();

	constructor(options: { paddingX?: number; autocompleteMaxVisible?: number; theme?: EditorTheme; terminalRows?: number } = {}) {
		this.theme = options.theme ?? DEFAULT_EDITOR_THEME;
		this.borderColor = this.theme.borderColor;
		this.paddingX = Math.max(0, options.paddingX ?? 0);
		this.autocompleteMaxVisible = Math.max(3, Math.min(20, options.autocompleteMaxVisible ?? 5));
		this.terminalRows = options.terminalRows ?? 24;
	}

	setTerminalRows(rows: number): void { this.terminalRows = rows; }
	setAutocompleteProvider(provider: AutocompleteProvider): void { this.autocompleteProvider = provider; }
	isShowingAutocomplete(): boolean { return this.autocompleteState !== null; }

	setHighlightedLines(lines: Map<number, string>): void { this.highlightedLines = lines; }

	getText(): string { return this.state.lines.join('\n'); }
	getLines(): string[] { return [...this.state.lines]; }
	getCursor(): { line: number; col: number } { return { line: this.state.cursorLine, col: this.state.cursorCol }; }

	getExpandedText(): string {
		let result = this.state.lines.join('\n');
		for (const [pasteId, pasteContent] of this.pastes) {
			const re = new RegExp(`\\[paste #${pasteId}( (\\+\\d+ lines|\\d+ chars))?\\]`, 'g');
			result = result.replace(re, pasteContent);
		}
		return result;
	}

	setText(text: string): void {
		this.lastAction = null;
		this.historyIndex = -1;
		if (this.getText() !== text) this.pushUndoSnapshot();
		this.setTextInternal(text);
	}

	addToHistory(text: string): void {
		const trimmed = text.trim();
		if (!trimmed) return;
		if (this.history.length > 0 && this.history[0] === trimmed) return;
		this.history.unshift(trimmed);
		if (this.history.length > 100) this.history.pop();
	}

	insertTextAtCursor(text: string): void {
		if (!text) return;
		this.pushUndoSnapshot();
		this.lastAction = null;
		this.historyIndex = -1;
		this.insertTextAtCursorInternal(text);
	}

	invalidate(): void {}

	render(width: number): string[] {
		const maxPadding = Math.max(0, Math.floor((width - 1) / 2));
		const px = Math.min(this.paddingX, maxPadding);

		// Line number gutter: " N │ "
		const totalLines = this.state.lines.length;
		const gutterWidth = this.lineNumbers ? String(totalLines).length + 3 : 0;

		const contentWidth = Math.max(1, width - px * 2 - gutterWidth);
		const layoutWidth = Math.max(1, contentWidth - (px ? 0 : 1));
		this.lastWidth = layoutWidth;

		const horizontal = this.borderColor('─');
		const layoutLines = this.layoutText(layoutWidth);
		const maxVisibleLines = Math.max(5, Math.floor(this.terminalRows * 0.3));

		let cursorLineIndex = layoutLines.findIndex((l) => l.hasCursor);
		if (cursorLineIndex === -1) cursorLineIndex = 0;

		if (cursorLineIndex < this.scrollOffset) this.scrollOffset = cursorLineIndex;
		else if (cursorLineIndex >= this.scrollOffset + maxVisibleLines) this.scrollOffset = cursorLineIndex - maxVisibleLines + 1;
		this.scrollOffset = Math.max(0, Math.min(this.scrollOffset, Math.max(0, layoutLines.length - maxVisibleLines)));

		const visibleLines = layoutLines.slice(this.scrollOffset, this.scrollOffset + maxVisibleLines);
		const result: string[] = [];
		const leftPad = ' '.repeat(px);
		const rightPad = leftPad;
		const emitCursorMarker = this.focused && !this.autocompleteState;
		const digits = gutterWidth > 0 ? String(totalLines).length : 0;

		// Top border
		if (this.scrollOffset > 0) {
			const ind = `─── ↑ ${this.scrollOffset} more `;
			result.push(this.borderColor(ind + '─'.repeat(Math.max(0, width - visibleWidth(ind)))));
		} else {
			result.push(horizontal.repeat(width));
		}

		// Track which logical line we're on for gutter numbering
		let lastLogicalLine = -1;

		for (const ll of visibleLines) {
			let displayText = ll.text;
			let lineVW = visibleWidth(ll.text);
			let cursorInPadding = false;

			// Use syntax-highlighted version if available
			const hlText = ll.logicalLine !== undefined ? this.highlightedLines.get(ll.logicalLine) : undefined;
			if (hlText !== undefined) {
				displayText = hlText;
				lineVW = visibleWidth(hlText);
			}

			if (ll.hasCursor && ll.cursorPos !== undefined) {
				// cursorPos is a byte offset into the plain text; convert to visible column
				const visCol = visibleWidth(ll.text.slice(0, ll.cursorPos));
				const before = sliceByColumn(displayText, 0, visCol);
				const marker = emitCursorMarker ? CURSOR_MARKER : '';

				// Get the cursor character from plain text to avoid ANSI codes in fg
				const plainAfter = ll.text.slice(ll.cursorPos);
				if (plainAfter.length > 0) {
					const fg = [...segmenter.segment(plainAfter)][0]?.segment || '';
					const fgWidth = visibleWidth(fg);
					displayText = before + marker + `\x1b[7m${fg}\x1b[0m` + sliceByColumn(displayText, visCol + fgWidth, contentWidth);
				} else {
					displayText = before + marker + '\x1b[7m \x1b[0m';
					lineVW += 1;
					if (lineVW > contentWidth && px > 0) cursorInPadding = true;
				}
			}

			// Build gutter prefix
			let gutter = '';
			if (gutterWidth > 0) {
				const logLine = ll.logicalLine ?? 0;
				const isFirstChunk = logLine !== lastLogicalLine;
				lastLogicalLine = logLine;
				const numStr = isFirstChunk ? String(logLine + 1).padStart(digits) : ' '.repeat(digits);
				gutter = `\x1b[2m${numStr} │ \x1b[0m`;
			}

			const padding = ' '.repeat(Math.max(0, contentWidth - lineVW));
			const rp = cursorInPadding ? rightPad.slice(1) : rightPad;
			result.push(`${leftPad}${gutter}${displayText}${padding}${rp}`);
		}

		// Bottom border
		const linesBelow = layoutLines.length - (this.scrollOffset + visibleLines.length);
		if (linesBelow > 0) {
			const ind = `─── ↓ ${linesBelow} more `;
			result.push(this.borderColor(ind + '─'.repeat(Math.max(0, width - visibleWidth(ind)))));
		} else {
			result.push(horizontal.repeat(width));
		}

		// Autocomplete
		if (this.autocompleteState && this.autocompleteList) {
			for (const line of this.autocompleteList.render(contentWidth)) {
				const lw = visibleWidth(line);
				result.push(`${leftPad}${line}${' '.repeat(Math.max(0, contentWidth - lw))}${rightPad}`);
			}
		}

		return result;
	}

	handleInput(data: string): void {
		const kb = getEditorKeybindings();

		// Jump mode
		if (this.jumpMode !== null) {
			if (kb.matches(data, 'jumpForward') || kb.matches(data, 'jumpBackward')) { this.jumpMode = null; return; }
			if (data.charCodeAt(0) >= 32) {
				const dir = this.jumpMode;
				this.jumpMode = null;
				this.jumpToChar(data, dir);
				return;
			}
			this.jumpMode = null;
		}

		// Bracketed paste
		if (data.includes('\x1b[200~')) {
			this.isInPaste = true;
			this.pasteBuffer = '';
			data = data.replace('\x1b[200~', '');
		}
		if (this.isInPaste) {
			this.pasteBuffer += data;
			const endIndex = this.pasteBuffer.indexOf('\x1b[201~');
			if (endIndex !== -1) {
				const content = this.pasteBuffer.substring(0, endIndex);
				if (content.length > 0) this.handlePaste(content);
				this.isInPaste = false;
				const remaining = this.pasteBuffer.substring(endIndex + 6);
				this.pasteBuffer = '';
				if (remaining.length > 0) this.handleInput(remaining);
			}
			return;
		}

		if (kb.matches(data, 'undo')) { this.undo(); return; }

		// Autocomplete mode
		if (this.autocompleteState && this.autocompleteList) {
			if (kb.matches(data, 'selectCancel')) { this.cancelAutocomplete(); return; }
			if (kb.matches(data, 'selectUp') || kb.matches(data, 'selectDown')) { this.autocompleteList.handleInput(data); return; }
			if (kb.matches(data, 'tab') || kb.matches(data, 'selectConfirm')) {
				const selected = this.autocompleteList.getSelectedItem();
				if (selected && this.autocompleteProvider) {
					this.pushUndoSnapshot();
					this.lastAction = null;
					const result = this.autocompleteProvider.applyCompletion(this.state.lines, this.state.cursorLine, this.state.cursorCol, selected, this.autocompletePrefix);
					this.state.lines = result.lines;
					this.state.cursorLine = result.cursorLine;
					this.setCursorCol(result.cursorCol);
					this.cancelAutocomplete();
					this.onChange?.(this.getText());
				}
				return;
			}
		}

		if (kb.matches(data, 'tab') && !this.autocompleteState) { this.tryTriggerAutocomplete(); return; }

		// Deletion
		if (kb.matches(data, 'deleteToLineEnd')) { this.deleteToEndOfLine(); return; }
		if (kb.matches(data, 'deleteToLineStart')) { this.deleteToStartOfLine(); return; }
		if (kb.matches(data, 'deleteWordBackward')) { this.deleteWordBackwards(); return; }
		if (kb.matches(data, 'deleteWordForward')) { this.deleteWordForward(); return; }
		if (kb.matches(data, 'deleteCharBackward') || matchesKey(data, 'shift+backspace')) { this.handleBackspace(); return; }
		if (kb.matches(data, 'deleteCharForward') || matchesKey(data, 'shift+delete')) { this.handleForwardDelete(); return; }

		// Kill ring
		if (kb.matches(data, 'yank')) { this.yank(); return; }
		if (kb.matches(data, 'yankPop')) { this.yankPop(); return; }

		// Cursor movement
		if (kb.matches(data, 'cursorLineStart')) { this.moveToLineStart(); return; }
		if (kb.matches(data, 'cursorLineEnd')) { this.moveToLineEnd(); return; }
		if (kb.matches(data, 'cursorWordLeft')) { this.moveWordBackwards(); return; }
		if (kb.matches(data, 'cursorWordRight')) { this.moveWordForwards(); return; }

		// New line
		if (kb.matches(data, 'newLine') || (data.charCodeAt(0) === 10 && data.length > 1) || data === '\x1b\r' || (data === '\n' && data.length === 1)) {
			this.addNewLine();
			return;
		}

		// Submit
		if (kb.matches(data, 'submit')) {
			if (this.disableSubmit) return;
			const currentLine = this.state.lines[this.state.cursorLine] || '';
			if (this.state.cursorCol > 0 && currentLine[this.state.cursorCol - 1] === '\\') {
				this.handleBackspace();
				this.addNewLine();
				return;
			}
			this.submitValue();
			return;
		}

		// Arrow keys with history
		if (kb.matches(data, 'cursorUp')) {
			if (this.isEditorEmpty()) this.navigateHistory(-1);
			else if (this.historyIndex > -1 && this.isOnFirstVisualLine()) this.navigateHistory(-1);
			else if (this.isOnFirstVisualLine()) this.moveToLineStart();
			else this.moveCursor(-1, 0);
			return;
		}
		if (kb.matches(data, 'cursorDown')) {
			if (this.historyIndex > -1 && this.isOnLastVisualLine()) this.navigateHistory(1);
			else if (this.isOnLastVisualLine()) this.moveToLineEnd();
			else this.moveCursor(1, 0);
			return;
		}
		if (kb.matches(data, 'cursorRight')) { this.moveCursor(0, 1); return; }
		if (kb.matches(data, 'cursorLeft')) { this.moveCursor(0, -1); return; }

		if (kb.matches(data, 'pageUp')) { this.pageScroll(-1); return; }
		if (kb.matches(data, 'pageDown')) { this.pageScroll(1); return; }
		if (kb.matches(data, 'jumpForward')) { this.jumpMode = 'forward'; return; }
		if (kb.matches(data, 'jumpBackward')) { this.jumpMode = 'backward'; return; }

		if (matchesKey(data, 'shift+space')) { this.insertCharacter(' '); return; }

		const kittyPrintable = decodeKittyPrintable(data);
		if (kittyPrintable !== undefined) { this.insertCharacter(kittyPrintable); return; }

		if (data.charCodeAt(0) >= 32) this.insertCharacter(data);
	}

	// --- Private: text layout ---

	private layoutText(contentWidth: number): LayoutLine[] {
		const layoutLines: LayoutLine[] = [];
		if (this.state.lines.length === 0 || (this.state.lines.length === 1 && this.state.lines[0] === '')) {
			layoutLines.push({ text: '', hasCursor: true, cursorPos: 0, logicalLine: 0 });
			return layoutLines;
		}
		for (let i = 0; i < this.state.lines.length; i++) {
			const line = this.state.lines[i] || '';
			const isCurrentLine = i === this.state.cursorLine;
			if (visibleWidth(line) <= contentWidth) {
				layoutLines.push({ text: line, hasCursor: isCurrentLine, cursorPos: isCurrentLine ? this.state.cursorCol : undefined, logicalLine: i });
			} else {
				const chunks = wordWrapLine(line, contentWidth);
				for (let ci = 0; ci < chunks.length; ci++) {
					const chunk = chunks[ci]!;
					const isLast = ci === chunks.length - 1;
					let hasCursor = false;
					let adjustedPos = 0;
					if (isCurrentLine) {
						const cp = this.state.cursorCol;
						if (isLast) {
							hasCursor = cp >= chunk.startIndex;
							adjustedPos = cp - chunk.startIndex;
						} else {
							hasCursor = cp >= chunk.startIndex && cp < chunk.endIndex;
							if (hasCursor) adjustedPos = Math.min(cp - chunk.startIndex, chunk.text.length);
						}
					}
					layoutLines.push({ text: chunk.text, hasCursor, cursorPos: hasCursor ? adjustedPos : undefined, logicalLine: i });
				}
			}
		}
		return layoutLines;
	}

	// --- Private: text manipulation ---

	private setTextInternal(text: string): void {
		const lines = text.replace(/\r\n/g, '\n').replace(/\r/g, '\n').split('\n');
		this.state.lines = lines.length === 0 ? [''] : lines;
		this.state.cursorLine = this.state.lines.length - 1;
		this.setCursorCol(this.state.lines[this.state.cursorLine]?.length || 0);
		this.scrollOffset = 0;
		this.highlightedLines.clear();
		this.onChange?.(this.getText());
	}

	private insertTextAtCursorInternal(text: string): void {
		if (!text) return;
		const normalized = text.replace(/\r\n/g, '\n').replace(/\r/g, '\n');
		const insertedLines = normalized.split('\n');
		const currentLine = this.state.lines[this.state.cursorLine] || '';
		const before = currentLine.slice(0, this.state.cursorCol);
		const after = currentLine.slice(this.state.cursorCol);
		if (insertedLines.length === 1) {
			this.state.lines[this.state.cursorLine] = before + normalized + after;
			this.setCursorCol(this.state.cursorCol + normalized.length);
		} else {
			this.state.lines = [
				...this.state.lines.slice(0, this.state.cursorLine),
				before + insertedLines[0],
				...insertedLines.slice(1, -1),
				insertedLines[insertedLines.length - 1] + after,
				...this.state.lines.slice(this.state.cursorLine + 1),
			];
			this.state.cursorLine += insertedLines.length - 1;
			this.setCursorCol((insertedLines[insertedLines.length - 1] || '').length);
		}
		this.onChange?.(this.getText());
	}

	private insertCharacter(char: string, skipUndoCoalescing?: boolean): void {
		this.historyIndex = -1;
		if (!skipUndoCoalescing) {
			if (isWhitespaceChar(char) || this.lastAction !== 'type-word') this.pushUndoSnapshot();
			this.lastAction = 'type-word';
		}
		const line = this.state.lines[this.state.cursorLine] || '';
		this.state.lines[this.state.cursorLine] = line.slice(0, this.state.cursorCol) + char + line.slice(this.state.cursorCol);
		this.setCursorCol(this.state.cursorCol + char.length);
		this.onChange?.(this.getText());
		if (this.autocompleteState) this.updateAutocomplete();
	}

	private handlePaste(pastedText: string): void {
		this.historyIndex = -1;
		this.lastAction = null;
		this.pushUndoSnapshot();
		let filtered = pastedText.replace(/\r\n/g, '\n').replace(/\r/g, '\n').replace(/\t/g, '    ')
			.split('').filter((c) => c === '\n' || c.charCodeAt(0) >= 32).join('');
		const pastedLines = filtered.split('\n');
		if (pastedLines.length > 10 || filtered.length > 1000) {
			this.pasteCounter++;
			const id = this.pasteCounter;
			this.pastes.set(id, filtered);
			const marker = pastedLines.length > 10 ? `[paste #${id} +${pastedLines.length} lines]` : `[paste #${id} ${filtered.length} chars]`;
			this.insertTextAtCursorInternal(marker);
			return;
		}
		if (pastedLines.length === 1) {
			for (const c of filtered) this.insertCharacter(c, true);
			return;
		}
		this.insertTextAtCursorInternal(filtered);
	}

	private addNewLine(): void {
		this.historyIndex = -1;
		this.lastAction = null;
		this.pushUndoSnapshot();
		const line = this.state.lines[this.state.cursorLine] || '';
		this.state.lines[this.state.cursorLine] = line.slice(0, this.state.cursorCol);
		this.state.lines.splice(this.state.cursorLine + 1, 0, line.slice(this.state.cursorCol));
		this.state.cursorLine++;
		this.setCursorCol(0);
		this.onChange?.(this.getText());
	}

	private submitValue(): void {
		let result = this.state.lines.join('\n').trim();
		for (const [id, content] of this.pastes) {
			result = result.replace(new RegExp(`\\[paste #${id}( (\\+\\d+ lines|\\d+ chars))?\\]`, 'g'), content);
		}
		this.state = { lines: [''], cursorLine: 0, cursorCol: 0 };
		this.pastes.clear();
		this.pasteCounter = 0;
		this.historyIndex = -1;
		this.scrollOffset = 0;
		this.undoStack.clear();
		this.lastAction = null;
		this.onChange?.('');
		this.onSubmit?.(result);
	}

	// --- Private: deletion ---

	private handleBackspace(): void {
		this.historyIndex = -1;
		this.lastAction = null;
		if (this.state.cursorCol > 0) {
			this.pushUndoSnapshot();
			const line = this.state.lines[this.state.cursorLine] || '';
			const graphemes = [...segmenter.segment(line.slice(0, this.state.cursorCol))];
			const last = graphemes[graphemes.length - 1];
			const len = last ? last.segment.length : 1;
			this.state.lines[this.state.cursorLine] = line.slice(0, this.state.cursorCol - len) + line.slice(this.state.cursorCol);
			this.setCursorCol(this.state.cursorCol - len);
		} else if (this.state.cursorLine > 0) {
			this.pushUndoSnapshot();
			const cur = this.state.lines[this.state.cursorLine] || '';
			const prev = this.state.lines[this.state.cursorLine - 1] || '';
			this.state.lines[this.state.cursorLine - 1] = prev + cur;
			this.state.lines.splice(this.state.cursorLine, 1);
			this.state.cursorLine--;
			this.setCursorCol(prev.length);
		}
		this.onChange?.(this.getText());
		if (this.autocompleteState) this.updateAutocomplete();
	}

	private handleForwardDelete(): void {
		this.historyIndex = -1;
		this.lastAction = null;
		const line = this.state.lines[this.state.cursorLine] || '';
		if (this.state.cursorCol < line.length) {
			this.pushUndoSnapshot();
			const graphemes = [...segmenter.segment(line.slice(this.state.cursorCol))];
			const len = graphemes[0] ? graphemes[0].segment.length : 1;
			this.state.lines[this.state.cursorLine] = line.slice(0, this.state.cursorCol) + line.slice(this.state.cursorCol + len);
		} else if (this.state.cursorLine < this.state.lines.length - 1) {
			this.pushUndoSnapshot();
			const next = this.state.lines[this.state.cursorLine + 1] || '';
			this.state.lines[this.state.cursorLine] = line + next;
			this.state.lines.splice(this.state.cursorLine + 1, 1);
		}
		this.onChange?.(this.getText());
		if (this.autocompleteState) this.updateAutocomplete();
	}

	private deleteToStartOfLine(): void {
		this.historyIndex = -1;
		const line = this.state.lines[this.state.cursorLine] || '';
		if (this.state.cursorCol > 0) {
			this.pushUndoSnapshot();
			this.killRing.push(line.slice(0, this.state.cursorCol), { prepend: true, accumulate: this.lastAction === 'kill' });
			this.lastAction = 'kill';
			this.state.lines[this.state.cursorLine] = line.slice(this.state.cursorCol);
			this.setCursorCol(0);
		} else if (this.state.cursorLine > 0) {
			this.pushUndoSnapshot();
			this.killRing.push('\n', { prepend: true, accumulate: this.lastAction === 'kill' });
			this.lastAction = 'kill';
			const prev = this.state.lines[this.state.cursorLine - 1] || '';
			this.state.lines[this.state.cursorLine - 1] = prev + line;
			this.state.lines.splice(this.state.cursorLine, 1);
			this.state.cursorLine--;
			this.setCursorCol(prev.length);
		}
		this.onChange?.(this.getText());
	}

	private deleteToEndOfLine(): void {
		this.historyIndex = -1;
		const line = this.state.lines[this.state.cursorLine] || '';
		if (this.state.cursorCol < line.length) {
			this.pushUndoSnapshot();
			this.killRing.push(line.slice(this.state.cursorCol), { prepend: false, accumulate: this.lastAction === 'kill' });
			this.lastAction = 'kill';
			this.state.lines[this.state.cursorLine] = line.slice(0, this.state.cursorCol);
		} else if (this.state.cursorLine < this.state.lines.length - 1) {
			this.pushUndoSnapshot();
			this.killRing.push('\n', { prepend: false, accumulate: this.lastAction === 'kill' });
			this.lastAction = 'kill';
			const next = this.state.lines[this.state.cursorLine + 1] || '';
			this.state.lines[this.state.cursorLine] = line + next;
			this.state.lines.splice(this.state.cursorLine + 1, 1);
		}
		this.onChange?.(this.getText());
	}

	private deleteWordBackwards(): void {
		this.historyIndex = -1;
		const line = this.state.lines[this.state.cursorLine] || '';
		if (this.state.cursorCol === 0) {
			if (this.state.cursorLine > 0) {
				this.pushUndoSnapshot();
				this.killRing.push('\n', { prepend: true, accumulate: this.lastAction === 'kill' });
				this.lastAction = 'kill';
				const prev = this.state.lines[this.state.cursorLine - 1] || '';
				this.state.lines[this.state.cursorLine - 1] = prev + line;
				this.state.lines.splice(this.state.cursorLine, 1);
				this.state.cursorLine--;
				this.setCursorCol(prev.length);
			}
		} else {
			this.pushUndoSnapshot();
			const wasKill = this.lastAction === 'kill';
			const oldCol = this.state.cursorCol;
			this.moveWordBackwards();
			const deleted = line.slice(this.state.cursorCol, oldCol);
			this.killRing.push(deleted, { prepend: true, accumulate: wasKill });
			this.lastAction = 'kill';
			this.state.lines[this.state.cursorLine] = line.slice(0, this.state.cursorCol) + line.slice(oldCol);
		}
		this.onChange?.(this.getText());
	}

	private deleteWordForward(): void {
		this.historyIndex = -1;
		const line = this.state.lines[this.state.cursorLine] || '';
		if (this.state.cursorCol >= line.length) {
			if (this.state.cursorLine < this.state.lines.length - 1) {
				this.pushUndoSnapshot();
				this.killRing.push('\n', { prepend: false, accumulate: this.lastAction === 'kill' });
				this.lastAction = 'kill';
				const next = this.state.lines[this.state.cursorLine + 1] || '';
				this.state.lines[this.state.cursorLine] = line + next;
				this.state.lines.splice(this.state.cursorLine + 1, 1);
			}
		} else {
			this.pushUndoSnapshot();
			const wasKill = this.lastAction === 'kill';
			const oldCol = this.state.cursorCol;
			this.moveWordForwards();
			const deleted = line.slice(oldCol, this.state.cursorCol);
			this.killRing.push(deleted, { prepend: false, accumulate: wasKill });
			this.lastAction = 'kill';
			this.state.lines[this.state.cursorLine] = line.slice(0, oldCol) + line.slice(this.state.cursorCol);
			this.setCursorCol(oldCol);
		}
		this.onChange?.(this.getText());
	}

	// --- Private: cursor movement ---

	private setCursorCol(col: number): void {
		this.state.cursorCol = col;
		this.preferredVisualCol = null;
	}

	private moveToLineStart(): void { this.lastAction = null; this.setCursorCol(0); }
	private moveToLineEnd(): void { this.lastAction = null; this.setCursorCol((this.state.lines[this.state.cursorLine] || '').length); }

	private moveCursor(deltaLine: number, deltaCol: number): void {
		this.lastAction = null;
		const visualLines = this.buildVisualLineMap(this.lastWidth);
		const currentVL = this.findCurrentVisualLine(visualLines);

		if (deltaLine !== 0) {
			const target = currentVL + deltaLine;
			if (target >= 0 && target < visualLines.length) this.moveToVisualLine(visualLines, currentVL, target);
		}

		if (deltaCol !== 0) {
			const line = this.state.lines[this.state.cursorLine] || '';
			if (deltaCol > 0) {
				if (this.state.cursorCol < line.length) {
					const graphemes = [...segmenter.segment(line.slice(this.state.cursorCol))];
					this.setCursorCol(this.state.cursorCol + (graphemes[0] ? graphemes[0].segment.length : 1));
				} else if (this.state.cursorLine < this.state.lines.length - 1) {
					this.state.cursorLine++;
					this.setCursorCol(0);
				}
			} else {
				if (this.state.cursorCol > 0) {
					const graphemes = [...segmenter.segment(line.slice(0, this.state.cursorCol))];
					const last = graphemes[graphemes.length - 1];
					this.setCursorCol(this.state.cursorCol - (last ? last.segment.length : 1));
				} else if (this.state.cursorLine > 0) {
					this.state.cursorLine--;
					this.setCursorCol((this.state.lines[this.state.cursorLine] || '').length);
				}
			}
		}
	}

	private moveWordBackwards(): void {
		this.lastAction = null;
		const line = this.state.lines[this.state.cursorLine] || '';
		if (this.state.cursorCol === 0) {
			if (this.state.cursorLine > 0) { this.state.cursorLine--; this.setCursorCol((this.state.lines[this.state.cursorLine] || '').length); }
			return;
		}
		const graphemes = [...segmenter.segment(line.slice(0, this.state.cursorCol))];
		let col = this.state.cursorCol;
		while (graphemes.length > 0 && isWhitespaceChar(graphemes[graphemes.length - 1]?.segment || '')) col -= graphemes.pop()?.segment.length || 0;
		if (graphemes.length > 0) {
			const last = graphemes[graphemes.length - 1]?.segment || '';
			if (isPunctuationChar(last)) {
				while (graphemes.length > 0 && isPunctuationChar(graphemes[graphemes.length - 1]?.segment || '')) col -= graphemes.pop()?.segment.length || 0;
			} else {
				while (graphemes.length > 0 && !isWhitespaceChar(graphemes[graphemes.length - 1]?.segment || '') && !isPunctuationChar(graphemes[graphemes.length - 1]?.segment || '')) col -= graphemes.pop()?.segment.length || 0;
			}
		}
		this.setCursorCol(col);
	}

	private moveWordForwards(): void {
		this.lastAction = null;
		const line = this.state.lines[this.state.cursorLine] || '';
		if (this.state.cursorCol >= line.length) {
			if (this.state.cursorLine < this.state.lines.length - 1) { this.state.cursorLine++; this.setCursorCol(0); }
			return;
		}
		const segments = segmenter.segment(line.slice(this.state.cursorCol));
		const iter = segments[Symbol.iterator]();
		let next = iter.next();
		let col = this.state.cursorCol;
		while (!next.done && isWhitespaceChar(next.value.segment)) { col += next.value.segment.length; next = iter.next(); }
		if (!next.done) {
			if (isPunctuationChar(next.value.segment)) {
				while (!next.done && isPunctuationChar(next.value.segment)) { col += next.value.segment.length; next = iter.next(); }
			} else {
				while (!next.done && !isWhitespaceChar(next.value.segment) && !isPunctuationChar(next.value.segment)) { col += next.value.segment.length; next = iter.next(); }
			}
		}
		this.setCursorCol(col);
	}

	private pageScroll(direction: -1 | 1): void {
		this.lastAction = null;
		const pageSize = Math.max(5, Math.floor(this.terminalRows * 0.3));
		const visualLines = this.buildVisualLineMap(this.lastWidth);
		const current = this.findCurrentVisualLine(visualLines);
		const target = Math.max(0, Math.min(visualLines.length - 1, current + direction * pageSize));
		this.moveToVisualLine(visualLines, current, target);
	}

	private jumpToChar(char: string, direction: 'forward' | 'backward'): void {
		this.lastAction = null;
		const isForward = direction === 'forward';
		const lines = this.state.lines;
		const end = isForward ? lines.length : -1;
		const step = isForward ? 1 : -1;
		for (let li = this.state.cursorLine; li !== end; li += step) {
			const line = lines[li] || '';
			const from = li === this.state.cursorLine ? (isForward ? this.state.cursorCol + 1 : this.state.cursorCol - 1) : undefined;
			const idx = isForward ? line.indexOf(char, from) : line.lastIndexOf(char, from);
			if (idx !== -1) { this.state.cursorLine = li; this.setCursorCol(idx); return; }
		}
	}

	// --- Private: visual line map ---

	private buildVisualLineMap(width: number): Array<{ logicalLine: number; startCol: number; length: number }> {
		const vls: Array<{ logicalLine: number; startCol: number; length: number }> = [];
		for (let i = 0; i < this.state.lines.length; i++) {
			const line = this.state.lines[i] || '';
			if (line.length === 0) { vls.push({ logicalLine: i, startCol: 0, length: 0 }); }
			else if (visibleWidth(line) <= width) { vls.push({ logicalLine: i, startCol: 0, length: line.length }); }
			else { for (const c of wordWrapLine(line, width)) vls.push({ logicalLine: i, startCol: c.startIndex, length: c.endIndex - c.startIndex }); }
		}
		return vls;
	}

	private findCurrentVisualLine(vls: Array<{ logicalLine: number; startCol: number; length: number }>): number {
		for (let i = 0; i < vls.length; i++) {
			const vl = vls[i]!;
			if (vl.logicalLine === this.state.cursorLine) {
				const col = this.state.cursorCol - vl.startCol;
				const isLast = i === vls.length - 1 || vls[i + 1]?.logicalLine !== vl.logicalLine;
				if (col >= 0 && (col < vl.length || (isLast && col <= vl.length))) return i;
			}
		}
		return vls.length - 1;
	}

	private moveToVisualLine(vls: Array<{ logicalLine: number; startCol: number; length: number }>, from: number, to: number): void {
		const src = vls[from];
		const tgt = vls[to];
		if (!src || !tgt) return;
		const currentVisualCol = this.state.cursorCol - src.startCol;
		const srcIsLast = from === vls.length - 1 || vls[from + 1]?.logicalLine !== src.logicalLine;
		const tgtIsLast = to === vls.length - 1 || vls[to + 1]?.logicalLine !== tgt.logicalLine;
		const srcMax = srcIsLast ? src.length : Math.max(0, src.length - 1);
		const tgtMax = tgtIsLast ? tgt.length : Math.max(0, tgt.length - 1);
		const moveToCol = this.computeVerticalMoveColumn(currentVisualCol, srcMax, tgtMax);
		this.state.cursorLine = tgt.logicalLine;
		this.state.cursorCol = Math.min(tgt.startCol + moveToCol, (this.state.lines[tgt.logicalLine] || '').length);
	}

	private computeVerticalMoveColumn(currentVisualCol: number, sourceMax: number, targetMax: number): number {
		const hasPreferred = this.preferredVisualCol !== null;
		const cursorInMiddle = currentVisualCol < sourceMax;
		const targetTooShort = targetMax < currentVisualCol;
		if (!hasPreferred || cursorInMiddle) {
			if (targetTooShort) { this.preferredVisualCol = currentVisualCol; return targetMax; }
			this.preferredVisualCol = null;
			return currentVisualCol;
		}
		const targetCantFitPreferred = targetMax < this.preferredVisualCol!;
		if (targetTooShort || targetCantFitPreferred) return targetMax;
		const result = this.preferredVisualCol!;
		this.preferredVisualCol = null;
		return result;
	}

	// --- Private: history ---

	private isEditorEmpty(): boolean { return this.state.lines.length === 1 && this.state.lines[0] === ''; }
	private isOnFirstVisualLine(): boolean { return this.findCurrentVisualLine(this.buildVisualLineMap(this.lastWidth)) === 0; }
	private isOnLastVisualLine(): boolean { const vls = this.buildVisualLineMap(this.lastWidth); return this.findCurrentVisualLine(vls) === vls.length - 1; }

	private navigateHistory(direction: 1 | -1): void {
		this.lastAction = null;
		if (this.history.length === 0) return;
		const newIndex = this.historyIndex - direction;
		if (newIndex < -1 || newIndex >= this.history.length) return;
		if (this.historyIndex === -1 && newIndex >= 0) this.pushUndoSnapshot();
		this.historyIndex = newIndex;
		this.setTextInternal(this.historyIndex === -1 ? '' : this.history[this.historyIndex] || '');
	}

	// --- Private: kill ring ---

	private yank(): void {
		if (this.killRing.length === 0) return;
		this.pushUndoSnapshot();
		this.insertYankedText(this.killRing.peek()!);
		this.lastAction = 'yank';
	}

	private yankPop(): void {
		if (this.lastAction !== 'yank' || this.killRing.length <= 1) return;
		this.pushUndoSnapshot();
		this.deleteYankedText();
		this.killRing.rotate();
		this.insertYankedText(this.killRing.peek()!);
		this.lastAction = 'yank';
	}

	private insertYankedText(text: string): void {
		this.historyIndex = -1;
		const lines = text.split('\n');
		if (lines.length === 1) {
			const line = this.state.lines[this.state.cursorLine] || '';
			this.state.lines[this.state.cursorLine] = line.slice(0, this.state.cursorCol) + text + line.slice(this.state.cursorCol);
			this.setCursorCol(this.state.cursorCol + text.length);
		} else {
			const line = this.state.lines[this.state.cursorLine] || '';
			const before = line.slice(0, this.state.cursorCol);
			const after = line.slice(this.state.cursorCol);
			this.state.lines[this.state.cursorLine] = before + (lines[0] || '');
			for (let i = 1; i < lines.length - 1; i++) this.state.lines.splice(this.state.cursorLine + i, 0, lines[i] || '');
			const lastIdx = this.state.cursorLine + lines.length - 1;
			this.state.lines.splice(lastIdx, 0, (lines[lines.length - 1] || '') + after);
			this.state.cursorLine = lastIdx;
			this.setCursorCol((lines[lines.length - 1] || '').length);
		}
		this.onChange?.(this.getText());
	}

	private deleteYankedText(): void {
		const yanked = this.killRing.peek();
		if (!yanked) return;
		const yankLines = yanked.split('\n');
		if (yankLines.length === 1) {
			const line = this.state.lines[this.state.cursorLine] || '';
			this.state.lines[this.state.cursorLine] = line.slice(0, this.state.cursorCol - yanked.length) + line.slice(this.state.cursorCol);
			this.setCursorCol(this.state.cursorCol - yanked.length);
		} else {
			const startLine = this.state.cursorLine - (yankLines.length - 1);
			const startCol = (this.state.lines[startLine] || '').length - (yankLines[0] || '').length;
			const afterCursor = (this.state.lines[this.state.cursorLine] || '').slice(this.state.cursorCol);
			const beforeYank = (this.state.lines[startLine] || '').slice(0, startCol);
			this.state.lines.splice(startLine, yankLines.length, beforeYank + afterCursor);
			this.state.cursorLine = startLine;
			this.setCursorCol(startCol);
		}
		this.onChange?.(this.getText());
	}

	// --- Private: undo ---

	private pushUndoSnapshot(): void { this.undoStack.push(this.state); }

	private undo(): void {
		this.historyIndex = -1;
		const snapshot = this.undoStack.pop();
		if (!snapshot) return;
		Object.assign(this.state, snapshot);
		this.lastAction = null;
		this.preferredVisualCol = null;
		this.onChange?.(this.getText());
	}

	// --- Private: autocomplete ---

	private tryTriggerAutocomplete(): void {
		if (!this.autocompleteProvider) return;
		const suggestions = this.autocompleteProvider.getSuggestions(this.state.lines, this.state.cursorLine, this.state.cursorCol);
		if (suggestions && suggestions.items.length > 0) {
			this.autocompletePrefix = suggestions.prefix;
			this.autocompleteList = new SelectList(suggestions.items, this.autocompleteMaxVisible, this.theme.selectList);
			this.autocompleteState = 'regular';
		} else {
			this.cancelAutocomplete();
		}
	}

	private updateAutocomplete(): void {
		if (!this.autocompleteState || !this.autocompleteProvider) return;
		const suggestions = this.autocompleteProvider.getSuggestions(this.state.lines, this.state.cursorLine, this.state.cursorCol);
		if (suggestions && suggestions.items.length > 0) {
			this.autocompletePrefix = suggestions.prefix;
			this.autocompleteList = new SelectList(suggestions.items, this.autocompleteMaxVisible, this.theme.selectList);
		} else {
			this.cancelAutocomplete();
		}
	}

	private cancelAutocomplete(): void {
		this.autocompleteState = null;
		this.autocompleteList = undefined;
		this.autocompletePrefix = '';
	}
}
