/**
 * Shared test infrastructure for E2E tests.
 * Provides FrameCapturingTerminal, helpers, and analyzers.
 */
import pkg from '@xterm/headless';
const { Terminal: XtermTerminal } = pkg;
import { TUI } from '../src/renderer/tui.js';
import type { Instance } from '../src/reconciler/render.js';
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

// --- Full-color screenshots (SVG) ---

/** One styled run of characters on a row (same fg/bg/bold). */
interface StyledRun {
	text: string;
	x: number;
	fg: string | null;
	bg: string | null;
	bold: boolean;
}

const CELL_W = 9;
const CELL_H = 18;
const DEFAULT_FG = '#fcfcfa';
const DEFAULT_BG = '#221f22';

/** 256-color palette index → hex (standard xterm palette). */
function paletteToHex(idx: number): string {
	if (idx < 16) {
		const base = [
			'#000000', '#cd0000', '#00cd00', '#cdcd00', '#0000ee', '#cd00cd', '#00cdcd', '#e5e5e5',
			'#7f7f7f', '#ff0000', '#00ff00', '#ffff00', '#5c5cff', '#ff00ff', '#00ffff', '#ffffff',
		];
		return base[idx];
	}
	if (idx < 232) {
		const c = idx - 16;
		const steps = [0, 95, 135, 175, 215, 255];
		const r = steps[Math.floor(c / 36)];
		const g = steps[Math.floor((c % 36) / 6)];
		const b = steps[c % 6];
		return `#${((r << 16) | (g << 8) | b).toString(16).padStart(6, '0')}`;
	}
	const v = 8 + (idx - 232) * 10;
	return `#${((v << 16) | (v << 8) | v).toString(16).padStart(6, '0')}`;
}

function escapeXml(s: string): string {
	return s.replace(/&/g, '&amp;').replace(/</g, '&lt;').replace(/>/g, '&gt;');
}

/**
 * Renders the terminal's CURRENT buffer to a full-color SVG screenshot,
 * reading per-cell RGB/palette colors and bold from the xterm buffer (the
 * plain-text Frame type drops these). Views in any browser; `rsvg-convert`
 * or a browser turns it into PNG.
 */
export function screenshotSvg(term: TestTerminal): string {
	const buf = term.xtermBuffer();
	const cols = term.columns;
	const rows = term.rows;
	const rowRuns: StyledRun[][] = [];
	const bgRects: Array<{ x: number; y: number; w: number; color: string }> = [];

	for (let y = 0; y < rows; y++) {
		const line = buf.getLine(buf.viewportY + y);
		const runs: StyledRun[] = [];
		if (!line) { rowRuns.push(runs); continue; }
		let current: StyledRun | null = null;
		for (let x = 0; x < cols; x++) {
			const cell = line.getCell(x);
			if (!cell) continue;
			const chars = cell.getChars() || ' ';
			const width = cell.getWidth();
			if (width === 0) continue; // continuation of a wide char
			let fg: string | null = null;
			let bg: string | null = null;
			if (cell.isFgRGB()) fg = `#${cell.getFgColor().toString(16).padStart(6, '0')}`;
			else if (cell.isFgPalette()) fg = paletteToHex(cell.getFgColor());
			if (cell.isBgRGB()) bg = `#${cell.getBgColor().toString(16).padStart(6, '0')}`;
			else if (cell.isBgPalette()) bg = paletteToHex(cell.getBgColor());
			const bold = !!cell.isBold();
			// Merge adjacent same-color bg cells into one span — per-cell rects
			// leave hairline gaps after SVG rasterization (striped highlights).
			if (bg) {
				const last = bgRects[bgRects.length - 1];
				if (last && last.y === y && last.color === bg && last.x + last.w === x) {
					last.w += width;
				} else {
					bgRects.push({ x, y, w: width, color: bg });
				}
			}
			if (current && current.fg === fg && current.bg === bg && current.bold === bold) {
				current.text += chars;
			} else {
				current = { text: chars, x, fg, bg, bold };
				runs.push(current);
			}
		}
		rowRuns.push(runs);
	}

	const W = cols * CELL_W;
	const H = rows * CELL_H;
	const parts: string[] = [
		`<svg xmlns="http://www.w3.org/2000/svg" width="${W}" height="${H}" font-family="Menlo, Consolas, monospace" font-size="14">`,
		`<rect width="${W}" height="${H}" fill="${DEFAULT_BG}"/>`,
	];
	// Background rects (merged per run would be nicer; per-cell is correct).
	for (const r of bgRects) {
		parts.push(`<rect x="${r.x * CELL_W}" y="${r.y * CELL_H}" width="${r.w * CELL_W}" height="${CELL_H}" fill="${r.color}"/>`);
	}
	// Text runs.
	for (let y = 0; y < rowRuns.length; y++) {
		for (const run of rowRuns[y]) {
			if (run.text.trim() === '') continue;
			const fill = run.fg ?? DEFAULT_FG;
			const weight = run.bold ? ' font-weight="bold"' : '';
			parts.push(
				`<text x="${run.x * CELL_W}" y="${y * CELL_H + 14}" fill="${fill}"${weight} xml:space="preserve" textLength="${run.text.length * CELL_W}">${escapeXml(run.text)}</text>`,
			);
		}
	}
	parts.push('</svg>');
	return parts.join('\n');
}

/** Writes a full-color SVG screenshot of the current buffer into `dir`. */
export function dumpScreenshot(term: TestTerminal, dir: string, name = 'screenshot'): void {
	writeFileSync(join(dir, `${name}.svg`), screenshotSvg(term));
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

	// Sending before start() is a caller mistake, not something a test should absorb quietly.
	sendInput(data: string) {
		if (!this.inputHandler) throw new Error('sendInput before start() attached an input handler');
		this.inputHandler(data);
	}
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

	/** Raw xterm buffer access for full-color screenshot export. */
	xtermBuffer() { return this.xterm.buffer.active; }

	/** The cell the terminal's own cursor is parked on, with its attributes. */
	getCursorCell(): { char: string; inverse: boolean; row: number; col: number } {
		const buf = this.xterm.buffer.active;
		const row = buf.baseY + buf.cursorY;
		const col = buf.cursorX;
		const cell = buf.getLine(row)?.getCell(col);
		return {
			char: cell?.getChars() || ' ',
			inverse: (cell?.isInverse() ?? 0) !== 0,
			row,
			col,
		};
	}
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

// --- Paint synchronization ---

/** Lets tests observe paints from a reconciler instance or a bare TUI through one shape. */
export interface PaintSignal {
	onRenderComplete(cb: () => void): () => void;
	paints(): number;
}

export function instancePaints(instance: Instance): PaintSignal {
	return {
		onRenderComplete: cb => instance.onRenderComplete(cb),
		paints: () => instance.getMetrics().renderCount,
	};
}

export function tuiPaints(tui: TUI): PaintSignal {
	return {
		onRenderComplete: cb => tui.onRenderComplete(cb),
		paints: () => tui.perfRenderCount,
	};
}

/** A paced paint lands on a frame-budget timer, so only its own event marks the arrival. */
export async function waitForPaints(signal: PaintSignal, target: number): Promise<void> {
	if (signal.paints() >= target) return;
	await new Promise<void>(resolve => {
		const unsubscribe = signal.onRenderComplete(() => {
			if (signal.paints() < target) return;
			unsubscribe();
			resolve();
		});
	});
}

/**
 * Resolves once the paint count stops moving, so the counter itself decides when
 * the work is over instead of a wall-clock guess that dilates under load. With no
 * paint to follow it only yields `quietTurns` times, which is not quiescence but is
 * what carries a mount past React's passive-effect flush — the turn an input
 * listener registers on. No paint marks that turn, so the counter cannot wait for
 * it; drop the mount-side call and the assertion runs before registration happens.
 */
export async function settlePaints(signal: PaintSignal, quietTurns = 2): Promise<number> {
	let quiet = 0;
	let last = signal.paints();
	while (quiet < quietTurns) {
		await new Promise(resolve => setImmediate(resolve));
		const current = signal.paints();
		// A single idle turn can sit between two paints of one input chain.
		quiet = current === last ? quiet + 1 : 0;
		last = current;
	}
	return last;
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
