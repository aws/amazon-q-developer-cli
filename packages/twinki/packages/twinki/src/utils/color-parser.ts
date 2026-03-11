/**
 * Color parsing and ANSI code generation.
 *
 * Uses chalk as the single source of truth for color resolution.
 * Chalk handles terminal capability fallback automatically
 * (truecolor → 256-color → 16-color → basic).
 *
 * Supported formats:
 * - Named: 'red', 'greenBright', etc.
 * - Hex: '#ff0000', '##ff0000', '#f00'
 * - RGB: 'rgb(255, 0, 0)'
 * - ANSI 256: 'ansi256(196)'
 *
 * Uses a forced truecolor chalk instance so twinki always generates
 * full-fidelity ANSI codes. Terminal downgrading is handled at the
 * output layer, not the color generation layer.
 */
import { Chalk, type ForegroundColorName, type BackgroundColorName } from 'chalk';

// Force truecolor — twinki controls ANSI output directly.
const chalk = new Chalk({ level: 3 });

const rgbRegex = /^rgb\(\s?(\d+),\s?(\d+),\s?(\d+)\s?\)$/;
const ansiRegex = /^ansi256\(\s?(\d+)\s?\)$/;

const isNamedColor = (color: string): color is ForegroundColorName => {
	return color in chalk;
};

/**
 * Normalizes hex color strings to standard `#rrggbb` format.
 * Handles edge cases from theme systems: `##rrggbb`, `#rgb`, bare `rrggbb`.
 */
function normalizeHex(color: string): string {
	if (!color.startsWith('#')) return color;
	let hex = color.replace(/^#+/, '');
	if (hex.length === 3) {
		hex = hex[0]! + hex[0]! + hex[1]! + hex[1]! + hex[2]! + hex[2]!;
	}
	return '#' + hex;
}

/**
 * Applies a color to a string using ANSI escape sequences.
 *
 * @param str - Text to colorize
 * @param color - Color value (named, hex, rgb, ansi256)
 * @param type - 'foreground' or 'background'
 * @returns Colorized string, or original string if color is invalid
 */
export function colorize(
	str: string,
	color: string | undefined,
	type: 'foreground' | 'background',
): string {
	if (!color) return str;

	if (isNamedColor(color)) {
		if (type === 'foreground') return chalk[color](str);
		const methodName = `bg${color[0]!.toUpperCase() + color.slice(1)}` as BackgroundColorName;
		return chalk[methodName](str);
	}

	if (color.startsWith('#')) {
		const normalized = normalizeHex(color);
		return type === 'foreground'
			? chalk.hex(normalized)(str)
			: chalk.bgHex(normalized)(str);
	}

	if (color.startsWith('ansi256')) {
		const matches = ansiRegex.exec(color);
		if (!matches) return str;
		const value = Number(matches[1]);
		return type === 'foreground'
			? chalk.ansi256(value)(str)
			: chalk.bgAnsi256(value)(str);
	}

	if (color.startsWith('rgb')) {
		const matches = rgbRegex.exec(color);
		if (!matches) return str;
		return type === 'foreground'
			? chalk.rgb(Number(matches[1]), Number(matches[2]), Number(matches[3]))(str)
			: chalk.bgRgb(Number(matches[1]), Number(matches[2]), Number(matches[3]))(str);
	}

	return str;
}

/**
 * Converts a color string to ANSI SGR parameters.
 *
 * Extracts the raw SGR code from chalk's output. Used by the reconciler
 * to build ANSI sequences for Box backgroundColor and Text color props.
 *
 * @param color - Color value (any supported format)
 * @param bg - If true, returns background code; otherwise foreground
 * @returns ANSI SGR parameter string (e.g. '38;2;255;0;0'), or empty string if invalid
 */
export function colorToAnsi(color: string, bg: boolean): string {
	if (!color) return '';
	const test = colorize('X', color, bg ? 'background' : 'foreground');
	if (test === 'X') return ''; // colorize returned unchanged — invalid color
	const match = test.match(/\x1b\[([0-9;]+)m/);
	return match ? match[1]! : '';
}
