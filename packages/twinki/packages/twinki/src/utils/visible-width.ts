import stringWidth from "string-width";

// Shared grapheme segmenter instance
const segmenter = new Intl.Segmenter(undefined, { granularity: "grapheme" });

/**
 * Gets the shared grapheme segmenter instance.
 * 
 * The segmenter is used to properly split text into grapheme clusters
 * (user-perceived characters) which is essential for accurate width
 * calculation of complex Unicode text including emoji and combining characters.
 * 
 * @returns Shared Intl.Segmenter instance configured for grapheme segmentation
 */
export function getSegmenter(): Intl.Segmenter {
	return segmenter;
}

// --- Two-tier cache ---
//
// Grapheme cache: small strings (≤20 chars) from wrap-ansi per-grapheme calls.
// Population is bounded (char × ANSI-color combos), so it stabilizes and
// never thrashes — even when the line cache overflows and clears.
//
// Line cache: longer strings from text-renderer, box-renderer, tui.ts.
// Full-clear on overflow (matches ink's approach).
//
// The old single 10K cache mixed both populations. When unique long lines
// filled it past 10K, the clear() nuked cached graphemes too, forcing
// string-width re-evaluation (including the expensive RGI_Emoji regex)
// on the next wrap-ansi pass.

const GRAPHEME_CACHE_SIZE = 2_000;
const graphemeCache = new Map<string, number>();

const LINE_CACHE_SIZE = 10_000;
const lineCache = new Map<string, number>();

/**
 * Calculates the visible width of a string in terminal columns.
 * 
 * Uses the battle-tested `string-width` package for accurate width calculation,
 * with a fast ASCII path and two-tier LRU cache for performance.
 * 
 * @param str - String to measure
 * @returns Width in terminal columns
 * 
 * @example
 * ```typescript
 * visibleWidth('hello');           // 5
 * visibleWidth('🚀');              // 2 (emoji)
 * visibleWidth('\x1b[31mred\x1b[0m'); // 3 (ANSI codes ignored)
 * visibleWidth('こんにちは');        // 10 (East Asian wide chars)
 * ```
 */
export function visibleWidth(str: string): number {
	if (str.length === 0) return 0;

	// Fast path: pure ASCII printable
	let isPureAscii = true;
	for (let i = 0; i < str.length; i++) {
		const code = str.charCodeAt(i);
		if (code < 0x20 || code > 0x7e) {
			isPureAscii = false;
			break;
		}
	}
	if (isPureAscii) return str.length;

	// Short strings (graphemes) use dedicated cache
	if (str.length <= 20) {
		const cached = graphemeCache.get(str);
		if (cached !== undefined) return cached;

		const width = stringWidth(str.includes('\t') ? str.replace(/\t/g, '   ') : str);
		if (graphemeCache.size >= GRAPHEME_CACHE_SIZE) graphemeCache.clear();
		graphemeCache.set(str, width);
		return width;
	}

	// Longer strings use line cache
	const cached = lineCache.get(str);
	if (cached !== undefined) return cached;

	const width = stringWidth(str.includes('\t') ? str.replace(/\t/g, '   ') : str);
	if (lineCache.size >= LINE_CACHE_SIZE) lineCache.clear();
	lineCache.set(str, width);
	return width;
}

const PUNCTUATION_REGEX = /[(){}[\]<>.,;:'"!?+\-=*/\\|&%^$#@~`]/;

export function isWhitespaceChar(char: string): boolean {
	return /\s/.test(char);
}

export function isPunctuationChar(char: string): boolean {
	return PUNCTUATION_REGEX.test(char);
}
