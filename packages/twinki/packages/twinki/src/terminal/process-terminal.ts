import * as fs from "node:fs";
import { createRequire } from "node:module";
import { setKittyProtocolActive } from "../input/keys.js";
import { StdinBuffer } from "../input/stdin-buffer.js";
import type { Terminal } from "./terminal.js";

const cjsRequire = createRequire(import.meta.url);

/**
 * Terminal implementation using Node.js process.stdin/stdout.
 * 
 * Provides a full-featured terminal interface with support for:
 * - Raw mode input handling
 * - Kitty keyboard protocol for enhanced key detection
 * - Bracketed paste mode
 * - Windows VT input support
 * - Input buffering and sequence parsing
 * 
 * This is the primary terminal implementation for production use.
 */
export class ProcessTerminal implements Terminal {
	private wasRaw = false;
	private inputHandler?: (data: string) => void;
	private resizeHandler?: () => void;
	private _kittyProtocolActive = false;
	private stdinBuffer?: StdinBuffer;
	private stdinDataHandler?: (data: string) => void;
	private writeLogPath = process.env.TWINKI_WRITE_LOG || "";
	private _columns = process.stdout.columns || 80;
	private _rows = process.stdout.rows || 24;

	/**
	 * Whether Kitty keyboard protocol is currently active.
	 * 
	 * The Kitty protocol provides enhanced key detection capabilities,
	 * allowing distinction between keys that would otherwise be ambiguous.
	 */
	get kittyProtocolActive(): boolean {
		return this._kittyProtocolActive;
	}

	/**
	 * Starts the terminal in raw mode and sets up input/resize handlers.
	 * 
	 * This method:
	 * - Enables raw mode for immediate key detection
	 * - Sets up bracketed paste mode
	 * - Queries for Kitty keyboard protocol support
	 * - Enables Windows VT input if on Windows
	 * - Sets up resize event handling
	 * 
	 * @param onInput - Callback for input data
	 * @param onResize - Callback for terminal resize events
	 */
	start(onInput: (data: string) => void, onResize: () => void): void {
		this.inputHandler = onInput;
		this.resizeHandler = () => {
			this._columns = process.stdout.columns || 80;
			this._rows = process.stdout.rows || 24;
			onResize();
		};

		// Save previous state and enable raw mode
		this.wasRaw = process.stdin.isRaw || false;
		if (process.stdin.setRawMode) {
			process.stdin.setRawMode(true);
		}
		process.stdin.setEncoding("utf8");
		process.stdin.resume();

		// Enable bracketed paste mode
		process.stdout.write("\x1b[?2004h");

		// Set up resize handler
		process.stdout.on("resize", this.resizeHandler);

		// Refresh terminal dimensions (Unix only)
		if (process.platform !== "win32") {
			process.kill(process.pid, "SIGWINCH");
		}

		// Enable Windows VT input
		this.enableWindowsVTInput();

		// Query and enable Kitty keyboard protocol
		this.queryAndEnableKittyProtocol();
	}

	/**
	 * Sets up StdinBuffer to split batched input into individual sequences.
	 * 
	 * The StdinBuffer handles:
	 * - Splitting batched input into individual key sequences
	 * - Detecting and handling Kitty protocol responses
	 * - Managing bracketed paste content
	 * - Timeout-based sequence completion
	 */
	private setupStdinBuffer(): void {
		this.stdinBuffer = new StdinBuffer({ timeout: 10 });

		// Kitty protocol response pattern
		const kittyResponsePattern = /^\x1b\[\?(\d+)u$/;

		// Forward individual sequences to the input handler
		this.stdinBuffer.on("data", (sequence) => {
			// Check for Kitty protocol response
			if (!this._kittyProtocolActive) {
				const match = sequence.match(kittyResponsePattern);
				if (match) {
					this._kittyProtocolActive = true;
					setKittyProtocolActive(true);

					// Enable Kitty keyboard protocol with flags 1+2+4
					process.stdout.write("\x1b[>7u");
					return; // Don't forward protocol response to TUI
				}
			}

			if (this.inputHandler) {
				this.inputHandler(sequence);
			}
		});

		// Re-wrap paste content with bracketed paste markers
		this.stdinBuffer.on("paste", (content) => {
			if (this.inputHandler) {
				this.inputHandler(`\x1b[200~${content}\x1b[201~`);
			}
		});

		// Handler that pipes stdin data through the buffer
		this.stdinDataHandler = (data: string) => {
			this.stdinBuffer!.process(data);
		};
	}

	/**
	 * Queries terminal for Kitty keyboard protocol support.
	 * 
	 * Sends a query sequence and waits for a response to determine
	 * if the terminal supports enhanced keyboard protocol.
	 * If supported, enables the protocol with flags 1+2+4.
	 */
	private queryAndEnableKittyProtocol(): void {
		this.setupStdinBuffer();
		process.stdin.on("data", this.stdinDataHandler!);
		process.stdout.write("\x1b[?u");
	}

	/**
	 * Enables Windows VT input support using native Windows API.
	 * 
	 * On Windows, this enables ENABLE_VIRTUAL_TERMINAL_INPUT flag
	 * to support ANSI escape sequences in console input.
	 * Uses koffi library for native API access if available.
	 */
	private enableWindowsVTInput(): void {
		if (process.platform !== "win32") return;
		try {
			const koffi = cjsRequire("koffi");
			const k32 = koffi.load("kernel32.dll");
			const GetStdHandle = k32.func("void* __stdcall GetStdHandle(int)");
			const GetConsoleMode = k32.func("bool __stdcall GetConsoleMode(void*, _Out_ uint32_t*)");
			const SetConsoleMode = k32.func("bool __stdcall SetConsoleMode(void*, uint32_t)");

			const STD_INPUT_HANDLE = -10;
			const ENABLE_VIRTUAL_TERMINAL_INPUT = 0x0200;
			const handle = GetStdHandle(STD_INPUT_HANDLE);
			const mode = new Uint32Array(1);
			GetConsoleMode(handle, mode);
			SetConsoleMode(handle, mode[0]! | ENABLE_VIRTUAL_TERMINAL_INPUT);
		} catch {
			// koffi not available
		}
	}

	/**
	 * Drains stdin before exiting to prevent key release events from
	 * leaking to the parent shell over slow connections.
	 * 
	 * This is critical for preventing phantom keystrokes in the parent
	 * shell when the TUI exits, especially over SSH or slow connections.
	 * 
	 * @param maxMs - Maximum time to drain in milliseconds (default: 1000)
	 * @param idleMs - Exit early if no input arrives within this time (default: 50)
	 */
	async drainInput(maxMs = 1000, idleMs = 50): Promise<void> {
		if (this._kittyProtocolActive) {
			// Disable Kitty keyboard protocol first
			process.stdout.write("\x1b[<u");
			this._kittyProtocolActive = false;
			setKittyProtocolActive(false);
		}

		const previousHandler = this.inputHandler;
		this.inputHandler = undefined;

		let lastDataTime = Date.now();
		const onData = () => {
			lastDataTime = Date.now();
		};

		process.stdin.on("data", onData);
		const endTime = Date.now() + maxMs;

		try {
			while (true) {
				const now = Date.now();
				const timeLeft = endTime - now;
				if (timeLeft <= 0) break;
				if (now - lastDataTime >= idleMs) break;
				await new Promise((resolve) => setTimeout(resolve, Math.min(idleMs, timeLeft)));
			}
		} finally {
			process.stdin.removeListener("data", onData);
			this.inputHandler = previousHandler;
		}
	}

	/**
	 * Stops the terminal and restores previous state.
	 * 
	 * This method:
	 * - Disables bracketed paste mode
	 * - Disables Kitty keyboard protocol
	 * - Cleans up StdinBuffer and event handlers
	 * - Pauses stdin to prevent buffered input leakage
	 * - Restores raw mode state
	 */
	stop(): void {
		// Disable bracketed paste mode
		process.stdout.write("\x1b[?2004l");

		// Disable Kitty keyboard protocol
		if (this._kittyProtocolActive) {
			process.stdout.write("\x1b[<u");
			this._kittyProtocolActive = false;
			setKittyProtocolActive(false);
		}

		// Clean up StdinBuffer
		if (this.stdinBuffer) {
			this.stdinBuffer.destroy();
			this.stdinBuffer = undefined;
		}

		// Remove event handlers
		if (this.stdinDataHandler) {
			process.stdin.removeListener("data", this.stdinDataHandler);
			this.stdinDataHandler = undefined;
		}
		this.inputHandler = undefined;
		if (this.resizeHandler) {
			process.stdout.removeListener("resize", this.resizeHandler);
			this.resizeHandler = undefined;
		}

		// Pause stdin to prevent buffered input from being re-interpreted
		process.stdin.pause();

		// Restore raw mode state
		if (process.stdin.setRawMode) {
			process.stdin.setRawMode(this.wasRaw);
		}
	}

	/**
	 * Writes data to stdout with optional logging.
	 * 
	 * If TWINKI_WRITE_LOG environment variable is set,
	 * all output is also logged to the specified file for debugging.
	 * 
	 * @param data - Data to write to terminal
	 */
	write(data: string): void {
		process.stdout.write(data);
		if (this.writeLogPath) {
			try {
				fs.appendFileSync(this.writeLogPath, data, { encoding: "utf8" });
			} catch {
				// Ignore logging errors
			}
		}
	}

	/**
	 * Gets the terminal width in columns.
	 * Falls back to 80 if unable to determine.
	 * Cached and updated on resize to avoid repeated syscalls.
	 */
	get columns(): number {
		return this._columns;
	}

	/**
	 * Gets the terminal height in rows.
	 * Falls back to 24 if unable to determine.
	 * Cached and updated on resize to avoid repeated syscalls.
	 */
	get rows(): number {
		return this._rows;
	}

	/**
	 * Moves cursor up (negative) or down (positive) by specified lines.
	 * 
	 * @param lines - Number of lines to move (negative = up, positive = down)
	 */
	moveBy(lines: number): void {
		if (lines > 0) {
			process.stdout.write(`\x1b[${lines}B`);
		} else if (lines < 0) {
			process.stdout.write(`\x1b[${-lines}A`);
		}
	}

	/**
	 * Hides the terminal cursor.
	 */
	hideCursor(): void {
		process.stdout.write("\x1b[?25l");
	}

	/**
	 * Shows the terminal cursor.
	 */
	showCursor(): void {
		process.stdout.write("\x1b[?25h");
	}

	/**
	 * Clears the current line from cursor to end.
	 */
	clearLine(): void {
		process.stdout.write("\x1b[K");
	}

	/**
	 * Clears from cursor position to end of screen.
	 */
	clearFromCursor(): void {
		process.stdout.write("\x1b[J");
	}

	/**
	 * Clears entire screen and moves cursor to top-left (0,0).
	 */
	clearScreen(): void {
		process.stdout.write("\x1b[2J\x1b[H");
	}

	/**
	 * Sets the terminal window title.
	 * 
	 * @param title - The title to set
	 */
	setTitle(title: string): void {
		process.stdout.write(`\x1b]0;${title}\x07`);
	}

	enableMouse(): void {
		// 1000=button tracking, 1003=any-event tracking (motion), 1006=SGR encoding
		process.stdout.write('\x1b[?1000h\x1b[?1003h\x1b[?1006h');
	}

	disableMouse(): void {
		process.stdout.write('\x1b[?1006l\x1b[?1003l\x1b[?1000l');
	}
}