import pkg from '@xterm/headless';
const { Terminal: XtermTerminal } = pkg;
import type { Terminal } from 'twinki';

export class VirtualTerminal implements Terminal {
	private xterm: InstanceType<typeof XtermTerminal>;
	private inputHandler?: (data: string) => void;
	private resizeHandler?: () => void;
	private _columns: number;
	private _rows: number;

	constructor(columns = 80, rows = 24) {
		this._columns = columns;
		this._rows = rows;
		this.xterm = new XtermTerminal({
			cols: columns,
			rows,
			allowProposedApi: true,
		});
	}

	get kittyProtocolActive(): boolean {
		return true; // Virtual terminal assumes full protocol support
	}

	get columns(): number {
		return this._columns;
	}

	get rows(): number {
		return this._rows;
	}

	start(onInput: (data: string) => void, onResize: () => void): void {
		this.inputHandler = onInput;
		this.resizeHandler = onResize;
	}

	stop(): void {
		this.inputHandler = undefined;
		this.resizeHandler = undefined;
	}

	async drainInput(): Promise<void> {}

	write(data: string): void {
		this.xterm.write(data);
	}

	async flush(): Promise<void> {
		return new Promise((resolve) => this.xterm.write('', resolve));
	}

	sendInput(data: string): void {
		this.inputHandler?.(data);
	}

	resize(cols: number, rows: number): void {
		this._columns = cols;
		this._rows = rows;
		this.xterm.resize(cols, rows);
		this.resizeHandler?.();
	}

	getViewport(): string[] {
		const buf = this.xterm.buffer.active;
		const lines: string[] = [];
		const viewportY = buf.viewportY;
		for (let i = 0; i < this._rows; i++) {
			const line = buf.getLine(viewportY + i);
			lines.push(line ? line.translateToString(true) : '');
		}
		return lines;
	}

	getScrollBuffer(): string[] {
		const buf = this.xterm.buffer.active;
		const lines: string[] = [];
		for (let i = 0; i < buf.length; i++) {
			const line = buf.getLine(i);
			lines.push(line ? line.translateToString(true) : '');
		}
		return lines;
	}

	getCursorPosition(): { x: number; y: number } {
		const buf = this.xterm.buffer.active;
		return { x: buf.cursorX, y: buf.cursorY };
	}

	moveBy(lines: number): void {
		if (lines > 0) this.write(`\x1b[${lines}B`);
		else if (lines < 0) this.write(`\x1b[${-lines}A`);
	}

	hideCursor(): void {
		this.write('\x1b[?25l');
	}

	showCursor(): void {
		this.write('\x1b[?25h');
	}

	clearLine(): void {
		this.write('\x1b[K');
	}

	clearFromCursor(): void {
		this.write('\x1b[J');
	}

	clearScreen(): void {
		this.write('\x1b[2J\x1b[H');
	}

	setTitle(title: string): void {
		this.write(`\x1b]0;${title}\x07`);
	}

	enableMouse(): void {}
	disableMouse(): void {}

	dispose(): void {
		this.xterm.dispose();
	}
}
