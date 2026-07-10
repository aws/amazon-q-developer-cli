/**
 * Monokai Pro (Spectrum) UI palette — the single source of truth for the
 * app's chrome colors (borders, statusline, tree, accents). Keeping colors
 * in one place is the DRY home for theming the interface.
 *
 * Note: "Monokai Pro" is a proprietary editor theme, so its TextMate theme
 * file is not bundled with shiki. We use its well-known color *palette* for
 * the UI here, and default code syntax highlighting to shiki's bundled
 * `monokai` (its closest available equivalent). See this example's README.
 */
export const palette = {
	bg: '#2d2a2e',
	bgAlt: '#403e41',
	fg: '#fcfcfa',
	dim: '#727072',
	red: '#ff6188',
	orange: '#fc9867',
	yellow: '#ffd866',
	green: '#a9dc76',
	blue: '#78dce8',
	purple: '#ab9df2',
} as const;

export type Palette = typeof palette;
