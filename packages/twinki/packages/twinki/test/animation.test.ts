/**
 * Animation utils tests — halfBlock, stamp, renderGrid, COLOR_RGB
 */
import { describe, it, expect } from 'vitest';
import { halfBlock, stamp, renderGrid, COLOR_RGB } from '../src/animation/index.js';

describe('COLOR_RGB', () => {
	it('has transparent key', () => {
		expect(COLOR_RGB['.']).toBeUndefined();
	});
	it('has white body key W', () => {
		const w = COLOR_RGB['W'];
		expect(w).toBeDefined();
		expect(w![0]).toBeGreaterThan(200); // bright
	});
});

describe('halfBlock', () => {
	it('returns a string containing the half-block char', () => {
		const result = halfBlock([255, 0, 0], [0, 0, 255]);
		expect(result).toContain('▀');
	});
	it('encodes top color as fg (38;2)', () => {
		const result = halfBlock([10, 20, 30], [40, 50, 60]);
		expect(result).toContain('38;2;10;20;30');
	});
	it('encodes bottom color as bg (48;2)', () => {
		const result = halfBlock([10, 20, 30], [40, 50, 60]);
		expect(result).toContain('48;2;40;50;60');
	});
	it('ends with reset', () => {
		const result = halfBlock([0, 0, 0], [0, 0, 0]);
		expect(result).toContain('\x1b[0m');
	});
});

describe('stamp', () => {
	it('stamps non-dot chars into grid', () => {
		const grid: string[][] = [['.','.','.'],['.','.','.']];
		stamp(grid, ['WW', 'KK'], 0, 0);
		expect(grid[0]![0]).toBe('W');
		expect(grid[0]![1]).toBe('W');
		expect(grid[1]![0]).toBe('K');
	});
	it('skips dot chars (transparent)', () => {
		const grid: string[][] = [['X','X'],['X','X']];
		stamp(grid, ['.W', 'W.'], 0, 0);
		expect(grid[0]![0]).toBe('X'); // dot = transparent, keep X
		expect(grid[0]![1]).toBe('W');
		expect(grid[1]![0]).toBe('W');
		expect(grid[1]![1]).toBe('X');
	});
	it('clips to grid bounds', () => {
		const grid: string[][] = [['.','.']];
		stamp(grid, ['WWWW'], -1, 0); // starts off-screen left
		expect(grid[0]![0]).toBe('W'); // col 1 of sprite = col 0 of grid
		expect(grid[0]![1]).toBe('W');
	});
	it('ignores rows outside grid', () => {
		const grid: string[][] = [['.','.']];
		stamp(grid, ['WW', 'KK'], 0, 1); // row 1 is out of bounds
		expect(grid[0]![0]).toBe('.'); // row 0 not touched
	});
});

describe('renderGrid', () => {
	it('returns one line per 2 pixel rows', () => {
		const grid = [['W','W'],['K','K'],['W','W'],['K','K']];
		const result = renderGrid(grid);
		expect(result.split('\n')).toHaveLength(2);
	});
	it('each output char contains ▀', () => {
		const grid = [['W','W'],['K','K']];
		const result = renderGrid(grid);
		const count = (result.match(/▀/g) ?? []).length;
		expect(count).toBe(2); // 2 columns
	});
	it('uses bgRgb for dot pixels', () => {
		const grid: string[][] = [['.','.'],['.', '.']];
		const bg: [number,number,number][][] = [
			[[255,0,0],[255,0,0]],
			[[0,255,0],[0,255,0]],
		];
		const result = renderGrid(grid, bg);
		// top row dot → fg = bg[0] = red
		expect(result).toContain('38;2;255;0;0');
		// bottom row dot → bg color = bg[1] = green
		expect(result).toContain('48;2;0;255;0');
	});
	it('uses COLOR_RGB for non-dot pixels', () => {
		const grid: string[][] = [['W','.'],['.', '.']];
		const bg: [number,number,number][][] = [
			[[0,0,0],[0,0,0]],
			[[0,0,0],[0,0,0]],
		];
		const result = renderGrid(grid, bg);
		const [r, g, b] = COLOR_RGB['W']!;
		expect(result).toContain(`38;2;${r};${g};${b}`);
	});
});
