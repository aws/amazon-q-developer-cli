const ESC = 0x1b;
const BEL = 0x07;
const ST = 0x9c;
const CSI = 0x9b;
const STRING_CONTROLS = new Set([0x90, 0x98, 0x9d, 0x9e, 0x9f]);

function stringSequenceEnd(value: string, start: number): number {
	for (let index = start; index < value.length; index += 1) {
		const code = value.charCodeAt(index);
		if (code === BEL || code === ST) return index + 1;
		if (code === ESC && value.charCodeAt(index + 1) === 0x5c) {
			return index + 2;
		}
	}
	return value.length;
}

function controlSequenceEnd(value: string, start: number): number {
	for (let index = start; index < value.length; index += 1) {
		const code = value.charCodeAt(index);
		if (code >= 0x40 && code <= 0x7e) return index + 1;
	}
	return value.length;
}

function escapeSequenceEnd(value: string, start: number): number {
	const next = value.charCodeAt(start + 1);
	if (next === 0x5b) return controlSequenceEnd(value, start + 2);
	if ([0x50, 0x58, 0x5d, 0x5e, 0x5f].includes(next)) {
		return stringSequenceEnd(value, start + 2);
	}
	let index = start + 1;
	while (index < value.length) {
		const code = value.charCodeAt(index);
		if (code >= 0x30 && code <= 0x7e) return index + 1;
		index += 1;
	}
	return value.length;
}

/** Removes terminal commands while preserving printable Unicode, tabs, and newlines. */
export function sanitizeTerminalText(value: string): string {
	let result = '';
	for (let index = 0; index < value.length;) {
		const code = value.charCodeAt(index);
		if (code === ESC) {
			index = escapeSequenceEnd(value, index);
			continue;
		}
		if (code === CSI) {
			index = controlSequenceEnd(value, index + 1);
			continue;
		}
		if (STRING_CONTROLS.has(code)) {
			index = stringSequenceEnd(value, index + 1);
			continue;
		}
		if (
			(code < 0x20 && code !== 0x09 && code !== 0x0a) ||
			(code >= 0x7f && code <= 0x9f)
		) {
			index += 1;
			continue;
		}
		const point = value.codePointAt(index)!;
		result += String.fromCodePoint(point);
		index += point > 0xffff ? 2 : 1;
	}
	return result;
}
