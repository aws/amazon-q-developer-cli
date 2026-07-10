import { useCallback, useState } from 'react';

/**
 * Code syntax-highlighting themes for shiki, cycled with Tab / Shift+Tab.
 *
 * Default (index 0) is `monokai` — the closest bundled shiki theme to
 * Monokai Pro, which is proprietary and not shipped with shiki. The app's
 * UI chrome uses the Monokai Pro (Spectrum) palette (see lib/palette.ts).
 */
export const THEMES = [
	'monokai',
	'dracula',
	'tokyo-night',
	'one-dark-pro',
	'catppuccin-mocha',
	'nord',
	'vitesse-dark',
	'github-dark',
	'catppuccin-latte',
	'github-light',
] as const;

export type ThemeName = (typeof THEMES)[number];

export interface ThemeRotation {
	/** Currently selected theme id. */
	theme: ThemeName;
	/** Index of the current theme. */
	index: number;
	/** Total number of themes. */
	count: number;
	/** Advance to the next theme (wraps). */
	next: () => void;
	/** Go to the previous theme (wraps). */
	prev: () => void;
}

/** Owns the single responsibility of cycling the syntax-highlight theme. */
export function useThemeRotation(): ThemeRotation {
	const [index, setIndex] = useState(0);
	const next = useCallback(() => setIndex((i) => (i + 1) % THEMES.length), []);
	const prev = useCallback(() => setIndex((i) => (i - 1 + THEMES.length) % THEMES.length), []);
	return { theme: THEMES[index]!, index, count: THEMES.length, next, prev };
}
