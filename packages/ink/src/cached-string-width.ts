/**
 * Singleton memoized string-width wrapper.
 *
 * `string-width` is expensive because it compiles /^\p{RGI_Emoji}$/v on every
 * call.  By caching results here, every call-site in Ink (output.ts,
 * measure-text.ts, render-node-to-output.ts, wrap-text.ts, ink.tsx) shares a
 * single cache and avoids redundant regex work.
 */
import originalStringWidth from 'string-width';

const cache = new Map<string, number>();

export function cachedStringWidth(str: string): number {
	let w = cache.get(str);
	if (w === undefined) {
		w = originalStringWidth(str);
		if (cache.size > 10_000) cache.clear();
		cache.set(str, w);
	}
	return w;
}

/**
 * Cached replacement for `widest-line`.
 * Splits on newlines and returns the max visual width.
 */
export function cachedWidestLine(str: string): number {
	let max = 0;
	for (const line of str.split('\n')) {
		const w = cachedStringWidth(line);
		if (w > max) max = w;
	}
	return max;
}
