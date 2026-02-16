const sgrMouseRe = /\x1b\[<(\d+);(\d+);(\d+)([Mm])/;

export type MouseEvent = {
	button: 'left' | 'right' | 'middle' | 'scrollUp' | 'scrollDown' | 'none';
	col: number;
	row: number;
	type: 'press' | 'release' | 'drag';
};

const buttonMap = (b: number): {button: MouseEvent['button']; drag: boolean} => {
	const drag = !!(b & 32);
	const base = b & ~32;
	if (base & 64) return {button: (base & 1) ? 'scrollDown' : 'scrollUp', drag};
	const btn = base & 3;
	if (btn === 0) return {button: 'left', drag};
	if (btn === 1) return {button: 'middle', drag};
	if (btn === 2) return {button: 'right', drag};
	return {button: 'none', drag};
};

export const parseMouse = (input: string): MouseEvent | null => {
	const match = sgrMouseRe.exec(input);
	if (!match) return null;

	const raw = parseInt(match[1]!, 10);
	const {button, drag} = buttonMap(raw);
	return {
		button,
		col: parseInt(match[2]!, 10),
		row: parseInt(match[3]!, 10),
		type: drag ? 'drag' : (match[4] === 'M' ? 'press' : 'release'),
	};
};

export const isMouseSequence = (input: string): boolean => sgrMouseRe.test(input);