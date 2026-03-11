/**
 * SGR mouse event parser.
 *
 * SGR (mode 1006) sequences: CSI < Cb ; Cx ; Cy M  (press)
 *                            CSI < Cb ; Cx ; Cy m  (release)
 *
 * Cb encodes button + modifiers:
 *   bits 0-1: button (0=left, 1=middle, 2=right)
 *   bit 5:    motion (32)
 *   bit 2:    shift  (4)
 *   bit 3:    alt    (8)
 *   bit 4:    ctrl   (16)
 *   bits 6-7: 64=scroll up, 65=scroll down
 */

export type MouseButton = 'left' | 'middle' | 'right' | 'none';
export type MouseEventType = 'mousedown' | 'mouseup' | 'mousemove' | 'scrollup' | 'scrolldown';

export interface MouseEvent {
	x: number;       // 0-based column
	y: number;       // 0-based row
	button: MouseButton;
	type: MouseEventType;
	shift: boolean;
	alt: boolean;
	ctrl: boolean;
}

const SGR_MOUSE_RE = /^\x1b\[<(\d+);(\d+);(\d+)([Mm])$/;

export function parseSGRMouse(data: string): MouseEvent | null {
	const m = SGR_MOUSE_RE.exec(data);
	if (!m) return null;

	const cb = parseInt(m[1], 10);
	const x = parseInt(m[2], 10) - 1; // SGR is 1-based
	const y = parseInt(m[3], 10) - 1;
	const isRelease = m[4] === 'm';

	const shift = (cb & 4) !== 0;
	const alt = (cb & 8) !== 0;
	const ctrl = (cb & 16) !== 0;
	const motion = (cb & 32) !== 0;
	const btnBits = cb & 3;
	const highBits = cb & 192;

	// Scroll events
	if (highBits === 64) {
		return { x, y, button: 'none', type: btnBits === 0 ? 'scrollup' : 'scrolldown', shift, alt, ctrl };
	}

	const button: MouseButton = btnBits === 0 ? 'left' : btnBits === 1 ? 'middle' : btnBits === 2 ? 'right' : 'none';

	if (motion) {
		return { x, y, button, type: 'mousemove', shift, alt, ctrl };
	}

	return { x, y, button, type: isRelease ? 'mouseup' : 'mousedown', shift, alt, ctrl };
}

export function isSGRMouse(data: string): boolean {
	return SGR_MOUSE_RE.test(data);
}
