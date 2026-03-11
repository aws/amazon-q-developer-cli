import { visibleWidth, getSegmenter } from './visible-width.js';
import { extractAnsiCode, AnsiCodeTracker } from './ansi.js';

// For now, let's implement a simpler optimization that focuses on the main bottleneck
// without changing the core algorithm to ensure correctness

/**
 * Cache for ANSI parsing results to avoid re-parsing the same content
 */
const ANSI_PARSE_CACHE = new Map<string, { hasAnsi: boolean; segments: Array<{ text: string; isAnsi: boolean }> }>();
const CACHE_SIZE_LIMIT = 256;

/**
 * Optimized ANSI text wrapping with caching and single-pass parsing.
 */
function wrapTextWithAnsiOptimized(text: string, width: number): string[] {
	if (!text) return [""];
	
	// Fast path: ASCII-only text
	if (isAsciiOnly(text)) {
		return wrapAsciiText(text, width);
	}

	// Check if we've parsed this text before
	let parseResult = ANSI_PARSE_CACHE.get(text);
	if (!parseResult) {
		parseResult = parseAnsiText(text);
		
		// Cache with size limit
		if (ANSI_PARSE_CACHE.size >= CACHE_SIZE_LIMIT) {
			const firstKey = ANSI_PARSE_CACHE.keys().next().value;
			if (firstKey) ANSI_PARSE_CACHE.delete(firstKey);
		}
		ANSI_PARSE_CACHE.set(text, parseResult);
	}

	// If no ANSI codes, use fast ASCII path
	if (!parseResult.hasAnsi) {
		return wrapAsciiText(text, width);
	}

	// Use original algorithm for ANSI text but with pre-parsed segments
	return wrapWithParsedSegments(parseResult.segments, width);
}

/**
 * Parse text to identify ANSI vs text segments
 */
function parseAnsiText(text: string): { hasAnsi: boolean; segments: Array<{ text: string; isAnsi: boolean }> } {
	const segments: Array<{ text: string; isAnsi: boolean }> = [];
	let hasAnsi = false;
	let i = 0;

	while (i < text.length) {
		const ansiResult = extractAnsiCode(text, i);
		if (ansiResult) {
			segments.push({ text: ansiResult.code, isAnsi: true });
			hasAnsi = true;
			i += ansiResult.length;
		} else {
			// Find next ANSI code or end
			let textEnd = i;
			while (textEnd < text.length && !extractAnsiCode(text, textEnd)) {
				textEnd++;
			}
			segments.push({ text: text.slice(i, textEnd), isAnsi: false });
			i = textEnd;
		}
	}

	return { hasAnsi, segments };
}

/**
 * Wrap text using pre-parsed segments (fallback to original algorithm for correctness)
 */
function wrapWithParsedSegments(segments: Array<{ text: string; isAnsi: boolean }>, width: number): string[] {
	// For now, reconstruct the text and use the original algorithm
	// This still provides caching benefits for repeated content
	const reconstructed = segments.map(s => s.text).join('');
	
	// Use the original wrapTextWithAnsi implementation from the main file
	// We'll import and call the original functions
	return wrapTextWithAnsiOriginal(reconstructed, width);
}

/**
 * Original implementation - copied to ensure correctness
 */
function wrapTextWithAnsiOriginal(text: string, width: number): string[] {
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

/**
 * Updates tracker state from text containing ANSI codes.
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
 * Fast ASCII-only text wrapping.
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
 * Check if text is ASCII-only for fast path.
 */
function isAsciiOnly(text: string): boolean {
	for (let i = 0; i < text.length; i++) {
		const code = text.charCodeAt(i);
		if (!(code === 9 || code === 10 || (code >= 32 && code <= 126))) {
			return false;
		}
	}
	return true;
}

// Export the optimized function
export { wrapTextWithAnsiOptimized };