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

// Cache for width calculations.
// string-width compiles /^\p{RGI_Emoji}$/v on every call — caching avoids
// redundant regex work. 10K entries matches ink's cached-string-width.ts.
const WIDTH_CACHE_SIZE = 10_000;
const widthCache = new Map<string, number>();

/**
 * Calculates the visible width of a string in terminal columns.
 * 
 * Uses the battle-tested `string-width` package for accurate width calculation,
 * with a fast ASCII path and LRU cache for performance.
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

	// Check cache
	const cached = widthCache.get(str);
	if (cached !== undefined) return cached;

	const width = stringWidth(str.includes('\t') ? str.replace(/\t/g, '   ') : str);

	// Cache with full clear on overflow (matches ink's approach)
	if (widthCache.size >= WIDTH_CACHE_SIZE) widthCache.clear();
	widthCache.set(str, width);

	return width;
}

const PUNCTUATION_REGEX = /[(){}[\]<>.,;:'"!?+\-=*/\\|&%^$#@~`]/;

export function isWhitespaceChar(char: string): boolean {
	return /\s/.test(char);
}

export function isPunctuationChar(char: string): boolean {
	return PUNCTUATION_REGEX.test(char);
}
