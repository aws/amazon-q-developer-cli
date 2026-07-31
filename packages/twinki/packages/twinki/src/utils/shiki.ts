import { bundledLanguagesInfo } from 'shiki/langs';

let highlighterPromise: Promise<any> | null = null;
export let cachedHighlighter: any = null;
export const loadedThemes = new Set<string>();
export const loadedLangs = new Set<string>();
const languageIds = new Map(
	bundledLanguagesInfo.flatMap(({ id, name, aliases = [] }) =>
		[id, name, ...aliases].map(
			(value) => [value.toLowerCase(), id] as const,
		),
	),
);

export function canonicalShikiLanguage(language: string): string | undefined {
	return languageIds.get(language.trim().toLowerCase());
}

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

	const canonicalLanguage = lang
		? canonicalShikiLanguage(lang) ?? lang
		: undefined;
	if (canonicalLanguage && !loadedLangs.has(canonicalLanguage)) {
		try {
			await cachedHighlighter.loadLanguage(canonicalLanguage);
			loadedLangs.add(canonicalLanguage);
		} catch { /* unknown lang */ }
	}

	return cachedHighlighter;
}
