import { visibleWidth, getSegmenter } from './visible-width.js';
import { extractAnsiCode, AnsiCodeTracker } from './ansi.js';

/**
 * Updates tracker state from text containing ANSI codes.
 * 
 * Scans through text and processes any ANSI escape sequences
 * to keep the tracker's state synchronized with the text formatting.
 * 
 * @param text - Text that may contain ANSI codes
 * @param tracker - AnsiCodeTracker to update
 */
function updateTrackerFromText(text: string, tracker: AnsiCodeTracker): void {
	let i = 0;
	while (i < text.length) {
		const ansiResult = extractAnsiCode(text, i);
		if (ansiResult) {
			tracker.process(ansiResult.code);
			i += ansiResult.length;
		} else {
			i++;
		}
	}
}

/**
 * Splits text into tokens while keeping ANSI codes attached to adjacent content.
 * 
 * This function tokenizes text by whitespace boundaries while ensuring that
 * ANSI escape sequences remain attached to the content they modify. This is
 * crucial for proper text wrapping where formatting must be preserved.
 * 
 * @param text - Text to tokenize
 * @returns Array of tokens with ANSI codes attached
 */
function splitIntoTokensWithAnsi(text: string): string[] {
	const tokens: string[] = [];
	let current = "";
	let pendingAnsi = "";
	let inWhitespace = false;
	let i = 0;

	while (i < text.length) {
		const ansiResult = extractAnsiCode(text, i);
		if (ansiResult) {
			pendingAnsi += ansiResult.code;
			i += ansiResult.length;
			continue;
		}

		const char = text[i];
		const charIsSpace = char === " ";

		if (charIsSpace !== inWhitespace && current) {
			tokens.push(current);
			current = "";
		}

		if (pendingAnsi) {
			current += pendingAnsi;
			pendingAnsi = "";
		}

		inWhitespace = charIsSpace;
		current += char;
		i++;
	}

	if (pendingAnsi) {
		current += pendingAnsi;
	}

	if (current) {
		tokens.push(current);
	}

	return tokens;
}

/**
 * Breaks a long word character by character while preserving ANSI codes.
 * 
 * When a word is too long to fit on a line, this function splits it at
 * grapheme cluster boundaries (user-perceived characters) while maintaining
 * proper ANSI code continuity across line breaks.
 * 
 * @param word - Word to break
 * @param width - Maximum width per line
 * @param tracker - AnsiCodeTracker for state management
 * @returns Array of broken word segments
 */
function breakLongWord(word: string, width: number, tracker: AnsiCodeTracker): string[] {
	const lines: string[] = [];
	let currentLine = tracker.getActiveCodes();
	let currentWidth = 0;

	const segmenter = getSegmenter();
	let i = 0;
	const segments: Array<{ type: "ansi" | "grapheme"; value: string }> = [];

	while (i < word.length) {
		const ansiResult = extractAnsiCode(word, i);
		if (ansiResult) {
			segments.push({ type: "ansi", value: ansiResult.code });
			i += ansiResult.length;
		} else {
			let end = i;
			while (end < word.length) {
				const nextAnsi = extractAnsiCode(word, end);
				if (nextAnsi) break;
				end++;
			}
			const textPortion = word.slice(i, end);
			for (const seg of segmenter.segment(textPortion)) {
				segments.push({ type: "grapheme", value: seg.segment });
			}
			i = end;
		}
	}

	for (const seg of segments) {
		if (seg.type === "ansi") {
			currentLine += seg.value;
			tracker.process(seg.value);
			continue;
		}

		const grapheme = seg.value;
		if (!grapheme) continue;

		const graphemeWidth = visibleWidth(grapheme);

		if (currentWidth + graphemeWidth > width) {
			const lineEndReset = tracker.getLineEndReset();
			if (lineEndReset) {
				currentLine += lineEndReset;
			}
			lines.push(currentLine);
			currentLine = tracker.getActiveCodes();
			currentWidth = 0;
		}

		currentLine += grapheme;
		currentWidth += graphemeWidth;
	}

	if (currentLine) {
		lines.push(currentLine);
	}

	return lines.length > 0 ? lines : [""];
}

/**
 * Wraps a single line of text while preserving ANSI formatting.
 * 
 * This function handles the core text wrapping logic for a single line:
 * - Tokenizes text by whitespace boundaries
 * - Handles long words by breaking them when necessary
 * - Maintains ANSI code continuity across line breaks
 * - Properly manages whitespace at line boundaries
 * 
 * @param line - Single line of text to wrap
 * @param width - Maximum visible width per line
 * @returns Array of wrapped lines
 */
function wrapSingleLine(line: string, width: number): string[] {
	if (!line) {
		return [""];
	}

	const visibleLength = visibleWidth(line);
	if (visibleLength <= width) {
		return [line];
	}

	const wrapped: string[] = [];
	const tracker = new AnsiCodeTracker();
	const tokens = splitIntoTokensWithAnsi(line);

	let currentLine = "";
	let currentVisibleLength = 0;

	for (const token of tokens) {
		const tokenVisibleLength = visibleWidth(token);
		const isWhitespace = token.trim() === "";

		// Token itself is too long - break it
		if (tokenVisibleLength > width && !isWhitespace) {
			if (currentLine) {
				const lineEndReset = tracker.getLineEndReset();
				if (lineEndReset) {
					currentLine += lineEndReset;
				}
				wrapped.push(currentLine);
				currentLine = "";
				currentVisibleLength = 0;
			}

			const broken = breakLongWord(token, width, tracker);
			wrapped.push(...broken.slice(0, -1));
			currentLine = broken[broken.length - 1];
			currentVisibleLength = visibleWidth(currentLine);
			continue;
		}

		const totalNeeded = currentVisibleLength + tokenVisibleLength;

		if (totalNeeded > width && currentVisibleLength > 0) {
			let lineToWrap = currentLine.trimEnd();
			const lineEndReset = tracker.getLineEndReset();
			if (lineEndReset) {
				lineToWrap += lineEndReset;
			}
			wrapped.push(lineToWrap);
			if (isWhitespace) {
				currentLine = tracker.getActiveCodes();
				currentVisibleLength = 0;
			} else {
				currentLine = tracker.getActiveCodes() + token;
				currentVisibleLength = tokenVisibleLength;
			}
		} else {
			currentLine += token;
			currentVisibleLength += tokenVisibleLength;
		}

		updateTrackerFromText(token, tracker);
	}

	if (currentLine) {
		wrapped.push(currentLine);
	}

	return wrapped.length > 0 ? wrapped.map((line) => line.trimEnd()) : [""];
}

/**
 * Fast path for ASCII-only text wrapping without ANSI processing.
 * 
 * @param text - Plain ASCII text to wrap
 * @param width - Maximum width per line
 * @returns Array of wrapped lines
 */
function wrapAsciiText(text: string, width: number): string[] {
	if (!text) return [""];
	
	const inputLines = text.split("\n");
	const result: string[] = [];
	
	for (const line of inputLines) {
		if (line.length <= width) {
			result.push(line);
			continue;
		}
		
		// Simple word wrapping for ASCII text
		const words = line.split(' ');
		let currentLine = '';
		
		for (const word of words) {
			if (word.length > width) {
				// Break long words
				if (currentLine) {
					result.push(currentLine.trimEnd());
					currentLine = '';
				}
				for (let i = 0; i < word.length; i += width) {
					result.push(word.slice(i, i + width));
				}
			} else if (currentLine.length + word.length + 1 <= width) {
				currentLine += (currentLine ? ' ' : '') + word;
			} else {
				if (currentLine) {
					result.push(currentLine.trimEnd());
				}
				currentLine = word;
			}
		}
		
		if (currentLine) {
			result.push(currentLine.trimEnd());
		}
	}
	
	return result.length > 0 ? result : [""];
}

/**
 * Checks if text contains only ASCII printable characters and spaces.
 * 
 * @param text - Text to check
 * @returns True if text is ASCII-only
 */
function isAsciiOnly(text: string): boolean {
	for (let i = 0; i < text.length; i++) {
		const code = text.charCodeAt(i);
		// Allow printable ASCII (32-126) plus newline (10) and tab (9)
		if (!(code === 9 || code === 10 || (code >= 32 && code <= 126))) {
			return false;
		}
	}
	return true;
}

/**
 * Wraps text with ANSI codes preserved across line breaks.
 * 
 * This is the main text wrapping function that handles multi-line input
 * while preserving ANSI formatting. It ensures that:
 * - Text is wrapped at the specified width
 * - ANSI formatting is preserved and continued on wrapped lines
 * - Existing line breaks in the input are respected
 * - Proper formatting state is maintained across the entire text
 * 
 * The function is essential for terminal text layout where styled content
 * needs to be reflowed to fit different terminal widths.
 * 
 * @param text - Text to wrap (may contain ANSI codes and newlines)
 * @param width - Maximum visible width per line
 * @returns Array of wrapped lines with ANSI codes preserved
 * 
 * @example
 * ```typescript
 * const wrapped = wrapTextWithAnsi('\x1b[31mLong red text that needs wrapping\x1b[0m', 10);
 * // Each line will maintain the red color formatting
 * ```
 */
export function wrapTextWithAnsi(text: string, width: number): string[] {
	if (!text) {
		return [""];
	}

	// Fast path: ASCII-only text without ANSI codes
	if (isAsciiOnly(text)) {
		return wrapAsciiText(text, width);
	}

	const inputLines = text.split("\n");
	const result: string[] = [];
	const tracker = new AnsiCodeTracker();

	for (const inputLine of inputLines) {
		const prefix = result.length > 0 ? tracker.getActiveCodes() : "";
		result.push(...wrapSingleLine(prefix + inputLine, width));
		updateTrackerFromText(inputLine, tracker);
	}

	return result.length > 0 ? result : [""];
}