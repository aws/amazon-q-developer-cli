import { colorToAnsi } from '../utils/color-parser.js';
import { DANGEROUS_CSI_RE } from './constants.js';
import type { ComponentProps } from '../types/props.js';

/**
 * Strips dangerous CSI escape sequences from text content.
 * 
 * Preserves SGR sequences (\x1b[...m) which control colors/bold/etc.
 * Strips all other CSI sequences (cursor movement, screen clearing,
 * alt screen, scroll region, etc.) which would corrupt the differential
 * renderer's cursor tracking if written to the terminal.
 * 
 * @param text - Text to sanitize
 * @returns Sanitized text with dangerous CSI sequences removed
 */
export function sanitizeText(text: string): string {
	return text.replace(DANGEROUS_CSI_RE, '');
}

/**
 * Applies ANSI styling codes to text based on props.
 * 
 * Converts React-style props (bold, color, etc.) to ANSI escape sequences
 * for terminal display. Handles both named colors and hex colors.
 * 
 * @param text - Text to style
 * @param props - Styling properties
 * @returns Text with ANSI escape sequences
 */
export function stylize(text: string, props: ComponentProps): string {
	const codes: string[] = [];
	if (props.bold) codes.push('1');
	if (props.dimColor) codes.push('2');
	if (props.italic) codes.push('3');
	if (props.underline) codes.push('4');
	if (props.strikethrough) codes.push('9');
	if (props.inverse) codes.push('7');
	if (props.color) codes.push(colorToAnsi(props.color, false));
	if (props.backgroundColor) codes.push(colorToAnsi(props.backgroundColor, true));
	if (codes.length === 0) return text;
	return `\x1b[${codes.join(';')}m${text}\x1b[0m`;
}