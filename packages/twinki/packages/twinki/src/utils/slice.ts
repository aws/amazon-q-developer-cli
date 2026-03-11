import { visibleWidth, getSegmenter } from './visible-width.js';
import { extractAnsiCode } from './ansi.js';

/**
 * Calculates the terminal width of a single grapheme cluster.
 * 
 * Fast path for ASCII (width 1) avoids the expensive string-width call.
 * Only falls back to visibleWidth for non-ASCII graphemes (CJK, emoji, etc.).
 * 
 * @param segment - Grapheme cluster to measure
 * @returns Width in terminal columns
 */
function graphemeWidth(segment: string): number {
	// Fast path: single ASCII printable char = width 1
	if (segment.length === 1) {
		const code = segment.charCodeAt(0);
		if (code >= 0x20 && code <= 0x7e) return 1;
		if (code < 0x20) return 0; // control chars
	}
	return visibleWidth(segment);
}

/**
 * Extracts a range of visible columns from a line of text.
 * 
 * This function slices text based on visual column positions rather than
 * character positions, properly handling ANSI escape sequences and wide
 * characters. Essential for terminal text layout and cursor positioning.
 * 
 * @param line - Line to slice
 * @param startCol - Starting column (0-based)
 * @param length - Number of columns to extract
 * @param strict - If true, exclude wide chars that would extend past the range
 * @returns Sliced text with ANSI codes preserved
 * 
 * @example
 * ```typescript
 * sliceByColumn('Hello 世界', 0, 5); // 'Hello'
 * sliceByColumn('Hello 世界', 6, 2); // '世' (wide character)
 * ```
 */
export function sliceByColumn(line: string, startCol: number, length: number, strict = false): string {
	return sliceWithWidth(line, startCol, length, strict).text;
}

/**
 * Like sliceByColumn but also returns the actual visible width of the result.
 * 
 * Provides both the sliced text and its measured width, useful when you need
 * to know the exact visual dimensions of the extracted content for layout
 * calculations.
 * 
 * @param line - Line to slice
 * @param startCol - Starting column (0-based)
 * @param length - Number of columns to extract
 * @param strict - If true, exclude wide chars that would extend past the range
 * @returns Object with sliced text and its visible width
 */
export function sliceWithWidth(
	line: string,
	startCol: number,
	length: number,
	strict = false,
): { text: string; width: number } {
	if (length <= 0) return { text: "", width: 0 };
	
	const endCol = startCol + length;
	let result = "";
	let resultWidth = 0;
	let currentCol = 0;
	let i = 0;
	let pendingAnsi = "";

	const segmenter = getSegmenter();

	while (i < line.length) {
		const ansi = extractAnsiCode(line, i);
		if (ansi) {
			if (currentCol >= startCol && currentCol < endCol) {
				result += ansi.code;
			} else if (currentCol < startCol) {
				pendingAnsi += ansi.code;
			}
			i += ansi.length;
			continue;
		}

		// Scan ahead to find the next ANSI code or end of string
		let textEnd = i + 1;
		while (textEnd < line.length && line.charCodeAt(textEnd) !== 0x1b) {
			textEnd++;
		}

		for (const { segment } of segmenter.segment(line.slice(i, textEnd))) {
			const w = graphemeWidth(segment);
			const inRange = currentCol >= startCol && currentCol < endCol;
			const fits = !strict || currentCol + w <= endCol;
			
			if (inRange && fits) {
				if (pendingAnsi) {
					result += pendingAnsi;
					pendingAnsi = "";
				}
				result += segment;
				resultWidth += w;
			}
			
			currentCol += w;
			if (currentCol >= endCol) break;
		}
		
		i = textEnd;
		if (currentCol >= endCol) break;
	}
	
	return { text: result, width: resultWidth };
}

/**
 * Truncates text to fit within a maximum visible width.
 * 
 * This function intelligently truncates text while:
 * - Preserving ANSI escape sequences up to the truncation point
 * - Adding an ellipsis indicator when truncation occurs
 * - Optionally padding the result to a fixed width
 * - Properly handling wide characters and grapheme clusters
 * - Adding reset codes to prevent style bleeding into ellipsis
 * 
 * Essential for displaying text in fixed-width columns or when content
 * might exceed available space.
 * 
 * @param text - Text to truncate (may contain ANSI codes)
 * @param maxWidth - Maximum visible width
 * @param ellipsis - Ellipsis string to append when truncating (default: "...")
 * @param pad - If true, pad result with spaces to exactly maxWidth
 * @returns Truncated text, optionally padded
 * 
 * @example
 * ```typescript
 * truncateToWidth('Very long text here', 10); // 'Very lo...'
 * truncateToWidth('Short', 10, '...', true);  // 'Short     '
 * ```
 */
export function truncateToWidth(
	text: string,
	maxWidth: number,
	ellipsis: string = "...",
	pad: boolean = false,
): string {
	const textVisibleWidth = visibleWidth(text);

	if (textVisibleWidth <= maxWidth) {
		return pad ? text + " ".repeat(maxWidth - textVisibleWidth) : text;
	}

	const ellipsisWidth = visibleWidth(ellipsis);
	const targetWidth = maxWidth - ellipsisWidth;

	if (targetWidth <= 0) {
		return ellipsis.substring(0, maxWidth);
	}

	const segmenter = getSegmenter();
	let i = 0;
	const segments: Array<{ type: "ansi" | "grapheme"; value: string }> = [];

	while (i < text.length) {
		const ansiResult = extractAnsiCode(text, i);
		if (ansiResult) {
			segments.push({ type: "ansi", value: ansiResult.code });
			i += ansiResult.length;
		} else {
			let end = i;
			while (end < text.length) {
				const nextAnsi = extractAnsiCode(text, end);
				if (nextAnsi) break;
				end++;
			}
			const textPortion = text.slice(i, end);
			for (const seg of segmenter.segment(textPortion)) {
				segments.push({ type: "grapheme", value: seg.segment });
			}
			i = end;
		}
	}

	let result = "";
	let currentWidth = 0;

	for (const seg of segments) {
		if (seg.type === "ansi") {
			result += seg.value;
			continue;
		}

		const grapheme = seg.value;
		if (!grapheme) continue;

		const graphemeWidth = visibleWidth(grapheme);

		if (currentWidth + graphemeWidth > targetWidth) {
			break;
		}

		result += grapheme;
		currentWidth += graphemeWidth;
	}

	// Add reset code before ellipsis to prevent styling leaking
	const truncated = `${result}\x1b[0m${ellipsis}`;
	if (pad) {
		const truncatedWidth = visibleWidth(truncated);
		return truncated + " ".repeat(Math.max(0, maxWidth - truncatedWidth));
	}
	return truncated;
}