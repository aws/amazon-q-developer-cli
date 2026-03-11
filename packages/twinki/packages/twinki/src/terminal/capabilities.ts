/**
 * Terminal capability detection and caching.
 * 
 * Detects various terminal features based on environment variables
 * and known terminal identifiers to enable appropriate functionality.
 */

/**
 * Describes the capabilities of the current terminal.
 */
export interface TerminalCapabilities {
	/** Image display protocol support */
	images: 'kitty' | 'iterm2' | null;
	/** True color (24-bit) support */
	trueColor: boolean;
	/** Hyperlink support */
	hyperlinks: boolean;
}

let cached: TerminalCapabilities | null = null;

/**
 * Detects terminal capabilities based on environment variables.
 * 
 * Analyzes TERM, TERM_PROGRAM, COLORTERM and other environment
 * variables to determine what features the terminal supports.
 * Results are cached for subsequent calls.
 * 
 * @returns Object describing terminal capabilities
 */
export function detectCapabilities(): TerminalCapabilities {
	if (cached) return cached;

	const env = process.env;
	const term = env.TERM ?? '';
	const termProgram = env.TERM_PROGRAM ?? '';
	const colorterm = env.COLORTERM ?? '';

	const isKitty = term === 'xterm-kitty' || termProgram === 'kitty';
	const isGhostty = termProgram === 'ghostty';
	const isWezTerm = termProgram === 'WezTerm';
	const isITerm2 = termProgram === 'iTerm.app';
	const isVSCode = termProgram === 'vscode';
	const isAlacritty = env.ALACRITTY_LOG !== undefined;

	let images: TerminalCapabilities['images'] = null;
	if (isKitty || isGhostty || isWezTerm) {
		images = 'kitty';
	} else if (isITerm2) {
		images = 'iterm2';
	}

	const trueColor = colorterm === 'truecolor' || colorterm === '24bit' ||
		isKitty || isGhostty || isWezTerm || isITerm2 || isAlacritty;

	const hyperlinks = isKitty || isGhostty || isWezTerm || isITerm2 || isVSCode || isAlacritty;

	cached = { images, trueColor, hyperlinks };
	return cached;
}

/**
 * Gets the cached terminal capabilities, detecting them if not already cached.
 * 
 * @returns Object describing terminal capabilities
 */
export function getCapabilities(): TerminalCapabilities {
	return detectCapabilities();
}

/**
 * Resets the capabilities cache, forcing re-detection on next call.
 * 
 * Useful for testing or when terminal environment changes at runtime.
 */
export function resetCapabilitiesCache(): void {
	cached = null;
}
