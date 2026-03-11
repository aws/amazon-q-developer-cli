/**
 * Animation utilities for half-block pixel rendering.
 *
 * Provides the core primitives used by pura-vida and kiro-ghost examples,
 * extracted into a reusable module so any example or app can import them.
 *
 * ## Rendering model
 * Each terminal character cell represents 2 vertical pixels using the
 * Unicode UPPER HALF BLOCK (▀) character:
 *   - Foreground color = top pixel
 *   - Background color = bottom pixel
 *
 * This gives an effective resolution of `cols × (rows*2)` pixels.
 *
 * ## Usage
 * ```ts
 * import { halfBlock, stamp, renderGrid, COLOR_RGB } from 'twinki/animation';
 *
 * // Build a pixel grid
 * const grid = Array.from({ length: 32 }, () => Array(32).fill('.'));
 *
 * // Stamp a sprite (char map) onto the grid
 * stamp(grid, MY_SPRITE, x, y);
 *
 * // Render to ANSI string (with optional background)
 * const output = renderGrid(grid, bgRgb);
 * ```
 */

/** RGB color triple */
export type RGB = [number, number, number];

/** A 2D grid of palette chars — '.' means transparent */
export type SpriteGrid = string[][];

const ESC = '\x1b[';
const RST = `${ESC}0m`;

/**
 * Render one terminal character representing two vertical pixels.
 * Uses UPPER HALF BLOCK ▀ with true-color fg (top) and bg (bottom).
 */
export function halfBlock(top: RGB, bottom: RGB): string {
	return `${ESC}38;2;${top[0]};${top[1]};${top[2]};48;2;${bottom[0]};${bottom[1]};${bottom[2]}m▀${RST}`;
}

/**
 * Default palette mapping single chars to RGB colors.
 * '.' is reserved for transparent (not in this map).
 * Extend or replace with your own palette.
 */
export const COLOR_RGB: Record<string, RGB> = {
	// Ghost / UI
	'W': [252, 252, 252],  // white body
	'w': [210, 208, 218],  // soft edge
	'x': [140, 138, 144],  // anti-alias
	'K': [8,   6,   10],   // dark / eyes
	// Pura-vida character palette
	'R': [216,62,32],  'r': [149,35,35],
	'A': [45,43,152],
	'S': [241,203,104], 's': [228,134,50], 'k': [77,41,20],
	'B': [43,41,149],  'b': [25,25,90],
	'H': [138,67,22],  'h': [77,41,20],
	'D': [7,5,8],
	'G': [34,139,34],  'g': [0,100,0],
	'P': [0,128,0],    'p': [216,62,32],
	'l': [80,200,80],  'f': [0,128,0],
	'Y': [220,180,40], 'O': [180,120,40], 'o': [140,90,30],
	'T': [180,120,60],
	'E': [240,145,15], 'I': [220,100,5],  'J': [165,65,20],
	'L': [245,235,210],'M': [235,210,180],
	'N': [220,160,170],'Q': [190,130,140],
	'U': [170,170,170],'V': [140,140,140],'X': [250,250,245],
	'a': [214,187,137],'c': [191,161,123],'d': [172,141,109],
	'e': [166,122,80], 'i': [150,110,76], 'j': [129,106,97],
	'm': [133,88,60],  'q': [115,85,70],  't': [90,72,74],
	'u': [111,69,48],  'v': [89,55,40],   'y': [59,36,31],
	'C': [210,190,50], '9': [190,20,30],
	'z': [180,200,255],'Z': [230,240,255],
	'1': [50,80,210],  '2': [30,40,140],  '3': [220,50,30],  '4': [170,30,20],
	'5': [240,200,50], '6': [240,140,30], '7': [240,230,210],'8': [100,60,30],
	'F': [245,245,245],'n': [255,175,0],
};

/**
 * Stamp a sprite onto a pixel grid.
 * '.' chars in the sprite are transparent (grid cell unchanged).
 * Clips to grid bounds automatically.
 */
export function stamp(grid: SpriteGrid, sprite: string[], x: number, y: number): void {
	const h = grid.length;
	const w = grid[0]?.length ?? 0;
	for (let r = 0; r < sprite.length; r++) {
		const row = y + r;
		if (row < 0 || row >= h) continue;
		const spriteRow = sprite[r]!;
		for (let c = 0; c < spriteRow.length; c++) {
			const col = x + c;
			if (col < 0 || col >= w) continue;
			if (spriteRow[c] !== '.') grid[row]![col] = spriteRow[c]!;
		}
	}
}

/**
 * Render a pixel grid to an ANSI string using half-block characters.
 *
 * @param grid - 2D char grid. '.' = transparent (uses bgRgb or fallback).
 * @param bgRgb - Optional background color grid (same dimensions as grid).
 *                When provided, '.' pixels sample from this instead of black.
 * @param palette - Optional custom palette (defaults to COLOR_RGB).
 * @returns Multi-line ANSI string, one terminal row per 2 pixel rows.
 */
export function renderGrid(
	grid: SpriteGrid,
	bgRgb?: RGB[][],
	palette: Record<string, RGB> = COLOR_RGB,
): string {
	const fallback: RGB = [0, 0, 0];
	const rows: string[] = [];
	for (let y = 0; y < grid.length; y += 2) {
		const topRow = grid[y]!;
		const botRow = grid[y + 1] ?? [];
		let line = '';
		for (let x = 0; x < topRow.length; x++) {
			const tc = topRow[x]!;
			const bc = botRow[x] ?? '.';
			const top: RGB = tc === '.' ? (bgRgb?.[y]?.[x] ?? fallback) : (palette[tc] ?? fallback);
			const bot: RGB = bc === '.' ? (bgRgb?.[y + 1]?.[x] ?? bgRgb?.[y]?.[x] ?? fallback) : (palette[bc] ?? fallback);
			line += halfBlock(top, bot);
		}
		rows.push(line);
	}
	return rows.join('\n');
}

/**
 * Create an empty pixel grid filled with '.' (transparent).
 */
export function createGrid(width: number, height: number): SpriteGrid {
	return Array.from({ length: height }, () => Array(width).fill('.'));
}

/**
 * Create a solid background RGB grid (e.g. for a scene).
 */
export function solidBg(width: number, height: number, color: RGB): RGB[][] {
	return Array.from({ length: height }, () => Array(width).fill(color) as RGB[]);
}

/**
 * Create a radial glow background — dark center brightens outward from (cx, cy).
 * Useful for ghost/character glow effects.
 */
export function radialGlow(
	width: number,
	height: number,
	cx: number,
	cy: number,
	baseColor: RGB,
	glowColor: RGB,
	radius: number,
): RGB[][] {
	const r2 = radius * radius;
	return Array.from({ length: height }, (_, y) => {
		const dy2 = (y - cy) ** 2;
		return Array.from({ length: width }, (_, x) => {
			const glow = Math.max(0, 1 - ((x - cx) ** 2 + dy2) / r2);
			return [
				Math.round(baseColor[0] + (glowColor[0] - baseColor[0]) * glow),
				Math.round(baseColor[1] + (glowColor[1] - baseColor[1]) * glow),
				Math.round(baseColor[2] + (glowColor[2] - baseColor[2]) * glow),
			] as RGB;
		});
	});
}
