/** Encodes text for the terminal's system clipboard using OSC 52. */
export function osc52ClipboardSequence(text: string): string {
	const payload = Buffer.from(text, 'utf8').toString('base64');
	return `\x1b]52;c;${payload}\x07`;
}
