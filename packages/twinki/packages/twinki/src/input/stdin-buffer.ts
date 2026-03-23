import { EventEmitter } from "node:events";

const ESC = "\x1b";
const BRACKETED_PASTE_START = "\x1b[200~";
const BRACKETED_PASTE_END = "\x1b[201~";



/**
 * Checks if a string is a complete escape sequence or needs more data.
 * 
 * Terminal input often arrives in chunks, and escape sequences may be
 * split across multiple chunks. This function determines whether we have
 * a complete sequence or need to wait for more data.
 * 
 * @param data - Input string to check
 * @returns Status indicating if sequence is complete, incomplete, or not an escape sequence
 */
function isCompleteSequence(data: string): "complete" | "incomplete" | "not-escape" {
	if (!data.startsWith(ESC)) {
		return "not-escape";
	}

	if (data.length === 1) {
		return "incomplete";
	}

	const afterEsc = data.slice(1);

	// CSI sequences: ESC [
	if (afterEsc.startsWith("[")) {
		// Check for old-style mouse sequence: ESC[M + 3 bytes
		if (afterEsc.startsWith("[M")) {
			return data.length >= 6 ? "complete" : "incomplete";
		}
		return isCompleteCsiSequence(data);
	}

	// OSC sequences: ESC ]
	if (afterEsc.startsWith("]")) {
		return isCompleteOscSequence(data);
	}

	// DCS sequences: ESC P
	if (afterEsc.startsWith("P")) {
		return isCompleteDcsSequence(data);
	}

	// APC sequences: ESC _
	if (afterEsc.startsWith("_")) {
		return isCompleteApcSequence(data);
	}

	// SS3 sequences: ESC O
	if (afterEsc.startsWith("O")) {
		return afterEsc.length >= 2 ? "complete" : "incomplete";
	}

	// Meta key sequences: ESC followed by a single character
	if (afterEsc.length === 1) {
		return "complete";
	}

	return "complete";
}

/**
 * Checks if a CSI (Control Sequence Introducer) sequence is complete.
 * 
 * CSI sequences start with ESC[ and end with a character in the range 0x40-0x7E.
 * Special handling is provided for SGR mouse sequences which have a specific format.
 * 
 * @param data - Input string starting with ESC[
 * @returns Whether the CSI sequence is complete or needs more data
 */
function isCompleteCsiSequence(data: string): "complete" | "incomplete" {
	if (!data.startsWith(`${ESC}[`)) {
		return "complete";
	}

	if (data.length < 3) {
		return "incomplete";
	}

	const payload = data.slice(2);
	const lastChar = payload[payload.length - 1];
	const lastCharCode = lastChar.charCodeAt(0);

	if (lastCharCode >= 0x40 && lastCharCode <= 0x7e) {
		// Special handling for SGR mouse sequences
		if (payload.startsWith("<")) {
			const mouseMatch = /^<\d+;\d+;\d+[Mm]$/.test(payload);
			if (mouseMatch) {
				return "complete";
			}
			if (lastChar === "M" || lastChar === "m") {
				const parts = payload.slice(1, -1).split(";");
				if (parts.length === 3 && parts.every((p) => /^\d+$/.test(p))) {
					return "complete";
				}
			}
			return "incomplete";
		}
		return "complete";
	}

	return "incomplete";
}

/**
 * Checks if an OSC (Operating System Command) sequence is complete.
 * 
 * OSC sequences start with ESC] and end with either ESC\ or BEL (0x07).
 * 
 * @param data - Input string starting with ESC]
 * @returns Whether the OSC sequence is complete or needs more data
 */
function isCompleteOscSequence(data: string): "complete" | "incomplete" {
	if (!data.startsWith(`${ESC}]`)) {
		return "complete";
	}

	if (data.endsWith(`${ESC}\\`) || data.endsWith("\x07")) {
		return "complete";
	}

	return "incomplete";
}

/**
 * Checks if a DCS (Device Control String) sequence is complete.
 * 
 * DCS sequences start with ESC P and end with ESC\.
 * 
 * @param data - Input string starting with ESC P
 * @returns Whether the DCS sequence is complete or needs more data
 */
function isCompleteDcsSequence(data: string): "complete" | "incomplete" {
	if (!data.startsWith(`${ESC}P`)) {
		return "complete";
	}

	if (data.endsWith(`${ESC}\\`)) {
		return "complete";
	}

	return "incomplete";
}

/**
 * Checks if an APC (Application Program Command) sequence is complete.
 * 
 * APC sequences start with ESC _ and end with ESC\.
 * 
 * @param data - Input string starting with ESC _
 * @returns Whether the APC sequence is complete or needs more data
 */
function isCompleteApcSequence(data: string): "complete" | "incomplete" {
	if (!data.startsWith(`${ESC}_`)) {
		return "complete";
	}

	if (data.endsWith(`${ESC}\\`)) {
		return "complete";
	}

	return "incomplete";
}

/**
 * Splits accumulated buffer into complete sequences and remainder.
 * 
 * Processes the buffer character by character, identifying complete
 * escape sequences and single characters. Incomplete sequences are
 * left in the remainder for future processing.
 * 
 * @param buffer - Accumulated input buffer
 * @returns Object containing complete sequences array and remainder string
 */
function extractCompleteSequences(buffer: string): { sequences: string[]; remainder: string } {
	const sequences: string[] = [];
	let pos = 0;

	while (pos < buffer.length) {
		const remaining = buffer.slice(pos);

		if (remaining.startsWith(ESC)) {
			let seqEnd = 1;
			while (seqEnd <= remaining.length) {
				const candidate = remaining.slice(0, seqEnd);
				const status = isCompleteSequence(candidate);

				if (status === "complete") {
					sequences.push(candidate);
					pos += seqEnd;
					break;
				} else if (status === "incomplete") {
					seqEnd++;
				} else {
					sequences.push(candidate);
					pos += seqEnd;
					break;
				}
			}

			if (seqEnd > remaining.length) {
				return { sequences, remainder: remaining };
			}
		} else {
			const ch = remaining.charCodeAt(0);
			if (ch >= 0x20) {
				// Batch consecutive printable characters into a single sequence,
				// matching Ink's behavior of passing whole chunks through.
				let end = 1;
				while (end < remaining.length && remaining.charCodeAt(end) >= 0x20 && remaining[end] !== ESC) {
					end++;
				}
				sequences.push(remaining.slice(0, end));
				pos += end;
			} else {
				// Control characters (0x00-0x1F except ESC) emitted individually
				sequences.push(remaining[0]!);
				pos++;
			}
		}
	}

	return { sequences, remainder: "" };
}

/**
 * Configuration options for StdinBuffer.
 */
export type StdinBufferOptions = {
	/**
	 * Maximum time to wait for sequence completion in milliseconds (default: 10ms).
	 * 
	 * When an incomplete escape sequence is detected, the buffer waits this long
	 * for additional data before giving up and emitting the partial sequence.
	 */
	timeout?: number;
};

/**
 * Event map for StdinBuffer events.
 */
export type StdinBufferEventMap = {
	/** Emitted when a complete input sequence is ready */
	data: [string];
	/** Emitted when bracketed paste content is received */
	paste: [string];
};

/**
 * Buffers stdin input and emits complete sequences via events.
 * 
 * This class handles the complexity of terminal input processing:
 * - Reassembles escape sequences that arrive in multiple chunks
 * - Detects and handles bracketed paste mode
 * - Converts high-byte characters to proper escape sequences
 * - Provides timeout-based fallback for incomplete sequences
 * 
 * The buffer is essential for reliable key detection, as terminal input
 * can arrive fragmented, especially over network connections or with
 * complex escape sequences.
 * 
 * @example
 * ```typescript
 * const buffer = new StdinBuffer({ timeout: 10 });
 * 
 * buffer.on('data', (sequence) => {
 *   console.log('Key sequence:', sequence);
 * });
 * 
 * buffer.on('paste', (content) => {
 *   console.log('Pasted:', content);
 * });
 * 
 * process.stdin.on('data', (data) => {
 *   buffer.process(data);
 * });
 * ```
 */
export class StdinBuffer extends EventEmitter<StdinBufferEventMap> {
	private buffer: string = "";
	private timeout: ReturnType<typeof setTimeout> | null = null;
	private readonly timeoutMs: number;
	private pasteMode: boolean = false;
	private pasteBuffer: string = "";

	/**
	 * Creates a new StdinBuffer instance.
	 * 
	 * @param options - Configuration options
	 */
	constructor(options: StdinBufferOptions = {}) {
		super();
		this.timeoutMs = options.timeout ?? 10;
	}

	/**
	 * Processes input data and emits complete sequences.
	 * 
	 * This method handles:
	 * - Buffer conversion and high-byte character translation
	 * - Bracketed paste mode detection and processing
	 * - Sequence completion detection with timeout fallback
	 * - Event emission for complete sequences and paste content
	 * 
	 * @param data - Input data from stdin (string or Buffer)
	 */
	public process(data: string | Buffer): void {
		if (this.timeout) {
			clearTimeout(this.timeout);
			this.timeout = null;
		}

		// Handle high-byte conversion
		let str: string;
		if (Buffer.isBuffer(data)) {
			if (data.length === 1 && data[0]! > 127) {
				const byte = data[0]! - 128;
				str = `\x1b${String.fromCharCode(byte)}`;
			} else {
				str = data.toString();
			}
		} else {
			str = data;
		}

		if (str.length === 0 && this.buffer.length === 0) {
			this.emit("data", "");
			return;
		}

		this.buffer += str;

		if (this.pasteMode) {
			this.pasteBuffer += this.buffer;
			this.buffer = "";

			const endIndex = this.pasteBuffer.indexOf(BRACKETED_PASTE_END);
			if (endIndex !== -1) {
				const pastedContent = this.pasteBuffer.slice(0, endIndex);
				const remaining = this.pasteBuffer.slice(endIndex + BRACKETED_PASTE_END.length);

				this.pasteMode = false;
				this.pasteBuffer = "";

				this.emit("paste", pastedContent);

				if (remaining.length > 0) {
					this.process(remaining);
				}
			}
			return;
		}

		const startIndex = this.buffer.indexOf(BRACKETED_PASTE_START);
		if (startIndex !== -1) {
			if (startIndex > 0) {
				const beforePaste = this.buffer.slice(0, startIndex);
				const result = extractCompleteSequences(beforePaste);
				for (const sequence of result.sequences) {
					this.emit("data", sequence);
				}
			}

			this.buffer = this.buffer.slice(startIndex + BRACKETED_PASTE_START.length);
			this.pasteMode = true;
			this.pasteBuffer = this.buffer;
			this.buffer = "";

			const endIndex = this.pasteBuffer.indexOf(BRACKETED_PASTE_END);
			if (endIndex !== -1) {
				const pastedContent = this.pasteBuffer.slice(0, endIndex);
				const remaining = this.pasteBuffer.slice(endIndex + BRACKETED_PASTE_END.length);

				this.pasteMode = false;
				this.pasteBuffer = "";

				this.emit("paste", pastedContent);

				if (remaining.length > 0) {
					this.process(remaining);
				}
			}
			return;
		}

		const result = extractCompleteSequences(this.buffer);
		this.buffer = result.remainder;

		for (const sequence of result.sequences) {
			this.emit("data", sequence);
		}

		if (this.buffer.length > 0) {
			this.timeout = setTimeout(() => {
				const flushed = this.flush();
				for (const sequence of flushed) {
					this.emit("data", sequence);
				}
			}, this.timeoutMs);
		}
	}

	/**
	 * Flushes any remaining buffer content as complete sequences.
	 * 
	 * This method is called when the timeout expires or when explicitly
	 * requested. It treats any buffered content as complete sequences.
	 * 
	 * @returns Array of flushed sequences
	 */
	flush(): string[] {
		if (this.timeout) {
			clearTimeout(this.timeout);
			this.timeout = null;
		}

		if (this.buffer.length === 0) {
			return [];
		}

		const sequences = [this.buffer];
		this.buffer = "";
		return sequences;
	}

	/**
	 * Clears all buffers and cancels pending timeouts.
	 * 
	 * Resets the buffer to a clean state, useful for testing
	 * or when input processing needs to be restarted.
	 */
	clear(): void {
		if (this.timeout) {
			clearTimeout(this.timeout);
			this.timeout = null;
		}
		this.buffer = "";
		this.pasteMode = false;
		this.pasteBuffer = "";
	}

	/**
	 * Gets the current buffer content for debugging purposes.
	 * 
	 * @returns Current buffer string
	 */
	getBuffer(): string {
		return this.buffer;
	}

	/**
	 * Destroys the buffer and cleans up all resources.
	 * 
	 * Should be called when the buffer is no longer needed
	 * to prevent memory leaks and ensure proper cleanup.
	 */
	destroy(): void {
		this.clear();
	}
}