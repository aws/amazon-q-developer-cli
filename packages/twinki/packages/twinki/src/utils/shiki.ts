let highlighterPromise: Promise<any> | null = null;
export let cachedHighlighter: any = null;
export const loadedThemes = new Set<string>();
export const loadedLangs = new Set<string>();

export function getHighlighterSync(): any | null {
	return cachedHighlighter;
}

/**
 * Lazy-loaded shiki highlighter with on-demand theme/language loading.
 *
 * @param theme - Theme to load (e.g. 'monokai', 'dracula')
 * @param lang - Language to load (e.g. 'typescript', 'python')
 * @returns Shared shiki highlighter instance
 */
export async function getHighlighter(theme?: string, lang?: string) {
	if (!cachedHighlighter) {
		if (!highlighterPromise) {
			highlighterPromise = import('shiki').then(async (shiki) => {
				cachedHighlighter = await shiki.createHighlighter({ themes: [], langs: [] });
				return cachedHighlighter;
			});
		}
		await highlighterPromise;
	}

	if (theme && !loadedThemes.has(theme)) {
		try {
			await cachedHighlighter.loadTheme(theme);
			loadedThemes.add(theme);
		} catch { /* unknown theme — caller's try/catch handles it */ }
	}

	if (lang && !loadedLangs.has(lang)) {
		try {
			await cachedHighlighter.loadLanguage(lang);
			loadedLangs.add(lang);
		} catch { /* unknown lang */ }
	}

	return cachedHighlighter;
}
