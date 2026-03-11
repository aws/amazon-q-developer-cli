/**
 * 19-kiro-ghost-chat.tsx — Kiro ghost chat experience
 *
 * Run: npx tsx examples/19-kiro-ghost-chat.tsx
 *
 * Click or press Enter to trigger chat. Ghost simulates tool calls,
 * thinking states, and streaming responses with speech bubble.
 */
import React from 'react';
import { render, Text, Box, useApp, useInput, useFrames, useStdout, useMouse } from 'twinki';
import type { MouseEvent } from 'twinki';

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

// ── Chat simulation ───────────────────────────────────────────────────────────

const SPINNER = ['⠋','⠙','⠹','⠸','⠼','⠴','⠦','⠧','⠇','⠏'];

const INTERACTIONS = [
	{ prompt: 'Hey Kiro, any easter eggs?',   tools: ['Searching codebase...','Reading context...','Thinking...'],       response: 'Found one! Try the Konami code in the editor. ↑↑↓↓←→←→BA' },
	{ prompt: 'What are you building?',        tools: ['Scanning files...','Analyzing project...','Generating...'],       response: 'A flicker-free TUI renderer. Differential updates, zero tearing.' },
	{ prompt: 'Can you help me debug this?',   tools: ['Reading error...','Searching symbols...','Reasoning...'],         response: 'Looks like a missing await. Line 42 — async boundary issue.' },
	{ prompt: 'Show me something cool',        tools: ['Exploring...','Fetching examples...','Composing...'],             response: 'Check examples/17-pura-vida.tsx — full platformer in the terminal!' },
];

type ChatState = 'idle' | 'typing' | 'thinking' | 'responding' | 'done';

// ── App ───────────────────────────────────────────────────────────────────────

function App() {
	const { exit } = useApp();
	const { stdout } = useStdout();
	const frame = useFrames(60);

	const SCENE_W = stdout.columns || 80;
	const SCENE_H = ((stdout.rows || 24) - 3) * 2;

	const pos = React.useRef({ x: Math.floor((SCENE_W - SPRITE_W) / 2), y: Math.floor((SCENE_H - SPRITE_H) / 2), facing: 1, vx: 0, vy: 0 });
	const lastInput = React.useRef(0);
	const chat = React.useRef({ state: 'idle' as ChatState, stateFrame: 0, idx: 0, typedChars: 0, toolIdx: 0, respondedChars: 0 });

	function triggerChat() {
		const c = chat.current;
		if (c.state !== 'idle' && c.state !== 'done') return;
		c.idx = (c.idx + 1) % INTERACTIONS.length;
		c.state = 'typing'; c.stateFrame = frame; c.typedChars = 0; c.toolIdx = 0; c.respondedChars = 0;
	}

	useInput((_ch, key) => {
		if (_ch === 'q') { exit(); return; }
		const SPEED = 4;
		if (key.leftArrow)  { pos.current.vx = -SPEED; pos.current.facing = -1; lastInput.current = frame; }
		if (key.rightArrow) { pos.current.vx =  SPEED; pos.current.facing =  1; lastInput.current = frame; }
		if (key.upArrow)    { pos.current.vy = -SPEED; lastInput.current = frame; }
		if (key.downArrow)  { pos.current.vy =  SPEED; lastInput.current = frame; }
		if (key.return) triggerChat();
	});

	useMouse((e: MouseEvent) => {
		if (e.type !== 'mousedown' || e.button !== 'left') return;
		const px = e.x, py = e.y * 2 - 2;
		const { x, y } = pos.current;
		if (px >= x / 2 && px <= (x + SPRITE_W) / 2 && py >= y && py <= y + SPRITE_H) triggerChat();
	});

	// Movement
	if (pos.current.vx !== 0) { pos.current.x = Math.max(0, Math.min(SCENE_W - SPRITE_W, pos.current.x + pos.current.vx)); pos.current.vx = 0; }
	if (pos.current.vy !== 0) { pos.current.y = Math.max(0, Math.min(SCENE_H - SPRITE_H, pos.current.y + pos.current.vy)); pos.current.vy = 0; }

	// Autonomous drift when idle
	if (frame - lastInput.current > 180 && chat.current.state === 'idle') {
		const dt = frame * 0.015;
		pos.current.x = Math.round(Math.max(0, Math.min(SCENE_W - SPRITE_W, pos.current.x + Math.sin(dt * 1.3) * 0.6)));
		pos.current.y = Math.round(Math.max(0, Math.min(SCENE_H - SPRITE_H, pos.current.y + Math.sin(dt * 0.7) * 0.4)));
	}

	// Chat state machine
	const c = chat.current;
	const interaction = INTERACTIONS[c.idx]!;
	const elapsed = frame - c.stateFrame;

	if (c.state === 'typing') {
		c.typedChars = Math.min(interaction.prompt.length, Math.floor(elapsed / 3));
		if (elapsed > interaction.prompt.length * 3 + 20) { c.state = 'thinking'; c.stateFrame = frame; }
	} else if (c.state === 'thinking') {
		c.toolIdx = Math.min(interaction.tools.length - 1, Math.floor(elapsed / 60));
		if (elapsed > interaction.tools.length * 60) { c.state = 'responding'; c.stateFrame = frame; }
	} else if (c.state === 'responding') {
		c.respondedChars = Math.min(interaction.response.length, Math.floor(elapsed / 2));
		if (c.respondedChars >= interaction.response.length) { c.state = 'done'; c.stateFrame = frame; }
	} else if (c.state === 'done' && elapsed > 300) {
		c.state = 'idle';
	}

	// Ghost sprite
	const t = (frame % 180) / 180 * Math.PI * 2;
	const sin = Math.sin(t), cos = Math.cos(t);
	const wiggle = c.state === 'typing' && elapsed < 20 ? Math.sin(elapsed * 1.5) * 2 : 0;
	const sprite = cos > 0.7 ? GHOST_A : cos > 0.2 ? GHOST_B : cos < -0.7 ? GHOST_E : cos < -0.2 ? GHOST_D : GHOST_C;
	const facedSprite = pos.current.facing === -1 ? sprite.map(r => [...r].reverse().join('')) : sprite;
	const bobPx = Math.round((1 - sin) * 3);

	const grid: string[][] = Array.from({ length: SCENE_H }, () => Array(SCENE_W).fill('.'));
	stamp(grid, facedSprite, Math.round(pos.current.x + wiggle), pos.current.y + bobPx);

	const bgCache = React.useRef<{ x: number; y: number; data: RGB[][] } | null>(null);
	const { x: gx, y: gy } = pos.current;
	if (!bgCache.current || bgCache.current.x !== gx || bgCache.current.y !== gy) {
		const d: RGB[][] = [];
		const cx = gx + SPRITE_W / 2, cy = gy + SPRITE_H / 2, glowR = 50 * 50;
		for (let y = 0; y < SCENE_H; y++) {
			const row: RGB[] = []; const dy2 = (y - cy) ** 2;
			for (let x = 0; x < SCENE_W; x++) {
				const g = Math.max(0, 1 - ((x - cx) ** 2 + dy2) / glowR);
				row.push([Math.round(18 + g * 22), Math.round(14 + g * 16), Math.round(28 + g * 30)]);
			}
			d.push(row);
		}
		bgCache.current = { x: gx, y: gy, data: d };
	}

	// Speech bubble content
	const showBubble = c.state !== 'idle';
	let bubbleLines: string[] = [];
	let bubbleColor = 'white';

	if (c.state === 'typing') {
		bubbleLines = ['> ' + interaction.prompt.slice(0, c.typedChars) + (frame % 6 < 3 ? '▋' : ' ')];
		bubbleColor = 'cyan';
	} else if (c.state === 'thinking') {
		bubbleLines = [`${SPINNER[frame % SPINNER.length]} ${interaction.tools[c.toolIdx]}`];
		bubbleColor = 'yellow';
	} else if (c.state === 'responding' || c.state === 'done') {
		const text = interaction.response.slice(0, c.respondedChars);
		const words = text.split(' '); let line = '';
		for (const w of words) {
			if (line.length + w.length + 1 > 32) { bubbleLines.push(line); line = w; }
			else line = line ? line + ' ' + w : w;
		}
		if (line) bubbleLines.push(line);
	}

	// Bubble position (unused now — computed inline below)


	const canvasStr = renderGrid(grid, bgCache.current!.data);

	// Bubble rendered in a fixed 4-line area above the canvas
	const maxW = bubbleLines.length > 0 ? Math.max(...bubbleLines.map(l => l.length), 3) : 3;
	const pad = (s: string) => s + ' '.repeat(maxW - s.length);
	const ansiColor = bubbleColor === 'yellow' ? '\x1b[33m' : bubbleColor === 'cyan' ? '\x1b[36m' : '\x1b[37m';
	const rst = '\x1b[0m';
	const bubbleArea: string[] = showBubble && bubbleLines.length > 0 ? [
		`${ansiColor}╭${'─'.repeat(maxW + 2)}╮${rst}`,
		...bubbleLines.map(l => `${ansiColor}│ ${pad(l)} │${rst}`),
		`${ansiColor}╰${'─'.repeat(maxW + 2)}╯${rst}`,
		`${ansiColor}  ▼${rst}`,
	] : ['', '', '', ''];

	return (
		<Box flexDirection="column">
			<Text dimColor> click ghost or Enter to chat   ← → ↑ ↓ move   q quit</Text>
			<Text>{bubbleArea.join('\n')}</Text>
			<Text>{canvasStr}</Text>
		</Box>
	);
}

render(<App />);
