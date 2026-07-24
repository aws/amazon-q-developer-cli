import * as fs from "node:fs";
import * as os from "node:os";
import * as path from "node:path";
import { isGhostty, isKitty } from "./capabilities.js";
import { setKittyProtocolActive } from "../input/keys.js";
import { StdinBuffer } from "../input/stdin-buffer.js";
import type { Terminal } from "./terminal.js";

/**
 * Resolves the render log file path.
 *
 * - KIRO_RENDER_LOG=1 enables logging to the default path: $TMPDIR/kiro-log/kiro-render.log
 * - KIRO_RENDER_LOG_FILE overrides the path (implies enabled)
 * - TWINKI_WRITE_LOG is supported as a legacy fallback
 */
function resolveRenderLogPath(): string {
	if (process.env.KIRO_RENDER_LOG_FILE) {
		return process.env.KIRO_RENDER_LOG_FILE;
	}
	if (process.env.KIRO_RENDER_LOG === "1") {
		const logsDir = path.join(os.tmpdir(), "kiro-log");
		if (!fs.existsSync(logsDir)) {
			fs.mkdirSync(logsDir, { recursive: true });
		}
		return path.join(logsDir, "kiro-render.log");
	}
	// Legacy fallback
	return process.env.TWINKI_WRITE_LOG || "";
}

/**
 * Kitty keyboard protocol flags.
 *
 * The Kitty keyboard protocol (https://sw.kovidgoyal.net/kitty/keyboard-protocol/)
 * enhances terminal key reporting so the application can distinguish keypresses
 * that legacy VT sequences cannot (e.g. Shift+Enter vs Enter, Ctrl+I vs Tab).
 *
 * Despite the name, this protocol is NOT specific to the Kitty terminal — it is
 * a cross-terminal standard also implemented by several others (see
 * KNOWN_KITTY_TERMINALS below). Terminals that don't support it silently ignore
 * the enable/disable sequences.
 *
 * Flags are a bitmask:
 *   1 = disambiguateEscapeCodes  — report modified keys as CSI u sequences
 *   2 = reportEventTypes         — include press/repeat/release event type
 *   4 = reportAlternateKeys      — include shifted and base-layout codepoints
 *
 * We use flag 1 only. Flag 2 (reportEventTypes) causes terminals to send
 * both press and release events which can confuse components that don't
 * expect them. Flag 4 is useful for international layouts but not required.
 */
const KITTY_FLAGS = 1;

/**
 * xterm modifyOtherKeys escape sequences.
 *
 * modifyOtherKeys (https://invisible-island.net/xterm/manpage/xterm.html#VT100-Widget-Resources:modifyOtherKeys)
 * is a widely supported xterm feature that reports modified keys using the
 * format `CSI 27 ; modifier ; keycode ~`. This lets the application
 * distinguish keypresses like Shift+Enter from plain Enter.
 *
 * Level 1 modifies only keys whose modifier would otherwise be lost (e.g.
 * Shift+Enter, which normally sends the same `\r` as Enter). Level 2
 * modifies all modified keys, which can break uppercase letter input in
 * parsers that only expect lowercase keycodes.
 *
 * We use level 1 as a fallback when the Kitty keyboard protocol is not
 * available. Supported by VTE ≥ 0.62 (GNOME Terminal), xterm, foot, and
 * many other terminals.
 */
const MODIFY_OTHER_KEYS_ENABLE = "\x1b[>4;1m";
const MODIFY_OTHER_KEYS_DISABLE = "\x1b[>4;0m";

/**
 * Terminals known to support the Kitty keyboard protocol.
 *
 * Some terminals (notably iTerm2) support the protocol but do NOT respond to
 * the standard query sequence `CSI ? u`. For these we must force-enable the
 * protocol without waiting for a response.
 *
 * Detection uses environment variables set by each terminal:
 *   - KITTY_WINDOW_ID          → Kitty
 *   - TERM = xterm-kitty       → Kitty
 *   - TERM_PROGRAM = WezTerm   → WezTerm
 *   - TERM_PROGRAM = ghostty   → Ghostty
 *   - TERM = xterm-ghostty     → Ghostty (over SSH)
 *   - TERM_PROGRAM = iTerm.app → iTerm2 (≥ 3.5, does NOT respond to query)
 */
const KNOWN_KITTY_TERMINALS: ReadonlyArray<(env: NodeJS.ProcessEnv) => boolean> = [
	() => isKitty(),
	(env) => env['TERM_PROGRAM'] === 'WezTerm',
	() => isGhostty(),
	(env) => env['TERM_PROGRAM'] === 'iTerm.app',
];

function isKnownKittyTerminal(): boolean {
	return KNOWN_KITTY_TERMINALS.some((check) => check(process.env));
}

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
	/**
	 * Number of entries this process has pushed onto the terminal's
	 * keyboard-mode stack. `CSI > flags u` is a stack PUSH, not an idempotent
	 * mode set: every unbalanced push leaves the protocol enabled in the
	 * parent shell after exit. All keyboard-protocol writes are gated on this
	 * counter so pushes and pops always balance, and we never pop entries a
	 * host process pushed below ours.
	 */
	private kittyPushDepth = 0;
	private _modifyOtherKeysActive = false;
	private _suspendedKitty = false;
	private _suspendedModify = false;
	private stdinBuffer?: StdinBuffer;
	private stdinDataHandler?: (data: string) => void;
	private writeLogPath = resolveRenderLogPath();
	private _columns = process.stdout.columns || 80;
	private _rows = process.stdout.rows || 24;

	/**
	 * Whether Kitty keyboard protocol is currently active.
	 * 
	 * The Kitty protocol provides enhanced key detection capabilities,
	 * allowing distinction between keys that would otherwise be ambiguous.
	 */
	get kittyProtocolActive(): boolean {
		return this.kittyPushDepth > 0;
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
			const newCols = process.stdout.columns || 80;
			const newRows = process.stdout.rows || 24;
			// Skip invalid dimensions (iTerm can report 0 during transitions)
			if (newCols < 1 || newRows < 1) return;
			// A same-geometry reattach (e.g. iTerm2 tmux -CC over SSH) fires a
			// resize with unchanged dims but resets the terminal's DEC private
			// modes, so re-assert the enables before the unchanged-dims skip.
			this.reassertModeEnables();
			// Skip if dimensions haven't changed (e.g. tmux pane focus, attach/detach)
			if (newCols === this._columns && newRows === this._rows) return;
			this._columns = newCols;
			this._rows = newRows;
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
			// Check for Kitty protocol query response (unknown terminal path)
			if (!this.kittyProtocolActive) {
				const match = sequence.match(kittyResponsePattern);
				if (match) {
					this.enableKittyProtocol();
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
	 * Detects and enables Kitty keyboard protocol.
	 *
	 * For terminals in {@link KNOWN_KITTY_TERMINALS}, the protocol is
	 * force-enabled immediately — these terminals support the protocol but
	 * may not respond to the standard `CSI ? u` query (e.g. iTerm2).
	 *
	 * For unknown terminals, sends the query and waits for a response via
	 * the StdinBuffer's data handler (see {@link setupStdinBuffer}).
	 */
	private queryAndEnableKittyProtocol(): void {
		this.setupStdinBuffer();
		process.stdin.on("data", this.stdinDataHandler!);

		if (isKnownKittyTerminal()) {
			this.enableKittyProtocol();
			return;
		}

		// Unknown terminal — query for Kitty protocol and enable modifyOtherKeys
		// as a fallback. modifyOtherKeys is widely supported (VTE, xterm, foot)
		// and lets us detect Shift+Enter even without Kitty protocol support.
		// If the terminal responds to the Kitty query, enableKittyProtocol()
		// will disable modifyOtherKeys since Kitty supersedes it.
		process.stdout.write("\x1b[?u");
		this.enableModifyOtherKeys();
	}

	/**
	 * Enables Kitty keyboard protocol with {@link KITTY_FLAGS}.
	 * Disables modifyOtherKeys since Kitty protocol supersedes it.
	 *
	 * `CSI > flags u` PUSHES an entry onto the terminal's keyboard-mode
	 * stack, so this is a no-op while our push is already outstanding —
	 * a second push would leak an entry the single pop at teardown never
	 * removes.
	 */
	private enableKittyProtocol(): void {
		if (this.kittyPushDepth === 0) {
			this.kittyPushDepth++;
			process.stdout.write(`\x1b[>${KITTY_FLAGS}u`);
		}
		setKittyProtocolActive(true);
		this.disableModifyOtherKeys();
	}

	/**
	 * Pops every keyboard-mode stack entry this process pushed and clears
	 * the shared parser flag. Pops exactly {@link kittyPushDepth} entries,
	 * never touching entries a host process pushed below ours. Safe to call
	 * repeatedly; subsequent calls write nothing.
	 */
	private popKittyProtocol(): void {
		while (this.kittyPushDepth > 0) {
			process.stdout.write("\x1b[<u");
			this.kittyPushDepth--;
		}
		setKittyProtocolActive(false);
	}

	/**
	 * Synchronously restores legacy keyboard reporting: pops our Kitty
	 * keyboard-mode stack entries and disables modifyOtherKeys. Idempotent
	 * and safe to call from a `process.on('exit')` handler as a last-resort
	 * teardown on exit paths that bypass {@link stop}.
	 */
	resetKeyboardModes(): void {
		this.popKittyProtocol();
		this.disableModifyOtherKeys();
	}

	/**
	 * Re-emits the currently-active terminal-mode enables without re-running
	 * detection. A same-geometry reattach (iTerm2 tmux -CC over SSH) resets the
	 * terminal's DEC private modes; this restores bracketed paste and whichever
	 * keyboard protocol was negotiated at startup (Kitty XOR modifyOtherKeys).
	 *
	 * The Kitty flags are restored with the SET form (`CSI = flags ; 1 u`),
	 * which mutates the current stack entry in place. The PUSH form would add
	 * an entry per resize that teardown's single pop never removes, leaving
	 * the protocol enabled in the parent shell after exit. Every sequence
	 * here is idempotent, so it is safe to call on every resize.
	 */
	private reassertModeEnables(): void {
		process.stdout.write("\x1b[?2004h");
		if (this.kittyProtocolActive) {
			process.stdout.write(`\x1b[=${KITTY_FLAGS};1u`);
		} else if (this._modifyOtherKeysActive) {
			process.stdout.write(MODIFY_OTHER_KEYS_ENABLE);
		}
	}

	/**
	 * Enables xterm modifyOtherKeys level 1.
	 */
	private enableModifyOtherKeys(): void {
		this._modifyOtherKeysActive = true;
		process.stdout.write(MODIFY_OTHER_KEYS_ENABLE);
	}

	/**
	 * Disables xterm modifyOtherKeys if active.
	 */
	private disableModifyOtherKeys(): void {
		if (this._modifyOtherKeysActive) {
			this._modifyOtherKeysActive = false;
			process.stdout.write(MODIFY_OTHER_KEYS_DISABLE);
		}
	}

	/**
	 * Temporarily disables enhanced keyboard reporting (Kitty keyboard
	 * protocol + xterm modifyOtherKeys) so control keys reach the parent
	 * shell as legacy bytes while the process is backgrounded (e.g. after
	 * Ctrl+Z / SIGTSTP). Remembers which modes were active so
	 * {@link resumeKeyboard} can restore the pre-suspend state, and keeps the
	 * shared `kittyProtocolActive` parser flag in sync with the terminal.
	 */
	suspendKeyboard(): void {
		this._suspendedKitty = this.kittyProtocolActive;
		this._suspendedModify = this._modifyOtherKeysActive;
		this.resetKeyboardModes();
	}

	/**
	 * Re-enables the enhanced keyboard modes that {@link suspendKeyboard}
	 * disabled, restoring the pre-suspend state. Reuses the same enable paths
	 * as startup so the terminal mode and the `kittyProtocolActive` parser
	 * flag are turned back on together.
	 */
	resumeKeyboard(): void {
		if (this._suspendedKitty) {
			this.enableKittyProtocol();
		} else if (this._suspendedModify) {
			this.enableModifyOtherKeys();
		}
		this._suspendedKitty = false;
		this._suspendedModify = false;
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
		// Restore legacy key reporting before draining so any keys typed
		// during the drain window arrive as legacy bytes.
		this.resetKeyboardModes();

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

		// Restore legacy keyboard reporting (Kitty protocol + modifyOtherKeys)
		this.resetKeyboardModes();

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
	 * If KIRO_RENDER_LOG=1 or KIRO_RENDER_LOG_FILE is set,
	 * all output is also logged to the render log file for debugging.
	 * 
	 * @param data - Data to write to terminal
	 */
	write(data: string): void {
		// Safety guard: strip internal APC cursor marker if it leaked into the
		// write buffer. Prevents terminals/multiplexers from echoing it back as
		// visible text (e.g. tmux showing the APC payload in the input line).
		// Only strips BEL-terminated APC (\x1b_...\x07); ST-terminated sequences
		// (\x1b_...\x1b\\) used by Kitty graphics are preserved.
		// Uses indexOf loop instead of regex to avoid polynomial backtracking.
		if (data.includes('\x1b_')) {
			let i = data.indexOf('\x1b_');
			while (i !== -1) {
				const end = data.indexOf('\x07', i + 2);
				if (end === -1) break;
				data = data.slice(0, i) + data.slice(end + 1);
				i = data.indexOf('\x1b_', i);
			}
		}
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