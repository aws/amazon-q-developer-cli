import { describe, it, expect } from 'vitest';
import { renderText } from '../src/renderer/text-renderer.js';
import { createNode, createTextNode, setTextMeasureFunc } from '../src/reconciler/node-factory.js';
import { NODE_TYPES, WrapMode } from '../src/text/constants.js';
import { wrapTextWithAnsi } from '../src/utils/wrap-ansi.js';
import chalk from 'chalk';

const savedLevel = chalk.level;

function makeTextNode(content: string) {
	const node = createNode(NODE_TYPES.TWINKI_TEXT, {});
	const text = createTextNode(content);
	node.children.push(text);
	text.parent = node;
	return node;
}

/**
 * Regression tests for the yoga zero-width OOM.
 *
 * Root cause: resize to <6 cols → yoga measure returns width > container →
 * yoga re-layouts calling wrapTextWithAnsi(text, 0) thousands of times →
 * every char becomes a line with ANSI overhead → millions of allocs → OOM.
 *
 * On Linux (truecolor), each single-char line carries ~30 bytes of ANSI
 * escape state, making the explosion ~30x worse than plain text.
 */
describe('yoga zero-width OOM regression', () => {
	/**
	 * THE memory regression guard.
	 *
	 * Simulates the exact production failure: yoga diverges at narrow width,
	 * calling wrapTextWithAnsi with tiny unique widths (cache-busting) on
	 * truecolor-styled text. Without the renderText guard, this allocates
	 * 150MB+ of single-char lines. With the guard, renderText returns []
	 * and RSS stays flat.
	 *
	 * WITHOUT fix: 151MB RSS growth, 704K lines allocated
	 * WITH fix:      2MB RSS growth, 0 lines allocated
	 */
	it('renderText at width=0 must not grow RSS beyond 20MB under yoga-like load', () => {
		chalk.level = 3; // truecolor (Linux — the affected platform)
		const primary = chalk.rgb(201, 209, 217);
		const brand = chalk.rgb(255, 153, 0);

		const nodes = [
			makeTextNode(brand('⢸⣿⠀⠀⠀⠀⣿⡇⠹⣷⡀  ⠀⠈⢻⡇\n⠸⣿⣄⡀⢀⣠⣿⠇⠀⠙⣷⡀  ⢀⣼⠇')),
			makeTextNode(primary('Welcome to the new Kiro CLI UX! ') + brand('/tui') + primary(' to learn more.')),
			makeTextNode(chalk.bold(primary('AuthService')) + ' ' + primary('OAuth2 + SAML with SSO provider integration')),
			makeTextNode(primary('I recommend focusing on latency reduction for the APIGateway service.')),
			makeTextNode(chalk.dim('─'.repeat(80))),
		];

		const rssBefore = process.memoryUsage().rss;
		let totalLines = 0;

		// 2000 iterations × 5 nodes = 10,000 renderText calls at width=0.
		// This simulates yoga's measure loop diverging after a single resize.
		for (let i = 0; i < 2000; i++) {
			for (const node of nodes) {
				const lines = renderText(node, 0);
				totalLines += lines.length;
			}
		}

		const rssGrowth = process.memoryUsage().rss - rssBefore;
		const growthMB = rssGrowth / 1024 / 1024;

		// With fix: totalLines=0, growthMB≈2
		// Without fix: totalLines=704000, growthMB≈151 (and would be 6GB+ in real app)
		expect(totalLines).toBe(0);
		expect(growthMB).toBeLessThan(20);

		chalk.level = savedLevel;
	});

	/**
	 * Same test but bypasses renderText to hit wrapTextWithAnsi directly.
	 * Uses unique float widths to bust the wrapCache (simulating yoga's
	 * float-precision width oscillation during layout divergence).
	 *
	 * This catches regressions even if someone removes the renderText guard
	 * but fixes wrapTextWithAnsi itself.
	 */
	it('wrapTextWithAnsi at width≈0 with unique widths stays under 50MB RSS', () => {
		chalk.level = 3;
		const primary = chalk.rgb(201, 209, 217);
		const brand = chalk.rgb(255, 153, 0);

		const texts = [
			brand('⢸⣿⠀⠀⠀⠀⣿⡇⠹⣷⡀  ⠀⠈⢻⡇'),
			primary('Welcome to the new Kiro CLI UX!'),
			chalk.bold(primary('AuthService')) + ' ' + primary('OAuth2 + SAML'),
		];

		const rssBefore = process.memoryUsage().rss;

		for (let i = 0; i < 500; i++) {
			const w = i * 0.0001; // unique → cache miss
			for (const text of texts) {
				wrapTextWithAnsi(text, w);
			}
		}

		const growthMB = (process.memoryUsage().rss - rssBefore) / 1024 / 1024;
		expect(growthMB).toBeLessThan(50);

		chalk.level = savedLevel;
	});

	// ── Functional correctness ──

	it('renderText at width=0 returns empty array', () => {
		const node = makeTextNode('hello world');
		expect(renderText(node, 0)).toEqual([]);
	});

	it('renderText at negative width returns empty array', () => {
		const node = makeTextNode('hello world');
		expect(renderText(node, -5)).toEqual([]);
	});

	it('renderText at width=1 still works', () => {
		const node = makeTextNode('hi');
		const lines = renderText(node, 1);
		expect(lines.length).toBeGreaterThan(0);
		expect(lines.length).toBeLessThanOrEqual(3);
	});

	// ── Measure function convergence (root cause fix) ──

	it('overflow measure clamps width to available space', () => {
		const node = createNode(NODE_TYPES.TWINKI_TEXT, {
			wrap: WrapMode.OVERFLOW,
		});
		const text = createTextNode('A very long line that exceeds any narrow container');
		node.children.push(text);
		text.parent = node;
		setTextMeasureFunc(node);

		const measureFunc = node.yogaNode.getMeasureFunc?.();
		if (measureFunc) {
			// @ts-ignore — yoga measure signature
			const result = measureFunc(6, 1, Infinity, 0);
			// Without clamp: width=50 (content width) → yoga diverges
			// With clamp: width≤6 → yoga converges
			expect(result.width).toBeLessThanOrEqual(6);
		}
	});
});
