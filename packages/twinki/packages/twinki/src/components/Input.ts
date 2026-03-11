import { getEditorKeybindings } from '../input/keybindings.js';
import { KillRing } from '../utils/kill-ring.js';
import type { Component, Focusable } from '../renderer/component.js';
import { CURSOR_MARKER } from '../renderer/component.js';
import { UndoStack } from '../utils/undo-stack.js';
import { getSegmenter, isPunctuationChar, isWhitespaceChar, visibleWidth } from '../utils/visible-width.js';

const segmenter = getSegmenter();

interface InputState {
	value: string;
	cursor: number;
}

/**
 * Single-line text input with horizontal scrolling, undo, and Emacs keybindings.
 */
export class Input implements Component, Focusable {
	private value: string = '';
	private cursor: number = 0;
	public onSubmit?: (value: string) => void;
	public onEscape?: () => void;
	public onChange?: (value: string) => void;
	public placeholder?: string;

	focused: boolean = false;

	private pasteBuffer: string = '';
	private isInPaste: boolean = false;
	private killRing = new KillRing();
	private lastAction: 'kill' | 'yank' | 'type-word' | null = null;
	private undoStack = new UndoStack<InputState>();

	getValue(): string {
		return this.value;
	}

	setValue(value: string): void {
		this.value = value;
		this.cursor = Math.min(this.cursor, value.length);
	}

	handleInput(data: string): void {
		if (data.includes('\x1b[200~')) {
			this.isInPaste = true;
			this.pasteBuffer = '';
			data = data.replace('\x1b[200~', '');
		}

		if (this.isInPaste) {
			this.pasteBuffer += data;
			const endIndex = this.pasteBuffer.indexOf('\x1b[201~');
			if (endIndex !== -1) {
				const pasteContent = this.pasteBuffer.substring(0, endIndex);
				this.handlePaste(pasteContent);
				this.isInPaste = false;
				const remaining = this.pasteBuffer.substring(endIndex + 6);
				this.pasteBuffer = '';
				if (remaining) this.handleInput(remaining);
			}
			return;
		}

		const kb = getEditorKeybindings();

		if (kb.matches(data, 'selectCancel')) { this.onEscape?.(); return; }
		if (kb.matches(data, 'undo')) { this.undo(); return; }
		if (kb.matches(data, 'submit') || data === '\n') { this.onSubmit?.(this.value); return; }
		if (kb.matches(data, 'deleteCharBackward')) { this.handleBackspace(); return; }
		if (kb.matches(data, 'deleteCharForward')) { this.handleForwardDelete(); return; }
		if (kb.matches(data, 'deleteWordBackward')) { this.deleteWordBackwards(); return; }
		if (kb.matches(data, 'deleteWordForward')) { this.deleteWordForward(); return; }
		if (kb.matches(data, 'deleteToLineStart')) { this.deleteToLineStart(); return; }
		if (kb.matches(data, 'deleteToLineEnd')) { this.deleteToLineEnd(); return; }
		if (kb.matches(data, 'yank')) { this.yank(); return; }
		if (kb.matches(data, 'yankPop')) { this.yankPop(); return; }

		if (kb.matches(data, 'cursorLeft')) {
			this.lastAction = null;
			if (this.cursor > 0) {
				const graphemes = [...segmenter.segment(this.value.slice(0, this.cursor))];
				const last = graphemes[graphemes.length - 1];
				this.cursor -= last ? last.segment.length : 1;
			}
			return;
		}

		if (kb.matches(data, 'cursorRight')) {
			this.lastAction = null;
			if (this.cursor < this.value.length) {
				const graphemes = [...segmenter.segment(this.value.slice(this.cursor))];
				this.cursor += graphemes[0] ? graphemes[0].segment.length : 1;
			}
			return;
		}

		if (kb.matches(data, 'cursorLineStart')) { this.lastAction = null; this.cursor = 0; return; }
		if (kb.matches(data, 'cursorLineEnd')) { this.lastAction = null; this.cursor = this.value.length; return; }
		if (kb.matches(data, 'cursorWordLeft')) { this.moveWordBackwards(); return; }
		if (kb.matches(data, 'cursorWordRight')) { this.moveWordForwards(); return; }

		// Accept printable characters, reject control chars
		const hasControlChars = [...data].some((ch) => {
			const code = ch.charCodeAt(0);
			return code < 32 || code === 0x7f || (code >= 0x80 && code <= 0x9f);
		});
		if (!hasControlChars) this.insertCharacter(data);
	}

	private insertCharacter(char: string): void {
		if (isWhitespaceChar(char) || this.lastAction !== 'type-word') this.pushUndo();
		this.lastAction = 'type-word';
		this.value = this.value.slice(0, this.cursor) + char + this.value.slice(this.cursor);
		this.cursor += char.length;
		this.onChange?.(this.value);
	}

	private handleBackspace(): void {
		this.lastAction = null;
		if (this.cursor > 0) {
			this.pushUndo();
			const graphemes = [...segmenter.segment(this.value.slice(0, this.cursor))];
			const last = graphemes[graphemes.length - 1];
			const len = last ? last.segment.length : 1;
			this.value = this.value.slice(0, this.cursor - len) + this.value.slice(this.cursor);
			this.cursor -= len;
			this.onChange?.(this.value);
		}
	}

	private handleForwardDelete(): void {
		this.lastAction = null;
		if (this.cursor < this.value.length) {
			this.pushUndo();
			const graphemes = [...segmenter.segment(this.value.slice(this.cursor))];
			const len = graphemes[0] ? graphemes[0].segment.length : 1;
			this.value = this.value.slice(0, this.cursor) + this.value.slice(this.cursor + len);
			this.onChange?.(this.value);
		}
	}

	private deleteToLineStart(): void {
		if (this.cursor === 0) return;
		this.pushUndo();
		const deleted = this.value.slice(0, this.cursor);
		this.killRing.push(deleted, { prepend: true, accumulate: this.lastAction === 'kill' });
		this.lastAction = 'kill';
		this.value = this.value.slice(this.cursor);
		this.cursor = 0;
		this.onChange?.(this.value);
	}

	private deleteToLineEnd(): void {
		if (this.cursor >= this.value.length) return;
		this.pushUndo();
		const deleted = this.value.slice(this.cursor);
		this.killRing.push(deleted, { prepend: false, accumulate: this.lastAction === 'kill' });
		this.lastAction = 'kill';
		this.value = this.value.slice(0, this.cursor);
		this.onChange?.(this.value);
	}

	private deleteWordBackwards(): void {
		if (this.cursor === 0) return;
		const wasKill = this.lastAction === 'kill';
		this.pushUndo();
		const oldCursor = this.cursor;
		this.moveWordBackwards();
		const deleted = this.value.slice(this.cursor, oldCursor);
		this.killRing.push(deleted, { prepend: true, accumulate: wasKill });
		this.lastAction = 'kill';
		this.value = this.value.slice(0, this.cursor) + this.value.slice(oldCursor);
		this.onChange?.(this.value);
	}

	private deleteWordForward(): void {
		if (this.cursor >= this.value.length) return;
		const wasKill = this.lastAction === 'kill';
		this.pushUndo();
		const oldCursor = this.cursor;
		this.moveWordForwards();
		const deleted = this.value.slice(oldCursor, this.cursor);
		this.killRing.push(deleted, { prepend: false, accumulate: wasKill });
		this.lastAction = 'kill';
		this.value = this.value.slice(0, oldCursor) + this.value.slice(this.cursor);
		this.cursor = oldCursor;
		this.onChange?.(this.value);
	}

	private yank(): void {
		const text = this.killRing.peek();
		if (!text) return;
		this.pushUndo();
		this.value = this.value.slice(0, this.cursor) + text + this.value.slice(this.cursor);
		this.cursor += text.length;
		this.lastAction = 'yank';
		this.onChange?.(this.value);
	}

	private yankPop(): void {
		if (this.lastAction !== 'yank' || this.killRing.length <= 1) return;
		this.pushUndo();
		const prevText = this.killRing.peek() || '';
		this.value = this.value.slice(0, this.cursor - prevText.length) + this.value.slice(this.cursor);
		this.cursor -= prevText.length;
		this.killRing.rotate();
		const text = this.killRing.peek() || '';
		this.value = this.value.slice(0, this.cursor) + text + this.value.slice(this.cursor);
		this.cursor += text.length;
		this.lastAction = 'yank';
		this.onChange?.(this.value);
	}

	private pushUndo(): void {
		this.undoStack.push({ value: this.value, cursor: this.cursor });
	}

	private undo(): void {
		const snapshot = this.undoStack.pop();
		if (!snapshot) return;
		this.value = snapshot.value;
		this.cursor = snapshot.cursor;
		this.lastAction = null;
		this.onChange?.(this.value);
	}

	private moveWordBackwards(): void {
		if (this.cursor === 0) return;
		this.lastAction = null;
		const graphemes = [...segmenter.segment(this.value.slice(0, this.cursor))];

		while (graphemes.length > 0 && isWhitespaceChar(graphemes[graphemes.length - 1]?.segment || '')) {
			this.cursor -= graphemes.pop()?.segment.length || 0;
		}

		if (graphemes.length > 0) {
			const last = graphemes[graphemes.length - 1]?.segment || '';
			if (isPunctuationChar(last)) {
				while (graphemes.length > 0 && isPunctuationChar(graphemes[graphemes.length - 1]?.segment || '')) {
					this.cursor -= graphemes.pop()?.segment.length || 0;
				}
			} else {
				while (graphemes.length > 0 && !isWhitespaceChar(graphemes[graphemes.length - 1]?.segment || '') && !isPunctuationChar(graphemes[graphemes.length - 1]?.segment || '')) {
					this.cursor -= graphemes.pop()?.segment.length || 0;
				}
			}
		}
	}

	private moveWordForwards(): void {
		if (this.cursor >= this.value.length) return;
		this.lastAction = null;
		const segments = segmenter.segment(this.value.slice(this.cursor));
		const iterator = segments[Symbol.iterator]();
		let next = iterator.next();

		while (!next.done && isWhitespaceChar(next.value.segment)) {
			this.cursor += next.value.segment.length;
			next = iterator.next();
		}

		if (!next.done) {
			if (isPunctuationChar(next.value.segment)) {
				while (!next.done && isPunctuationChar(next.value.segment)) {
					this.cursor += next.value.segment.length;
					next = iterator.next();
				}
			} else {
				while (!next.done && !isWhitespaceChar(next.value.segment) && !isPunctuationChar(next.value.segment)) {
					this.cursor += next.value.segment.length;
					next = iterator.next();
				}
			}
		}
	}

	private handlePaste(pastedText: string): void {
		this.lastAction = null;
		this.pushUndo();
		const cleanText = pastedText.replace(/\r\n/g, '').replace(/\r/g, '').replace(/\n/g, '');
		this.value = this.value.slice(0, this.cursor) + cleanText + this.value.slice(this.cursor);
		this.cursor += cleanText.length;
		this.onChange?.(this.value);
	}

	invalidate(): void {}

	render(width: number): string[] {
		const prompt = '> ';
		const availableWidth = width - prompt.length;
		if (availableWidth <= 0) return [prompt];

		// Show placeholder when empty and not focused
		if (!this.value && this.placeholder && !this.focused) {
			return [prompt + `\x1b[2m${this.placeholder}\x1b[22m`];
		}

		let visibleText = '';
		let cursorDisplay = this.cursor;

		if (this.value.length < availableWidth) {
			visibleText = this.value;
		} else {
			const scrollWidth = this.cursor === this.value.length ? availableWidth - 1 : availableWidth;
			const halfWidth = Math.floor(scrollWidth / 2);

			const findValidStart = (start: number) => {
				while (start < this.value.length) {
					const code = this.value.charCodeAt(start);
					if (code >= 0xdc00 && code < 0xe000) { start++; continue; }
					break;
				}
				return start;
			};

			const findValidEnd = (end: number) => {
				while (end > 0) {
					const code = this.value.charCodeAt(end - 1);
					if (code >= 0xd800 && code < 0xdc00) { end--; continue; }
					break;
				}
				return end;
			};

			if (this.cursor < halfWidth) {
				visibleText = this.value.slice(0, findValidEnd(scrollWidth));
				cursorDisplay = this.cursor;
			} else if (this.cursor > this.value.length - halfWidth) {
				const start = findValidStart(this.value.length - scrollWidth);
				visibleText = this.value.slice(start);
				cursorDisplay = this.cursor - start;
			} else {
				const start = findValidStart(this.cursor - halfWidth);
				visibleText = this.value.slice(start, findValidEnd(start + scrollWidth));
				cursorDisplay = halfWidth;
			}
		}

		const graphemes = [...segmenter.segment(visibleText.slice(cursorDisplay))];
		const cursorGrapheme = graphemes[0];
		const beforeCursor = visibleText.slice(0, cursorDisplay);
		const atCursor = cursorGrapheme?.segment ?? ' ';
		const afterCursor = visibleText.slice(cursorDisplay + atCursor.length);

		const marker = this.focused ? CURSOR_MARKER : '';
		const cursorChar = `\x1b[7m${atCursor}\x1b[27m`;
		const textWithCursor = beforeCursor + marker + cursorChar + afterCursor;

		const visualLength = visibleWidth(textWithCursor);
		const padding = ' '.repeat(Math.max(0, availableWidth - visualLength));

		return [prompt + textWithCursor + padding];
	}
}
