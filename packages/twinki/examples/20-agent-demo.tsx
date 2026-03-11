/**
 * 20-agent-demo.tsx — Agentic character experience
 *
 * Run: npx tsx examples/20-agent-demo.tsx
 *
 * Demonstrates the full animation + overlay stack:
 *   - Character sprite rendered with halfBlock/stamp/renderGrid from twinki
 *   - Floating speech bubble via useOverlay
 *   - Simulated tool-call thinking states
 *   - Autonomous idle drift when not interacting
 *   - Click or Enter to trigger a new interaction
 *
 * To use your own character:
 *   1. Run: npx tsx scripts/sprite-extract.ts your-image.png 32 32
 *   2. Paste the output sprite + palette entries here
 *   3. Replace AGENT_FRAMES with your frames
 */
import React from 'react';
import {
	render, Text, Box,
	useApp, useInput, useFrames, useStdout, useMouse, useOverlay,
	halfBlock, stamp, renderGrid, createGrid, radialGlow,
	COLOR_RGB,
} from 'twinki';
import type { RGB, MouseEvent, OverlayHandle } from 'twinki';

// ── Character sprites (Kiro ghost — extracted from kiro-ghost.mov) ────────────

const GHOST_A: string[] = [
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
const GHOST_C: string[] = [
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
const GHOST_E: string[] = [
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

// ── Agent interactions ────────────────────────────────────────────────────────

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

	const pos = React.useRef({
		x: Math.floor((SCENE_W - SPRITE_W) / 2),
		y: Math.floor((SCENE_H - SPRITE_H) / 2),
		facing: 1, vx: 0, vy: 0,
	});
	const lastInput = React.useRef(0);
	const chat = React.useRef({
		state: 'idle' as ChatState, stateFrame: 0, idx: 0,
		typedChars: 0, toolIdx: 0, respondedChars: 0,
	});

	function triggerChat() {
		const c = chat.current;
		if (c.state !== 'idle' && c.state !== 'done') return;
		c.idx = (c.idx + 1) % INTERACTIONS.length;
		c.state = 'typing'; c.stateFrame = frame;
		c.typedChars = 0; c.toolIdx = 0; c.respondedChars = 0;
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
		triggerChat();
	});

	// Movement
	if (pos.current.vx !== 0) {
		pos.current.x = Math.max(0, Math.min(SCENE_W - SPRITE_W, pos.current.x + pos.current.vx));
		pos.current.vx = 0;
	}
	if (pos.current.vy !== 0) {
		pos.current.y = Math.max(0, Math.min(SCENE_H - SPRITE_H, pos.current.y + pos.current.vy));
		pos.current.vy = 0;
	}

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

	// ── Render ────────────────────────────────────────────────────────────────

	const t = (frame % 180) / 180 * Math.PI * 2;
	const sin = Math.sin(t), cos = Math.cos(t);
	const bobPx = Math.round((1 - sin) * 3);
	const wiggle = c.state === 'typing' && elapsed < 20 ? Math.sin(elapsed * 1.5) * 2 : 0;

	const sprite = cos > 0.3 ? GHOST_A : cos < -0.3 ? GHOST_E : GHOST_C;
	const facedSprite = pos.current.facing === -1 ? sprite.map(r => [...r].reverse().join('')) : sprite;

	const grid = createGrid(SCENE_W, SCENE_H);
	stamp(grid, facedSprite, Math.round(pos.current.x + wiggle), pos.current.y + bobPx);

	// Background glow
	const bgCache = React.useRef<{ x: number; y: number; data: RGB[][] } | null>(null);
	const { x: gx, y: gy } = pos.current;
	if (!bgCache.current || bgCache.current.x !== gx || bgCache.current.y !== gy) {
		bgCache.current = {
			x: gx, y: gy,
			data: radialGlow(SCENE_W, SCENE_H, gx + SPRITE_W / 2, gy + SPRITE_H / 2,
				[18, 14, 28], [40, 30, 58], 50),
		};
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
			if (line.length + w.length + 1 > 40) { bubbleLines.push(line); line = w; }
			else line = line ? line + ' ' + w : w;
		}
		if (line) bubbleLines.push(line);
	}

	// Speech bubble via useOverlay — floats over the canvas
	const bubbleRef = React.useRef<OverlayHandle | null>(null);

	const overlayRow = Math.max(0, Math.floor(pos.current.y / 2) - 5);
	const overlayCol = Math.max(0, Math.floor(pos.current.x / 2));

	const showBubbleOverlay = useOverlay(
		() => {
			const maxW = bubbleLines.length > 0 ? Math.max(...bubbleLines.map(l => l.length), 5) : 5;
			const padStr = (s: string) => s + ' '.repeat(maxW - s.length);
			return (
				<Box flexDirection="column">
					<Box borderStyle="round" borderColor={bubbleColor} paddingX={1}>
						<Box flexDirection="column" width={maxW + 2}>
							{bubbleLines.length > 0
								? bubbleLines.map((l, i) => <Text key={i} color={bubbleColor}>{padStr(l)}</Text>)
								: <Text color={bubbleColor}>···</Text>}
						</Box>
					</Box>
					<Text color={bubbleColor}>  ▼</Text>
				</Box>
			);
		},
		{ row: overlayRow, col: overlayCol },
	);

	// Show/hide overlay based on chat state
	React.useEffect(() => {
		if (showBubble && bubbleLines.length > 0) {
			bubbleRef.current?.hide();
			bubbleRef.current = showBubbleOverlay();
		} else if (!showBubble && bubbleRef.current) {
			bubbleRef.current.hide();
			bubbleRef.current = null;
		}
	});

	return (
		<Box flexDirection="column">
			<Text dimColor> Enter/click = chat   ← → ↑ ↓ = move   q = quit</Text>
			<Text>{renderGrid(grid, bgCache.current.data)}</Text>
		</Box>
	);
}

render(<App />);
