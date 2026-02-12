import wrapAnsi from 'wrap-ansi';
import cliTruncate from 'cli-truncate';
import {type Styles} from './styles.js';

const cache = new Map<string, string>();
let cacheBytes = 0;
const MAX_CACHE_BYTES = 50 * 1024 * 1024;

const hasAnsi = /\x1b/;
const hasWide = /[\u2E80-\uFFFF]/;

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
			wrappedText = wrapAnsi(text, maxWidth, {
				trim: false,
				hard: true,
			});
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
