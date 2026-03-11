/**
 * Extracts ANSI escape sequences from a string at the given position.
 * 
 * Recognizes and extracts various ANSI sequence types:
 * - CSI sequences (ESC [ ... m/G/K/H/J) for colors and cursor control
 * - OSC sequences (ESC ] ... BEL/ST) for operating system commands
 * - APC sequences (ESC _ ... BEL/ST) for application program commands
 * 
 * @param str - String to extract from
 * @param pos - Position to start extraction
 * @returns Object with extracted code and length, or null if none found
 */
export function extractAnsiCode(str: string, pos: number): { code: string; length: number } | null {
	if (pos >= str.length || str[pos] !== "\x1b") return null;

	const next = str[pos + 1];

	// CSI sequence: ESC [ ... m/G/K/H/J
	if (next === "[") {
		let j = pos + 2;
		while (j < str.length && !/[mGKHJ]/.test(str[j]!)) j++;
		if (j < str.length) return { code: str.substring(pos, j + 1), length: j + 1 - pos };
		return null;
	}

	// OSC sequence: ESC ] ... BEL or ESC ] ... ST (ESC \)
	if (next === "]") {
		let j = pos + 2;
		while (j < str.length) {
			if (str[j] === "\x07") return { code: str.substring(pos, j + 1), length: j + 1 - pos };
			if (str[j] === "\x1b" && str[j + 1] === "\\") return { code: str.substring(pos, j + 2), length: j + 2 - pos };
			j++;
		}
		return null;
	}

	// APC sequence: ESC _ ... BEL or ESC _ ... ST (ESC \)
	if (next === "_") {
		let j = pos + 2;
		while (j < str.length) {
			if (str[j] === "\x07") return { code: str.substring(pos, j + 1), length: j + 1 - pos };
			if (str[j] === "\x1b" && str[j + 1] === "\\") return { code: str.substring(pos, j + 2), length: j + 2 - pos };
			j++;
		}
		return null;
	}

	return null;
}

/**
 * Tracks active ANSI SGR (Select Graphic Rendition) codes to preserve styling across line breaks.
 * 
 * This class maintains the current state of text formatting attributes like colors,
 * bold, italic, etc. It's essential for proper text wrapping and layout where
 * styling needs to be preserved when content spans multiple lines.
 * 
 * The tracker handles:
 * - Basic attributes (bold, italic, underline, etc.)
 * - 8-bit and 24-bit color codes
 * - Proper reset and state management
 * - Generation of continuation codes for wrapped lines
 * 
 * @example
 * ```typescript
 * const tracker = new AnsiCodeTracker();
 * tracker.process('\x1b[1;31m'); // bold red
 * console.log(tracker.getActiveCodes()); // '\x1b[1;31m'
 * ```
 */
export class AnsiCodeTracker {
	private bold = false;
	private dim = false;
	private italic = false;
	private underline = false;
	private blink = false;
	private inverse = false;
	private hidden = false;
	private strikethrough = false;
	private fgColor: string | null = null;
	private bgColor: string | null = null;

	/**
	 * Processes an ANSI SGR code and updates internal state.
	 * 
	 * Parses SGR escape sequences and updates the tracker's state accordingly.
	 * Handles complex color codes including 256-color and RGB formats.
	 * 
	 * @param ansiCode - ANSI escape sequence to process
	 */
	process(ansiCode: string): void {
		if (!ansiCode.endsWith("m")) {
			return;
		}

		const match = ansiCode.match(/\x1b\[([\d;]*)m/);
		if (!match) return;

		const params = match[1];
		if (params === "" || params === "0") {
			this.reset();
			return;
		}

		const parts = params.split(";");
		let i = 0;
		while (i < parts.length) {
			const code = Number.parseInt(parts[i], 10);

			// Handle 256-color and RGB codes
			if (code === 38 || code === 48) {
				if (parts[i + 1] === "5" && parts[i + 2] !== undefined) {
					// 256 color
					const colorCode = `${parts[i]};${parts[i + 1]};${parts[i + 2]}`;
					if (code === 38) {
						this.fgColor = colorCode;
					} else {
						this.bgColor = colorCode;
					}
					i += 3;
					continue;
				} else if (parts[i + 1] === "2" && parts[i + 4] !== undefined) {
					// RGB color
					const colorCode = `${parts[i]};${parts[i + 1]};${parts[i + 2]};${parts[i + 3]};${parts[i + 4]}`;
					if (code === 38) {
						this.fgColor = colorCode;
					} else {
						this.bgColor = colorCode;
					}
					i += 5;
					continue;
				}
			}

			// Standard SGR codes
			switch (code) {
				case 0: this.reset(); break;
				case 1: this.bold = true; break;
				case 2: this.dim = true; break;
				case 3: this.italic = true; break;
				case 4: this.underline = true; break;
				case 5: this.blink = true; break;
				case 7: this.inverse = true; break;
				case 8: this.hidden = true; break;
				case 9: this.strikethrough = true; break;
				case 21: this.bold = false; break;
				case 22: this.bold = false; this.dim = false; break;
				case 23: this.italic = false; break;
				case 24: this.underline = false; break;
				case 25: this.blink = false; break;
				case 27: this.inverse = false; break;
				case 28: this.hidden = false; break;
				case 29: this.strikethrough = false; break;
				case 39: this.fgColor = null; break;
				case 49: this.bgColor = null; break;
				default:
					// Standard foreground colors
					if ((code >= 30 && code <= 37) || (code >= 90 && code <= 97)) {
						this.fgColor = String(code);
					}
					// Standard background colors
					else if ((code >= 40 && code <= 47) || (code >= 100 && code <= 107)) {
						this.bgColor = String(code);
					}
					break;
			}
			i++;
		}
	}

	/**
	 * Resets all formatting attributes to their default state.
	 */
	private reset(): void {
		this.bold = false;
		this.dim = false;
		this.italic = false;
		this.underline = false;
		this.blink = false;
		this.inverse = false;
		this.hidden = false;
		this.strikethrough = false;
		this.fgColor = null;
		this.bgColor = null;
	}

	/**
	 * Clears all state for reuse of the tracker instance.
	 */
	clear(): void {
		this.reset();
	}

	/**
	 * Generates ANSI codes for currently active attributes.
	 * 
	 * Creates an escape sequence that would reproduce the current
	 * formatting state. Used for continuing styles on wrapped lines.
	 * 
	 * @returns ANSI escape sequence or empty string if no attributes active
	 */
	getActiveCodes(): string {
		const codes: string[] = [];
		if (this.bold) codes.push("1");
		if (this.dim) codes.push("2");
		if (this.italic) codes.push("3");
		if (this.underline) codes.push("4");
		if (this.blink) codes.push("5");
		if (this.inverse) codes.push("7");
		if (this.hidden) codes.push("8");
		if (this.strikethrough) codes.push("9");
		if (this.fgColor) codes.push(this.fgColor);
		if (this.bgColor) codes.push(this.bgColor);

		if (codes.length === 0) return "";
		return `\x1b[${codes.join(";")}m`;
	}

	/**
	 * Checks if any formatting attributes are currently active.
	 * 
	 * @returns Whether any attributes are active
	 */
	hasActiveCodes(): boolean {
		return (
			this.bold ||
			this.dim ||
			this.italic ||
			this.underline ||
			this.blink ||
			this.inverse ||
			this.hidden ||
			this.strikethrough ||
			this.fgColor !== null ||
			this.bgColor !== null
		);
	}

	/**
	 * Gets reset codes for attributes that visually bleed into padding.
	 * 
	 * Some attributes like underline extend beyond the text content
	 * and need to be reset at line boundaries to prevent visual artifacts.
	 * 
	 * @returns Reset sequence or empty string if no bleeding attributes
	 */
	getLineEndReset(): string {
		// Only underline causes visual bleeding into padding
		if (this.underline) {
			return "\x1b[24m";
		}
		return "";
	}
}