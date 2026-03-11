/**
 * Shared test infrastructure for E2E tests.
 * Provides FrameCapturingTerminal, helpers, and analyzers.
 */
import pkg from '@xterm/headless';
const { Terminal: XtermTerminal } = pkg;
import { TUI } from '../src/renderer/tui.js';
import type { Component } from '../src/renderer/component.js';
import type { Terminal } from '../src/terminal/terminal.js';
import { mkdirSync, writeFileSync } from 'node:fs';
import { join } from 'node:path';

// --- Artifact output ---

const ARTIFACTS_DIR = join(import.meta.dirname, '.artifacts');

// Registry: tests register their terminal so afterEach can auto-dump
const activeTerminals: TestTerminal[] = [];

export function getArtifactsDir(): string { return ARTIFACTS_DIR; }

function sanitizeName(name: string): string {
	return name.replace(/[^a-zA-Z0-9_-]/g, '_').replace(/_+/g, '_').slice(0, 120);
}

export function testDir(suiteName: string, testName: string): string {
	const dir = join(ARTIFACTS_DIR, sanitizeName(suiteName), sanitizeName(testName));
	mkdirSync(dir, { recursive: true });
	return dir;
}

export function dumpLastFrame(term: TestTerminal, dir: string): void {
	const frame = term.getLastFrame();
	if (!frame) return;
	const width = term.columns;
	writeFileSync(join(dir, 'last-frame.txt'), serializeFrame(frame, width) + '\n');
}

export function dumpAllFrames(term: TestTerminal, dir: string): void {
	const frames = term.getFrames();
	if (frames.length === 0) return;
	const width = term.columns;
	const parts: string[] = [];
	for (let i = 0; i < frames.length; i++) {
		const f = frames[i]!;
		parts.push(serializeFrame(f, width));
		if (i < frames.length - 1) {
			const diff = diffFrames(f, frames[i + 1]!);
			if (diff.length > 0) {
				parts.push(`  Changes → Frame ${i + 1}:`);
				for (const d of diff) parts.push(`    ${d}`);
			} else {
				parts.push(`  (no changes → Frame ${i + 1})`);
			}
		}
		parts.push('');
	}

	// Append flicker report
	const flicker = analyzeFlicker(frames);
	parts.push(`--- Flicker Report ---`);
	parts.push(`Clean: ${flicker.clean}`);
	if (!flicker.clean) {
		for (const e of flicker.events.slice(0, 20)) {
			parts.push(`  flicker at frame ${e.frameIndex}, row ${e.row}, col ${e.col}`);
		}
		if (flicker.events.length > 20) parts.push(`  ... and ${flicker.events.length - 20} more`);
	}
	parts.push('');

	writeFileSync(join(dir, 'all-frames.txt'), parts.join('\n'));
}

/** Get the active terminals registered this test. Called by setup.ts afterEach. */
export function _getActiveTerminals(): TestTerminal[] { return activeTerminals; }
export function _clearActiveTerminals(): void { activeTerminals.length = 0; }

// --- Frame types ---

export interface Frame {
	index: number;
	timestamp: bigint;
	viewport: string[];
	writeBytes: number;
	isFull: boolean;
}

// --- FrameCapturingTerminal ---

export class TestTerminal implements Terminal {
	private xterm: InstanceType<typeof XtermTerminal>;
	private inputHandler?: (data: string) => void;
	private resizeHandler?: () => void;
	private _cols: number;
	private _rows: number;
	private frames: Frame[] = [];
	private frameIndex = 0;
	private pendingCapture = false;
	private pendingBytes = 0;
	private pendingIsFull = false;

	constructor(cols = 40, rows = 10) {
		this._cols = cols;
		this._rows = rows;
		this.xterm = new XtermTerminal({ cols, rows, allowProposedApi: true });
		activeTerminals.push(this);
	}

	get kittyProtocolActive() { return true; }
	get columns() { return this._cols; }
	get rows() { return this._rows; }
	start(onInput: (data: string) => void, onResize: () => void) {
		this.inputHandler = onInput;
		this.resizeHandler = onResize;
	}
	stop() {}
	async drainInput() {}

	write(data: string) {
		this.xterm.write(data);
		if (data.includes('\x1b[?2026l')) {
			this.pendingCapture = true;
			this.pendingBytes = data.length;
			this.pendingIsFull = data.includes('\x1b[3J') || data.includes('\x1b[2J');
		}
	}

	sendInput(data: string) { this.inputHandler?.(data); }
	resize(cols: number, rows: number) {
		this._cols = cols;
		this._rows = rows;
		this.xterm.resize(cols, rows);
		this.resizeHandler?.();
	}
	moveBy(n: number) { if (n > 0) this.write(`\x1b[${n}B`); else if (n < 0) this.write(`\x1b[${-n}A`); }
	hideCursor() { this.write('\x1b[?25l'); }
	showCursor() { this.write('\x1b[?25h'); }
	clearLine() { this.write('\x1b[K'); }
	clearFromCursor() { this.write('\x1b[J'); }
	clearScreen() { this.write('\x1b[2J\x1b[H'); }
	setTitle() {}
	enableMouse() {}
	disableMouse() {}

	async flush(): Promise<void> {
		await new Promise<void>(resolve => this.xterm.write('', resolve));
		if (this.pendingCapture) {
			this.frames.push({
				index: this.frameIndex++,
				timestamp: process.hrtime.bigint(),
				viewport: this.getViewport(),
				writeBytes: this.pendingBytes,
				isFull: this.pendingIsFull,
			});
			this.pendingCapture = false;
			this.pendingBytes = 0;
			this.pendingIsFull = false;
		}
	}

	getViewport(): string[] {
		const buf = this.xterm.buffer.active;
		const lines: string[] = [];
		for (let i = 0; i < this._rows; i++) {
			const line = buf.getLine(buf.viewportY + i);
			lines.push(line ? line.translateToString(true) : '');
		}
		return lines;
	}

	getFrames(): Frame[] { return [...this.frames]; }
	getLastFrame(): Frame | undefined { return this.frames[this.frames.length - 1]; }
}

// --- Mutable component ---

export class MutableComponent implements Component {
	lines: string[] = [];
	render() { return this.lines; }
	invalidate() {}
}

// --- Analyzers ---

export interface FlickerEvent {
	frameIndex: number;
	row: number;
	col: number;
}

export function analyzeFlicker(frames: Frame[]): { events: FlickerEvent[]; clean: boolean } {
	const events: FlickerEvent[] = [];
	for (let i = 1; i < frames.length - 1; i++) {
		const prev = frames[i - 1]!;
		const curr = frames[i]!;
		const next = frames[i + 1]!;

		// Skip frames where content height changed — that's a layout shift, not flicker.
		// Flicker is when the SAME cell goes non-blank → blank → non-blank.
		// Layout shifts (adding/removing lines) naturally cause rows to move.
		const prevHeight = prev.viewport.filter(l => l.trim() !== '').length;
		const currHeight = curr.viewport.filter(l => l.trim() !== '').length;
		const nextHeight = next.viewport.filter(l => l.trim() !== '').length;
		if (prevHeight !== currHeight || currHeight !== nextHeight) continue;

		const maxRows = Math.max(prev.viewport.length, curr.viewport.length, next.viewport.length);
		for (let row = 0; row < maxRows; row++) {
			const prevLine = prev.viewport[row] ?? '';
			const currLine = curr.viewport[row] ?? '';
			const nextLine = next.viewport[row] ?? '';
			const maxCols = Math.max(prevLine.length, currLine.length, nextLine.length);
			for (let col = 0; col < maxCols; col++) {
				const p = prevLine[col] ?? ' ';
				const c = currLine[col] ?? ' ';
				const n = nextLine[col] ?? ' ';
				if (p !== ' ' && c === ' ' && n !== ' ') {
					events.push({ frameIndex: i, row, col });
				}
			}
		}
	}
	return { events, clean: events.length === 0 };
}

export function diffFrames(a: Frame, b: Frame): string[] {
	const changed: string[] = [];
	const max = Math.max(a.viewport.length, b.viewport.length);
	for (let i = 0; i < max; i++) {
		if ((a.viewport[i] ?? '') !== (b.viewport[i] ?? '')) {
			changed.push(`row ${i}: ${JSON.stringify(a.viewport[i])} → ${JSON.stringify(b.viewport[i])}`);
		}
	}
	return changed;
}

export function serializeFrame(frame: Frame, width = 40): string {
	const header = `Frame ${frame.index} (${frame.writeBytes}B, ${frame.isFull ? 'full' : 'diff'}):`;
	const top = '┌' + '─'.repeat(width) + '┐';
	const bottom = '└' + '─'.repeat(width) + '┘';
	const lines = frame.viewport.map(l => '│' + l.padEnd(width) + '│');
	return [header, top, ...lines, bottom].join('\n');
}

// --- Helpers ---

export async function wait(ms = 15) {
	await new Promise(r => setTimeout(r, ms));
}

export async function renderAndCapture(
	term: TestTerminal,
	tui: TUI,
	comp: MutableComponent,
	lines: string[],
): Promise<Frame> {
	comp.lines = lines;
	tui.requestRender();
	await wait();
	await term.flush();
	return term.getLastFrame()!;
}
