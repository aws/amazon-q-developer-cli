import stripAnsi from "strip-ansi";
import { eastAsianWidth } from "get-east-asian-width";

// Shared grapheme segmenter instance
const segmenter = new Intl.Segmenter(undefined, { granularity: "grapheme" });

/**
 * Gets the shared grapheme segmenter instance.
 * 
 * @returns Shared Intl.Segmenter instance configured for grapheme segmentation
 */
export function getSegmenter(): Intl.Segmenter {
	return segmenter;
}

// ---------------------------------------------------------------------------
// Fast emoji detection via codepoint ranges.
// Replaces /^\p{RGI_Emoji}$/v which is ~900x slower in Bun's regex engine.
// ---------------------------------------------------------------------------
function isEmoji(s: string): boolean {
	const cp = s.codePointAt(0)!;
	if (cp < 0x200D) {
		// Keycap sequences: digit/#/* + optional VS16 + ⃣ (0x20E3)
		return s.length >= 2 && s.includes('\u20E3');
	}
	if (s.includes('\u200D')) return true; // ZWJ sequence
	if (cp >= 0x1F1E6 && cp <= 0x1F1FF) return true; // Regional indicators (flags)
	if (cp >= 0x1F600 && cp <= 0x1F64F) return true; // Emoticons
	if (cp >= 0x1F300 && cp <= 0x1F5FF) return true; // Misc Symbols & Pictographs
	if (cp >= 0x1F680 && cp <= 0x1F6FF) return true; // Transport & Map
	if (cp >= 0x1F900 && cp <= 0x1F9FF) return true; // Supplemental Symbols
	if (cp >= 0x1FA00 && cp <= 0x1FA6F) return true; // Chess Symbols
	if (cp >= 0x1FA70 && cp <= 0x1FAFF) return true; // Symbols Extended-A
	if (cp >= 0x2600 && cp <= 0x27BF) {
		// Misc Symbols & Dingbats — only emoji-width if VS16 present or multi-char
		return s.length >= 2;
	}
	if (cp >= 0x2300 && cp <= 0x23FF) {
		// Misc Technical — only emoji-width if VS16 present or multi-char
		return s.length >= 2;
	}
	// Skin tone modifier following base emoji
	if (s.length > 2) {
		const cp2 = s.codePointAt(2) ?? 0;
		if (cp2 >= 0x1F3FB && cp2 <= 0x1F3FF) return true;
	}
	return false;
}

// ---------------------------------------------------------------------------
// Zero-width codepoint checks (inlined from string-width logic)
// ---------------------------------------------------------------------------
function isZeroWidth(cp: number): boolean {
	if (cp <= 0x1F || (cp >= 0x7F && cp <= 0x9F)) return true; // Control
	if (cp >= 0x200B && cp <= 0x200F) return true; // ZW space, non-joiner, joiner, LTR/RTL marks
	if (cp === 0xFEFF) return true; // ZW no-break space
	if (cp >= 0x300 && cp <= 0x36F) return true; // Combining diacritical marks
	if (cp >= 0x1AB0 && cp <= 0x1AFF) return true; // Combining diacritical marks extended
	if (cp >= 0x1DC0 && cp <= 0x1DFF) return true; // Combining diacritical marks supplement
	if (cp >= 0x20D0 && cp <= 0x20FF) return true; // Combining diacritical marks for symbols
	if (cp >= 0xFE20 && cp <= 0xFE2F) return true; // Combining half marks
	if (cp >= 0xFE00 && cp <= 0xFE0F) return true; // Variation selectors
	if (cp >= 0xD800 && cp <= 0xDFFF) return true; // Surrogates
	return false;
}

// ---------------------------------------------------------------------------
// Fast string-width replacement. Same logic as string-width@8 but with
// codepoint-based emoji detection instead of the /^\p{RGI_Emoji}$/v regex.
// ---------------------------------------------------------------------------
function fastStringWidth(str: string): number {
	if (str.length === 0) return 0;

	// Strip ANSI if present
	if (str.includes('\u001B') || str.includes('\u009B')) {
		str = stripAnsi(str);
	}
	if (str.length === 0) return 0;

	let width = 0;
	for (const { segment } of segmenter.segment(str)) {
		const cp = segment.codePointAt(0)!;

		if (isZeroWidth(cp)) continue;

		if (isEmoji(segment)) {
			width += 2;
			continue;
		}

		width += eastAsianWidth(cp);
	}
	return width;
}

// --- Two-tier cache ---
//
// Grapheme cache: small strings (≤20 chars) from wrap-ansi per-grapheme calls.
// Population is bounded (char × ANSI-color combos), so it stabilizes and
// never thrashes — even when the line cache overflows and clears.
//
// Line cache: longer strings from text-renderer, box-renderer, tui.ts.
// Full-clear on overflow (matches ink's approach).

const GRAPHEME_CACHE_SIZE = 4_000;
const graphemeCache = new Map<string, number>();

const LINE_CACHE_SIZE = 25_000;
const lineCache = new Map<string, number>();

/** Code points above this value require two UTF-16 units (surrogate pair). */
const SURROGATE_PAIR_BOUNDARY = 0xFFFF;

/**
 * Calculates the visible width of a string in terminal columns.
 * 
 * Uses a fast inline width calculator with codepoint-based emoji detection,
 * bypassing string-width's /^\p{RGI_Emoji}$/v regex which is ~900x slower in Bun.
 * Two-tier LRU cache and fast ASCII paths for performance.
 * 
 * @param str - String to measure
 * @returns Width in terminal columns
 */
export function visibleWidth(str: string): number {
	if (str.length === 0) return 0;

	// Fast path: pure ASCII printable (no ANSI, no wide chars)
	let isPureAscii = true;
	for (let i = 0; i < str.length; i++) {
		const code = str.charCodeAt(i);
		if (code < 0x20 || code > 0x7e) {
			isPureAscii = false;
			break;
		}
	}
	if (isPureAscii) return str.length;

	// Strip APC sequences (ESC _ ... BEL) before measuring.
	// Neither string-width nor fastStringWidth recognise APC as zero-width,
	// so the CURSOR_MARKER (\x1b_twinki:c\x07) would otherwise inflate the
	// measured width.
	if (str.includes('\x1b_')) {
		let result = '';
		let i = 0;
		while (i < str.length) {
			if (str.charCodeAt(i) === 0x1b && str.charCodeAt(i + 1) === 0x5f) {
				// Skip until BEL or end of string
				i += 2;
				while (i < str.length && str.charCodeAt(i) !== 0x07) i++;
				if (i < str.length) i++; // skip BEL
			} else {
				// Use codePointAt to avoid splitting surrogate pairs
				const cp = str.codePointAt(i)!;
				result += String.fromCodePoint(cp);
				i += cp > SURROGATE_PAIR_BOUNDARY ? 2 : 1;
			}
		}
		str = result;
		if (str.length === 0) return 0;
	}

	// Short strings (graphemes) use dedicated cache
	if (str.length <= 20) {
		const cached = graphemeCache.get(str);
		if (cached !== undefined) return cached;

		const width = fastStringWidth(str.includes('\t') ? str.replace(/\t/g, '   ') : str);
		if (graphemeCache.size >= GRAPHEME_CACHE_SIZE) graphemeCache.clear();
		graphemeCache.set(str, width);
		return width;
	}

	// Fast path: ASCII + ANSI only (no wide chars, no emoji)
	// Runs before cache — ANSI-styled lines are unique per render, caching wastes space.
	let ansiAsciiWidth = 0;
	let isAnsiAscii = true;
	for (let i = 0; i < str.length; i++) {
		const code = str.charCodeAt(i);
		if (code === 0x1b) {
			// Skip CSI sequences: ESC [ <params> <final byte>
			if (i + 1 < str.length && str.charCodeAt(i + 1) === 0x5b) {
				i += 2;
				// Skip parameter bytes (0x30-0x3f) and intermediate bytes (0x20-0x2f)
				while (i < str.length && str.charCodeAt(i) < 0x40) i++;
				// i now points at final byte (0x40-0x7e), loop increment skips it
				continue;
			}
			// Other ESC sequences — bail
			isAnsiAscii = false;
			break;
		}
		if (code >= 0x20 && code <= 0x7e) {
			ansiAsciiWidth++;
		} else if (code === 0x09) {
			ansiAsciiWidth += 3; // tab = 3 spaces
		} else {
			isAnsiAscii = false;
			break;
		}
	}
	if (isAnsiAscii) return ansiAsciiWidth;

	// Longer strings use line cache
	const cached = lineCache.get(str);
	if (cached !== undefined) return cached;

	const width = fastStringWidth(str.includes('\t') ? str.replace(/\t/g, '   ') : str);
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
