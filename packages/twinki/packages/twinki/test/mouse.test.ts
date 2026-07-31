import { describe, it, expect } from 'vitest';
import {
	isSGRMouse,
	parseSGRMouse,
	parseSGRMouseEvents,
} from '../src/input/mouse.js';

describe('parseSGRMouse', () => {
	it('parses left button press', () => {
		const e = parseSGRMouse('\x1b[<0;10;5M');
		expect(e).toEqual({ x: 9, y: 4, button: 'left', type: 'mousedown', shift: false, alt: false, ctrl: false });
	});

	it('parses left button release', () => {
		const e = parseSGRMouse('\x1b[<0;10;5m');
		expect(e).toEqual({ x: 9, y: 4, button: 'left', type: 'mouseup', shift: false, alt: false, ctrl: false });
	});

	it('accepts iTerm mouse sequences with a trailing separator', () => {
		expect(parseSGRMouse('\x1b[<2;10;5;M')).toMatchObject({
			x: 9,
			y: 4,
			button: 'right',
			type: 'mousedown',
		});
	});

	it('parses right button press', () => {
		const e = parseSGRMouse('\x1b[<2;1;1M');
		expect(e!.button).toBe('right');
		expect(e!.type).toBe('mousedown');
	});

	it('parses middle button press', () => {
		const e = parseSGRMouse('\x1b[<1;1;1M');
		expect(e!.button).toBe('middle');
	});

	it('parses motion event', () => {
		const e = parseSGRMouse('\x1b[<32;15;8M');
		expect(e!.type).toBe('mousemove');
		expect(e!.x).toBe(14);
		expect(e!.y).toBe(7);
	});

	it('parses scroll up', () => {
		const e = parseSGRMouse('\x1b[<64;1;1M');
		expect(e!.type).toBe('scrollup');
	});

	it('parses scroll down', () => {
		const e = parseSGRMouse('\x1b[<65;1;1M');
		expect(e!.type).toBe('scrolldown');
	});

	it('parses shift modifier', () => {
		const e = parseSGRMouse('\x1b[<4;1;1M');
		expect(e!.shift).toBe(true);
		expect(e!.button).toBe('left');
	});

	it('parses ctrl modifier', () => {
		const e = parseSGRMouse('\x1b[<16;1;1M');
		expect(e!.ctrl).toBe(true);
	});

	it('parses alt modifier', () => {
		const e = parseSGRMouse('\x1b[<8;1;1M');
		expect(e!.alt).toBe(true);
	});

	it('returns null for non-mouse data', () => {
		expect(parseSGRMouse('\x1b[A')).toBeNull();
		expect(parseSGRMouse('hello')).toBeNull();
	});
});

describe('parseSGRMouseEvents', () => {
	it('parses batched events', () => {
		const events = parseSGRMouseEvents(
			'\x1b[<0;2;1M\x1b[<32;5;1M\x1b[<0;5;1m',
		);
		expect(events?.map(({ type, x, y }) => ({ type, x, y }))).toEqual([
			{ type: 'mousedown', x: 1, y: 0 },
			{ type: 'mousemove', x: 4, y: 0 },
			{ type: 'mouseup', x: 4, y: 0 },
		]);
	});

	it('rejects chunks containing non-mouse input', () => {
		expect(parseSGRMouseEvents('\x1b[<0;2;1Ma')).toBeNull();
	});
});

describe('isSGRMouse', () => {
	it('detects mouse sequences', () => {
		expect(isSGRMouse('\x1b[<0;1;1M')).toBe(true);
		expect(isSGRMouse('\x1b[<0;1;1m')).toBe(true);
		expect(isSGRMouse('\x1b[A')).toBe(false);
	});
});
