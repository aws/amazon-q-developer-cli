import wrapAnsi from 'wrap-ansi';
import cliTruncate from 'cli-truncate';
import {type Styles} from './styles.js';

const cache = new Map<string, string>();
let cacheBytes = 0;
const MAX_CACHE_BYTES = 50 * 1024 * 1024;

const hasAnsi = /\x1b/;
const hasWide = /[\u2E80-\uFFFF]/;

// Match full SGR escape sequences (e.g. \e[38;2;128;128;128m)
const SGR_REGEX = /\x1b\[([\d;]+)m/g;

/**
 * Fix ANSI color codes across line breaks.
 *
 * wrap-ansi only tracks single-number SGR codes (\e[38m) when closing/reopening
 * colors at line breaks. Compound sequences like truecolor (\e[38;2;r;g;bm) and
 * 256-color (\e[38;5;nm) get mangled — only \e[38m is re-emitted after the break,
 * losing the color parameters. This function re-processes the wrapped output to
 * properly carry the full sequence across newlines.
 */
function fixAnsiWrapping(text: string): string {
	const lines = text.split('\n');
	if (lines.length <= 1) return text;

	let activeSequence: string | undefined;
	const result: string[] = [];

	for (const line of lines) {
		let fixedLine = line;

		// If we have an active color from a previous line, prepend it
		if (activeSequence) {
			// Check if the line starts with a simple (broken) re-open like \e[38m
			// that wrap-ansi inserted — replace it with the full sequence
			const brokenReopen = /^\x1b\[\d+m/;
			if (brokenReopen.test(fixedLine)) {
				fixedLine = fixedLine.replace(brokenReopen, activeSequence);
			} else {
				fixedLine = activeSequence + fixedLine;
			}
		}

		// Track the last active SGR sequence on this line
		let lastOpen: string | undefined;
		let match: RegExpExecArray | null;
		SGR_REGEX.lastIndex = 0;
		while ((match = SGR_REGEX.exec(fixedLine)) !== null) {
			const params = match[1]!;
			const firstCode = Number.parseInt(params.split(';')[0]!, 10);
			// Code 39 = default foreground (reset), 0 = full reset
			if (firstCode === 39 || firstCode === 0 || firstCode === 49) {
				lastOpen = undefined;
			} else {
				lastOpen = match[0];
			}
		}

		activeSequence = lastOpen;
		result.push(fixedLine);
	}

	return result.join('\n');
}

function wrapPlainAscii(text: string, maxWidth: number): string {
	const lines = text.split('\n');
	const result: string[] = [];
	for (const line of lines) {
		if (line.length <= maxWidth) {
			result.push(line);
		} else {
			for (let i = 0; i < line.length; i += maxWidth) {
				result.push(line.slice(i, i + maxWidth));
			}
		}
	}
	return result.join('\n');
}

const wrapText = (
	text: string,
	maxWidth: number,
	wrapType: Styles['textWrap'],
): string => {
	const cacheKey = text + String(maxWidth) + String(wrapType);
	const cachedText = cache.get(cacheKey);

	if (cachedText) {
		return cachedText;
	}

	let wrappedText = text;

	if (wrapType === 'wrap') {
		if (!hasAnsi.test(text) && !hasWide.test(text)) {
			wrappedText = wrapPlainAscii(text, maxWidth);
		} else {
			wrappedText = fixAnsiWrapping(wrapAnsi(text, maxWidth, {
				trim: false,
				hard: true,
			}));
		}
	}

	if (wrapType!.startsWith('truncate')) {
		let position: 'end' | 'middle' | 'start' = 'end';

		if (wrapType === 'truncate-middle') {
			position = 'middle';
		}

		if (wrapType === 'truncate-start') {
			position = 'start';
		}

		wrappedText = cliTruncate(text, maxWidth, {position});
	}

	const entryBytes = (cacheKey.length + wrappedText.length) * 2;
	if (cacheBytes + entryBytes > MAX_CACHE_BYTES) {
		cache.clear();
		cacheBytes = 0;
	}
	cache.set(cacheKey, wrappedText);
	cacheBytes += entryBytes;

	return wrappedText;
};

export default wrapText;
