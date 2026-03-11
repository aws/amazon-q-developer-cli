import React, { useState } from 'react';
import { render, Text, Box, useInput, useFrames } from 'twinki';

// ── Palette ──────────────────────────────────────────────────────────────────
const bg = (n: number) => `\x1b[48;5;${n}m  \x1b[0m`;
const PAL: Record<string, string> = {
	'W': bg(231),  // white — ghost body
	'K': bg(232),  // dark gray — eyes + edge
	'x': bg(240),  // gray — soft edge
	'P': bg(93),   // purple — kiro cursor
	'.': bg(16),   // background (dark)
};

// ── Ghost sprites — 20×22, sampled from ghost.jpg reference ─────────────
// Facing right, eyes open — wide round body, oval eyes, left side droops
const GHOST_R_OPEN = [
	'..........xWWx........',//22
	'.......xWWWWWWWWx.....',//22
	'.....xWWWWWWWWWWWWx...',//22
	'....xWWWWWWWWWWWWWWx..',//22
	'...xWWWWWWWWWWWWWWWWx.',//22
	'...WWWWWWWWWWWWWWWWWW.',//22
	'..xWWWWWWWWWWWWWWWWWW.',//22
	'..WWWWWWWWWWWWWWWWWWW.',//22
	'..WWWWWWWWWWKKWWKKWWW.',//22
	'..WWWWWWWWWWKKWWKKWWW.',//22
	'..WWWWWWWWWWKKWWKKWWW.',//22
	'..WWWWWWWWWWWWWWWWWWW.',//22
	'..WWWWWWWWWWWWWWWWWWW.',//22
	'.xWWWWWWWWWWWWWWWWWWW.',//22
	'.WWWWWWWWWWWWWWWWWWWW.',//22
	'xWWWWWWWWWWWWWWWWWWWx.',//22
	'xWWWWWWWWWWWWWWWWWWx..',//22
	'xWWWWWWWWWWWWWWWWWWx..',//22
	'..xWWWWWWWWWWWWWWWx...',//22
	'...xWWWWWWWWWWWWWx....',//22
	'....xWWWWx..xWWWx.....',//22
	'.....xx........xx.....',//22
];
// Facing right, eyes shut
const GHOST_R_SHUT = [
	'..........xWWx........',//22
	'.......xWWWWWWWWx.....',//22
	'.....xWWWWWWWWWWWWx...',//22
	'....xWWWWWWWWWWWWWWx..',//22
	'...xWWWWWWWWWWWWWWWWx.',//22
	'...WWWWWWWWWWWWWWWWWW.',//22
	'..xWWWWWWWWWWWWWWWWWW.',//22
	'..WWWWWWWWWWWWWWWWWWW.',//22
	'..WWWWWWWWWWWWWWWWWWW.',//22
	'..WWWWWWWWWWKKWWKKWWW.',//22
	'..WWWWWWWWWWWWWWWWWWW.',//22
	'..WWWWWWWWWWWWWWWWWWW.',//22
	'.xWWWWWWWWWWWWWWWWWWW.',//22
	'.WWWWWWWWWWWWWWWWWWWW.',//22
	'xWWWWWWWWWWWWWWWWWWWx.',//22
	'xWWWWWWWWWWWWWWWWWWx..',//22
	'xWWWWWWWWWWWWWWWWWWx..',//22
	'..xWWWWWWWWWWWWWWWx...',//22
	'...xWWWWWWWWWWWWWx....',//22
	'....xWWWWx..xWWWx.....',//22
	'.....xx........xx.....',//22
];
// Wink left eye (right eye closes, left stays open)
const GHOST_R_WINKL = [
	'..........xWWx........',
	'.......xWWWWWWWWx.....',
	'.....xWWWWWWWWWWWWx...',
	'....xWWWWWWWWWWWWWWx..',
	'...xWWWWWWWWWWWWWWWWx.',
	'...WWWWWWWWWWWWWWWWWW.',
	'..xWWWWWWWWWWWWWWWWWW.',
	'..WWWWWWWWWWWWWWWWWWW.',
	'..WWWWWWWWWWKKWWWWWWW.',
	'..WWWWWWWWWWKKWWKKWWW.',
	'..WWWWWWWWWWKKWWWWWWW.',
	'..WWWWWWWWWWWWWWWWWWW.',
	'..WWWWWWWWWWWWWWWWWWW.',
	'.xWWWWWWWWWWWWWWWWWWW.',
	'.WWWWWWWWWWWWWWWWWWWW.',
	'xWWWWWWWWWWWWWWWWWWWx.',
	'xWWWWWWWWWWWWWWWWWWx..',
	'xWWWWWWWWWWWWWWWWWWx..',
	'..xWWWWWWWWWWWWWWWx...',
	'...xWWWWWWWWWWWWWx....',
	'....xWWWWx..xWWWx.....',
	'.....xx........xx.....',
];
// Wink right eye (left eye closes, right stays open)
const GHOST_R_WINKR = [
	'..........xWWx........',
	'.......xWWWWWWWWx.....',
	'.....xWWWWWWWWWWWWx...',
	'....xWWWWWWWWWWWWWWx..',
	'...xWWWWWWWWWWWWWWWWx.',
	'...WWWWWWWWWWWWWWWWWW.',
	'..xWWWWWWWWWWWWWWWWWW.',
	'..WWWWWWWWWWWWWWWWWWW.',
	'..WWWWWWWWWWWWWWKKWWW.',
	'..WWWWWWWWWWKKWWKKWWW.',
	'..WWWWWWWWWWWWWWKKWWW.',
	'..WWWWWWWWWWWWWWWWWWW.',
	'..WWWWWWWWWWWWWWWWWWW.',
	'.xWWWWWWWWWWWWWWWWWWW.',
	'.WWWWWWWWWWWWWWWWWWWW.',
	'xWWWWWWWWWWWWWWWWWWWx.',
	'xWWWWWWWWWWWWWWWWWWx..',
	'xWWWWWWWWWWWWWWWWWWx..',
	'..xWWWWWWWWWWWWWWWx...',
	'...xWWWWWWWWWWWWWx....',
	'....xWWWWx..xWWWx.....',
	'.....xx........xx.....',
];

function mirror(rows: string[]): string[] {
	return rows.map(r => r.split('').reverse().join(''));
}
const GHOST_L_OPEN = mirror(GHOST_R_OPEN);
const GHOST_L_SHUT = mirror(GHOST_R_SHUT);
const GHOST_L_WINKL = mirror(GHOST_R_WINKL);
const GHOST_L_WINKR = mirror(GHOST_R_WINKR);

const SPRITE_W = 22; // padded width for all frames

function pad(rows: string[], w: number): string[] {
	return rows.map(r => {
		if (r.length < w) return r + '.'.repeat(w - r.length);
		return r.slice(0, w);
	});
}

// ── Render a sprite row to ANSI ──────────────────────────────────────────
function renderRow(row: string): string {
	return row.split('').map(c => PAL[c] ?? PAL['.']).join('');
}

// ── App ──────────────────────────────────────────────────────────────────────
function App() {
	const [x, setX] = useState(10);
	const [y, setY] = useState(5);
	const [facing, setFacing] = useState<'r' | 'l'>('r');
	const frame = useFrames(30);

	useInput((ch, key) => {
		if (key.leftArrow) { setX(p => Math.max(0, p - 1)); setFacing('l'); }
		if (key.rightArrow) { setX(p => p + 1); setFacing('r'); }
		if (key.upArrow) setY(p => Math.max(0, p - 1));
		if (key.downArrow) setY(p => p + 1);
	});

	const bob = Math.sin(frame * 0.1) > 0 ? 0 : 1;
	// Cycle: open(80f) → blink(6f) → open(80f) → winkL(20f) → open(80f) → winkR(20f)
	const cycle = frame % 286;
	let eyes: 'open' | 'shut' | 'winkl' | 'winkr' = 'open';
	if (cycle >= 80 && cycle < 86) eyes = 'shut';
	else if (cycle >= 166 && cycle < 186) eyes = 'winkl';
	else if (cycle >= 266 && cycle < 286) eyes = 'winkr';

	const sprites = facing === 'r'
		? { open: GHOST_R_OPEN, shut: GHOST_R_SHUT, winkl: GHOST_R_WINKL, winkr: GHOST_R_WINKR }
		: { open: GHOST_L_OPEN, shut: GHOST_L_SHUT, winkl: GHOST_L_WINKL, winkr: GHOST_L_WINKR };
	const sprite = pad(sprites[eyes], SPRITE_W);

	const lines: string[] = [];
	// blank rows above ghost
	for (let r = 0; r < y + bob; r++) lines.push('');
	// ghost rows with x offset
	const indent = '.'.repeat(x);
	for (const row of sprite) lines.push(renderRow(indent + row));

	return <Text>{lines.join('\n')}</Text>;
}

render(<App />);
