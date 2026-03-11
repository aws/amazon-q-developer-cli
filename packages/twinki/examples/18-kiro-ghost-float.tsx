/**
 * 18-kiro-ghost-float.tsx — Kiro ghost floating animation
 *
 * Run: npx tsx examples/18-kiro-ghost-float.tsx
 *
 * Sprite extracted from kiro-ghost.mov via extract-sprite.py (32×32).
 * Half-block ▀ renderer (same as 17-pura-vida): each char = 2 pixel rows.
 * Ghost bobs on a sine wave; skirt bottom shifts with bob direction.
 * Background is a dark gradient — ghost '.' pixels sample it (transparent).
 */
import React from 'react';
import { render, Text, Box, useApp, useInput, useFrames, useStdout } from 'twinki';

// ── Half-block renderer (identical to pura-vida) ──────────────────────────────

const ESC = '\x1b[';
const RST = `${ESC}0m`;
type RGB = [number, number, number];

const halfBlock = (top: RGB, bottom: RGB) =>
	`${ESC}38;2;${top[0]};${top[1]};${top[2]};48;2;${bottom[0]};${bottom[1]};${bottom[2]}m▀${RST}`;

// ── Palette ───────────────────────────────────────────────────────────────────

const COLOR_RGB: Record<string, RGB> = {
	'W': [252, 252, 252],  // ghost body white
	'x': [140, 138, 144],  // anti-alias edge
	'K': [8,   6,   10],   // eyes / dark outline
};

// ── Sprites extracted from all 5 distinct frames in kiro-ghost.mov (32×32) ────
// A=highest/flattest skirt (bot=238) → E=lowest/most drooped (bot=247)

const GHOST_A = [
	'...........KKxxWWWWWWxK.K.......',
	'.........KxWWWWWWWWWWWWWx.......',
	'.........xWWWWWWWWWWWWWWWWxK....',
	'.......KWWWWWWWWWWWWWWWWWWWxK...',
	'......KWWWWWWWWWWWWWWWWWWWWWxK..',
	'......WWWWWWWWWWWWWWWWWWWWWWWxK.',
	'....KxWWWWWWWWWWWWWWWWWWWWWWWW..',
	'.....WWWWWWWWWWWWWWWWWWWWWWWWWxK',
	'...KxWWWWWWWWWWWWWWWWWWWWWWWWWW.',
	'...KWWWWWWWWWWWxxWWWWWxWWWWWWWWx',
	'....WWWWWWWWWWxKKxWWWKK.WWWWWWWx',
	'...KWWWWWWWWWWKKKKWWxKKKWWWWWWWW',
	'...xWWWWWWWWWWKKK.WWxKKKWWWWWWWW',
	'..KxWWWWWWWWWWxKKxWWWKK.WWWWWWWW',
	'..KxWWWWWWWWWWWWxWWWWWxWWWWWWWWW',
	'..KxWWWWWWWWWWWWWWWWWWWWWWWWWWWW',
	'...WWWWWWWWWWWWWWWWWWWWWWWWWWWWW',
	'..xWWWWWWWWWWWWWWWWWWWWWWWWWWWWW',
	'.KWWWWWWWWWWWWWWWWWWWWWWWWWWWWWx',
	'KxWWWWWWWWWWWWWWWWWWWWWWWWWWWWWx',
	'KWWWWWWWWWWWWWWWWWWWWWWWWWWWWWWK',
	'WWWWWWWWWWWWWWWWWWWWWWWWWWWWWWW.',
	'WWWWWWWWWWWWWWWWWWWWWWWWWWWWWWxK',
	'xWWWWWWWWWWWWWWWWWWWWWWWWWWWWWK.',
	'.xxxxxWWWWWWWWWWWWWWWWWWWWWWWxK.',
	'.KKKKxWWWWWWWWWWWWWWWWWWWWWWWK..',
	'....KWWWWWWWWWWWWWWWWWWWWWWWxK..',
	'....xWWWWWWWWWWWWWWWWWWWWWWW....',
	'....KWWWWWWWWWWWWWWWWWWWWWW.....',
	'....KxWWWWWWWWWWWWWWWWWWWx......',
	'......xWWWWWWWx.KWWWWWWWxK......',
	'......K.xxxxK.K..KWWWWx.K.......',
];
const GHOST_B = [
	'........KKxxWWWWWxxKK...........',
	'......KxWWWWWWWWWWWWWxK.........',
	'.....KWWWWWWWWWWWWWWWWW.........',
	'...KxWWWWWWWWWWWWWWWWWWWKK......',
	'..KxWWWWWWWWWWWWWWWWWWWWWK......',
	'..KWWWWWWWWWWWWWWWWWWWWWWW......',
	'.KWWWWWWWWWWWWWWWWWWWWWWWWxK....',
	'KxWWWWWWWWWWWWWWWWWWWWWWWWWK....',
	'KWWWWWxK.WWWxKKWWWWWWWWWWWWxK...',
	'KWWWWWKKKxWW.KKxWWWWWWWWWWWW....',
	'xWWWWWKKKxWW.KKKWWWWWWWWWWWWK...',
	'xWWWWWKKKWWWxKKxWWWWWWWWWWWWx...',
	'WWWWWWWxxWWWWxxWWWWWWWWWWWWWxK..',
	'WWWWWWWWWWWWWWWWWWWWWWWWWWWWxK..',
	'WWWWWWWWWWWWWWWWWWWxxWWWWWWWWK..',
	'WWWWWWWWWWWWWWWWWWWWWWWWWWWWW...',
	'WWWWWWWWWWWWWWWWWWWWWWWWWWWWW...',
	'WWWWWWWWWWWWWWWWWWWWWWWWWWWWWx..',
	'xWWWWWWWWWWWWWWWWWWWWWWWWWWWWxK.',
	'xWWWWWWWWWWWWWWWWWWWWWWWWWWWWWK.',
	'KWWWWWWWWWWWWWWWWWWWWWWWWWWWWWWK',
	'.WWWWWWWWWWWWWWWWWWWWWWWWWWWWWWx',
	'KxWWWWWWWWWWWWWWWWWWWWWWWWWWWWWW',
	'.KWWWWWWWWWWWWWWWWWWWWWWWWWWWWWW',
	'.KxWWWWWWWWWWWWWWWWWWWWWWWWWWWWx',
	'..KWWWWWWWWWWWWWWWWWWWWWWWxKxxK.',
	'..KxWWWWWWWWWWWWWWWWWWWWWWxK.K..',
	'....WWWWWWWWWWWWWWWWWWWWWWW.....',
	'....KWWWWWWWWWWWWWWWWWWWWWW.....',
	'......WWWWWWWWWWWWWWWWWWWWxK....',
	'.......xWWWWWWxKxWWWWWWWWW......',
	'.......KKxWWWx..K.xxWWWWx.......',
];
const GHOST_C = [
	'..........KKKxxWWWWWxxKK........',
	'.........KxWWWWWWWWWWWWWxK......',
	'........KWWWWWWWWWWWWWWWWWKK....',
	'......KxWWWWWWWWWWWWWWWWWWWxK...',
	'......xWWWWWWWWWWWWWWWWWWWWWxK..',
	'......WWWWWWWWWWWWWWWWWWWWWWWK..',
	'....KxWWWWWWWWWWWWWWWWWWWWWWWW..',
	'....KWWWWWWWWWWWWWWWWWWWWWWWWWxK',
	'...KxWWWWWWWWWWWWWWWWWWWWWWWWWWK',
	'....WWWWWWWWWWWWWWWWWWWWWWWWWWWK',
	'...KWWWWWWWWWWxxWWWWxxxWWWWWWWWx',
	'...xWWWWWWWWWxKKxWWWKKKWWWWWWWWx',
	'..KxWWWWWWWWWKKK.WWxKKKWWWWWWWWW',
	'..KxWWWWWWWWWKKKKWWxKKKWWWWWWWWW',
	'..KWWWWWWWWWWWKKxWWWKKxWWWWWWWWW',
	'...WWWWWWWWWWWWWWWWWWWWWWWWWWWWW',
	'...WWWWWWWWWWWWWWWWWWWWWWWWWWWWW',
	'..xWWWWWWWWWWWWWWWWWWWWWWWWWWWWW',
	'.KxWWWWWWWWWWWWWWWWWWWWWWWWWWWWx',
	'.KWWWWWWWWWWWWWWWWWWWWWWWWWWWWWx',
	'KxWWWWWWWWWWWWWWWWWWWWWWWWWWWWWK',
	'xWWWWWWWWWWWWWWWWWWWWWWWWWWWWWW.',
	'WWWWWWWWWWWWWWWWWWWWWWWWxWWWWWxK',
	'WWWWWWWWWWWWWWWWWWWWWWWWWWWWWWK.',
	'xWWWWWWWWWWWWWWWWWWWWWWWWWWWWxK.',
	'.KxxKKWWWWWWWWWWWWWWWWWWWWWWWK..',
	'..K.KxWWWWWWWWWWWWWWWWWWWWWWxK..',
	'....KWWWWWWWWWWWWWWWWWWWWWWW....',
	'....KWWWWWWWWWWWWWWWWWWWWWW.....',
	'....KxWWWWWWWWWWWWWWWWWWWW......',
	'......xWWWWWWWWxKxWWWWWWx.......',
	'......KxxWWWxx.K..xWWWx.K.......',
];
const GHOST_D = [
	'...........KKxxWWWWWWxK.K.......',
	'.........KKxWWWWWWWWWWWWx.......',
	'.........xWWWWWWWWWWWWWWWWxK....',
	'........WWWWWWWWWWWWWWWWWWWxK...',
	'.......WWWWWWWWWWWWWWWWWWWWWxK..',
	'.....KWWWWWWWWWWWWWWWWWWWWWWWxK.',
	'....KxWWWWWWWWWWWWWWWWWWWWWWWW..',
	'.....WWWWWWWWWWWWWWWWWWWWWWWWWxK',
	'....KWWWWWWWWWWWWWWWWWWWWWWWWWW.',
	'...KxWWWWWWWWWWWWWWWWWWWWWWWWWWx',
	'....WWWWWWWWWWxKWWWWxKxWWWWWWWWx',
	'....WWWWWWWWWx.KKWWWKKKWWWWWWWWW',
	'...KWWWWWWWWWxKxKWWWKKKxWWWWWWWW',
	'...xWWWWWWWWWx.KKWWWKKKxWWWWWWWW',
	'..KxWWWWWWWWWWKKxWWWxKKWWWWWWWWW',
	'..KxWWWWWWWWWWWWWWWWWWWWWWWWWWWW',
	'..KWWWWWWWWWWWWWWWWWWWWWWWWWWWWW',
	'..KWWWWWWWWWWWWWWWWWWWWWWWWWWWWW',
	'.KWWWWWWWWWWWWWWWWWWWWWWWWWWWWWx',
	'KxWWWWWWWWWWWWWWWWWWWWWWWWWWWWWx',
	'xWWWWWWWWWWWWWWWWWWWWWWWWWWWWWWK',
	'WWWWWWWWWWWWWWWWWWWWWWWWWWWWWWW.',
	'WWWWWWWWWWWWWWWWWWWWWWWWWWWWWWxK',
	'xWWWWWWWWWWWWWWWWWWWWWWWWWWWWWK.',
	'..xxKxWWWWWWWWWWWWWWWWWWWWWWWxK.',
	'..K.KWWWWWWWWWWWWWWWWWWWWWWWW...',
	'....KWWWWWWWWWWWWWWWWWWWWWWWxK..',
	'....xWWWWWWWWWWWWWWWWWWWWWWxK...',
	'....KWWWWWWWWWWWWWWWWWWWWWx.....',
	'....KxWWWWWWWWWWWWWWWWWWWxK.....',
	'......xWWWWWWxKKKWWWWWWWKK......',
	'......KK.KK..K...KWWWxxK........',
];
const GHOST_E = [
	'............K.xxWWWWWxxKK.......',
	'..........KKxWWWWWWWWWWWxKK.....',
	'.........KxWWWWWWWWWWWWWWWx.....',
	'........KxWWWWWWWWWWWWWWWWWW....',
	'.......KxWWWWWWWWWWWWWWWWWWWW...',
	'.......xWWWWWWWWWWWWWWWWWWWWWxK.',
	'.......WWWWWWWWWWWWWWWWWWWWWWWK.',
	'.....KxWWWWWWWWWWWWWWWWWWWWWWWWK',
	'.....KxWWWWWWWWWWWWWWWWWWWWWWWWK',
	'......WWWWWWWWWWWWWWWWWWWWWWWWWx',
	'.....KWWWWWWWWWx.xWWWK.xWWWWWWWx',
	'....KxWWWWWWWWxKK.WWxKKKWWWWWWWW',
	'....KxWWWWWWWWxKK.WWxKK.WWWWWWWW',
	'....KxWWWWWWWWWKK.WWxKKKWWWWWWWW',
	'....KWWWWWWWWWWxKWWWWxKWWWWWWWWW',
	'.....WWWWWWWWWWWWWWWWWWWWWWWWWWW',
	'....KWWWWWWWWWWWWWWWWWWWWWWWWWWW',
	'...KWWWWWWWWWWWWWWWWWWWWWWWWWWWW',
	'..KxWWWWWWWWWWWWWWWWWWWWWWWWWWWx',
	'...WWWWWWWWWWWWWWWWWWWWWWWWWWWWx',
	'.KxWWWWWWWWWWWWWWWWWWWWWWWWWWWWK',
	'..WWWWWWWWWWWWWWWWWWWWWWWWWWWWWK',
	'..WWWWWWWWWWWWWWWWWWWWWWWWWWWWxK',
	'..KxWWxWWWWWWWWWWWWWWWWWWWWWWW..',
	'...K.K.WWWWWWWWWWWWWWWWWWWWWWxK.',
	'.....KxWWWWWWWWWWWWWWWWWWWWWW...',
	'.....KxWWWWWWWWWWWWWWWWWWWWWK...',
	'.....KxWWWWWWWWWWWWWWWWWWWWxK...',
	'......KWWWWWWWWWWWWWWWWWWWxK....',
	'.......xWWWWWWWx.xWWWWWWWKK.....',
	'.........xxxxK.K..xWWWWx........',
	'x........KKKK.....K.K.KK........',
];

const SPRITE_W = 32;
const SPRITE_H = 32;

// ── Grid helpers (same pattern as pura-vida) ──────────────────────────────────

function stamp(grid: string[][], sprite: string[], x: number, y: number) {
	for (let r = 0; r < sprite.length; r++) {
		const row = y + r;
		if (row < 0 || row >= grid.length) continue;
		for (let c = 0; c < sprite[r]!.length; c++) {
			const col = x + c;
			if (col < 0 || col >= grid[0]!.length) continue;
			if (sprite[r]![c] !== '.') grid[row]![col] = sprite[r]![c]!;
		}
	}
}

function renderGrid(grid: string[][], bgRgb: RGB[][]): string {
	const rows: string[] = [];
	for (let y = 0; y < grid.length; y += 2) {
		const topRow = grid[y]!;
		const botRow = grid[y + 1] ?? [];
		let line = '';
		for (let x = 0; x < topRow.length; x++) {
			const tc = topRow[x]!;
			const bc = botRow[x] ?? '.';
			const top: RGB = tc === '.' ? bgRgb[y]![x]! : (COLOR_RGB[tc] ?? bgRgb[y]![x]!);
			const bot: RGB = bc === '.' ? (bgRgb[y + 1]?.[x] ?? bgRgb[y]![x]!) : (COLOR_RGB[bc] ?? bgRgb[y]![x]!);
			line += halfBlock(top, bot);
		}
		rows.push(line);
	}
	return rows.join('\n');
}

// ── App ───────────────────────────────────────────────────────────────────────

function App() {
	const { exit } = useApp();
	const { stdout } = useStdout();
	const frame = useFrames(60);

	// Scene fills terminal — pixel rows = (termRows-3)*2, pixel cols = termCols
	const SCENE_W = stdout.columns || 80;
	const SCENE_H = ((stdout.rows || 24) - 3) * 2;

	// Ghost position + facing direction
	const pos = React.useRef({ x: Math.floor((SCENE_W - SPRITE_W) / 2), y: Math.floor((SCENE_H - SPRITE_H) / 2), facing: 1, vx: 0, vy: 0 });

	useInput((_ch, key) => {
		if (_ch === 'q') { exit(); return; }
		const SPEED = 4;
		if (key.leftArrow)  { pos.current.vx = -SPEED; pos.current.facing = -1; }
		if (key.rightArrow) { pos.current.vx =  SPEED; pos.current.facing =  1; }
		if (key.upArrow)    { pos.current.vy = -SPEED; }
		if (key.downArrow)  { pos.current.vy =  SPEED; }
	});

	// Apply velocity every frame then clear it — re-set each keypress
	const lastInput = React.useRef(0);
	if (pos.current.vx !== 0 || pos.current.vy !== 0) lastInput.current = frame;

	if (pos.current.vx !== 0) {
		pos.current.x = Math.max(0, Math.min(SCENE_W - SPRITE_W, pos.current.x + pos.current.vx));
		pos.current.vx = 0;
	}
	if (pos.current.vy !== 0) {
		pos.current.y = Math.max(0, Math.min(SCENE_H - SPRITE_H, pos.current.y + pos.current.vy));
		pos.current.vy = 0;
	}

	// Autonomous drift when idle for >3s — slow sine wander
	const idle = frame - lastInput.current > 180;
	if (idle) {
		const dt = frame * 0.015;
		pos.current.x = Math.round(Math.max(0, Math.min(SCENE_W - SPRITE_W,
			pos.current.x + Math.sin(dt * 1.3) * 0.6)));
		pos.current.y = Math.round(Math.max(0, Math.min(SCENE_H - SPRITE_H,
			pos.current.y + Math.sin(dt * 0.7) * 0.4)));
	}

	// Sine bob — period 180 frames (~3s at 60fps, matches video)
	const t = (frame % 180) / 180 * Math.PI * 2;
	const sin = Math.sin(t);
	const cos = Math.cos(t);

	const sprite = cos > 0.7 ? GHOST_A
		: cos > 0.2 ? GHOST_B
		: cos < -0.7 ? GHOST_E
		: cos < -0.2 ? GHOST_D
		: GHOST_C;
	const facedSprite = pos.current.facing === -1
		? sprite.map(r => [...r].reverse().join(''))
		: sprite;
	const bobPx = Math.round((1 - sin) * 3); // 0..6 pixel rows

	// Build grid
	const grid: string[][] = Array.from({ length: SCENE_H }, () => Array(SCENE_W).fill('.'));
	stamp(grid, facedSprite, pos.current.x, pos.current.y + bobPx);

	// Background: recompute only when ghost position changes
	const bgCache = React.useRef<{ x: number; y: number; data: RGB[][] } | null>(null);
	const { x: gx, y: gy } = pos.current;
	if (!bgCache.current || bgCache.current.x !== gx || bgCache.current.y !== gy) {
		const bgRgbNew: RGB[][] = [];
		const cx = gx + SPRITE_W / 2;
		const cy = gy + SPRITE_H / 2;
		const glowR = 50 * 50;
		for (let y = 0; y < SCENE_H; y++) {
			const row: RGB[] = [];
			const dy2 = (y - cy) ** 2;
			for (let x = 0; x < SCENE_W; x++) {
				const dist2 = (x - cx) ** 2 + dy2;
				const glow = Math.max(0, 1 - dist2 / glowR);
				row.push([Math.round(18 + glow * 22), Math.round(14 + glow * 16), Math.round(28 + glow * 30)]);
			}
			bgRgbNew.push(row);
		}
		bgCache.current = { x: gx, y: gy, data: bgRgbNew };
	}
	const bgRgb = bgCache.current.data;

	return (
		<Box flexDirection="column">
			<Text dimColor> ← → ↑ ↓ move   q quit</Text>
			<Text>{renderGrid(grid, bgRgb)}</Text>
		</Box>
	);
}

render(<App />);
