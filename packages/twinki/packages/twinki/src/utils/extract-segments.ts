import { visibleWidth, getSegmenter } from './visible-width.js';
import { extractAnsiCode, AnsiCodeTracker } from './ansi.js';

/**
 * Calculates the terminal width of a single grapheme cluster.
 * 
 * Wrapper around visibleWidth for consistency within segment extraction.
 * 
 * @param segment - Grapheme cluster to measure
 * @returns Width in terminal columns
 */
function graphemeWidth(segment: string): number {
	return visibleWidth(segment);
}

// Pooled tracker instance for extractSegments (avoids allocation per call)
const pooledStyleTracker = new AnsiCodeTracker();

/**
 * Extracts "before" and "after" segments from a line in a single pass.
 * 
 * This function is optimized for overlay compositing where content needs to be
 * extracted from both sides of an overlay region. It efficiently processes the
 * line once while:
 * - Extracting content before the overlay (beforeEnd columns)
 * - Extracting content after the overlay (afterStart to afterStart+afterLen)
 * - Preserving ANSI styling that should affect the "after" segment
 * - Handling wide characters and grapheme clusters correctly
 * 
 * The styling preservation is crucial for overlays - if text before the overlay
 * has formatting (like color), that formatting should continue in the "after"
 * segment unless explicitly reset.
 * 
 * @param line - Source line to extract from
 * @param beforeEnd - End column of "before" segment (exclusive)
 * @param afterStart - Start column of "after" segment (inclusive)
 * @param afterLen - Length of "after" segment in columns
 * @param strictAfter - If true, exclude wide chars that would extend past afterLen
 * @returns Object with before/after segments and their measured widths
 * 
 * @example
 * ```typescript
 * // Extract columns 0-5 and 10-15 from a line
 * const result = extractSegments('Hello world test', 5, 10, 5);
 * // result.before = 'Hello', result.after = ' test'
 * ```
 */
export function extractSegments(
	line: string,
	beforeEnd: number,
	afterStart: number,
	afterLen: number,
	strictAfter = false,
): { before: string; beforeWidth: number; after: string; afterWidth: number } {
	let before = "";
	let beforeWidth = 0;
	let after = "";
	let afterWidth = 0;
	let currentCol = 0;
	let i = 0;
	let pendingAnsiBefore = "";
	let afterStarted = false;
	const afterEnd = afterStart + afterLen;

	// Track styling state so "after" inherits styling from before the overlay
	pooledStyleTracker.clear();

	const segmenter = getSegmenter();

	while (i < line.length) {
		const ansi = extractAnsiCode(line, i);
		if (ansi) {
			// Track all SGR codes to know styling state at afterStart
			pooledStyleTracker.process(ansi.code);
			// Include ANSI codes in their respective segments
			if (currentCol < beforeEnd) {
				pendingAnsiBefore += ansi.code;
			} else if (currentCol >= afterStart && currentCol < afterEnd && afterStarted) {
				// Only include after we've started "after" (styling already prepended)
				after += ansi.code;
			}
			i += ansi.length;
			continue;
		}

		let textEnd = i;
		while (textEnd < line.length && !extractAnsiCode(line, textEnd)) {
			textEnd++;
		}

		for (const { segment } of segmenter.segment(line.slice(i, textEnd))) {
			const w = graphemeWidth(segment);

			if (currentCol < beforeEnd) {
				if (pendingAnsiBefore) {
					before += pendingAnsiBefore;
					pendingAnsiBefore = "";
				}
				before += segment;
				beforeWidth += w;
			} else if (currentCol >= afterStart && currentCol < afterEnd) {
				const fits = !strictAfter || currentCol + w <= afterEnd;
				if (fits) {
					// On first "after" grapheme, prepend inherited styling from before overlay
					if (!afterStarted) {
						after += pooledStyleTracker.getActiveCodes();
						afterStarted = true;
					}
					after += segment;
					afterWidth += w;
				}
			}

			currentCol += w;
			// Early exit: done with "before" only, or done with both segments
			if (afterLen <= 0 ? currentCol >= beforeEnd : currentCol >= afterEnd) break;
		}
		
		i = textEnd;
		if (afterLen <= 0 ? currentCol >= beforeEnd : currentCol >= afterEnd) break;
	}

	return { before, beforeWidth, after, afterWidth };
}