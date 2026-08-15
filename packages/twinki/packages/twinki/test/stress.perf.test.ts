/**
 * Stress Perf — Rendering Engine Timing Budgets
 *
 * Full-size versions of the stress scenarios, measured end to end:
 *   Component.render() → line diffing → escape sequence build → terminal.write()
 *
 * These budgets are wall-clock numbers, so they only mean something on a quiet
 * machine and must never gate a merge.
 *
 * Flicker and diff ratio are asserted alongside them because they are output
 * ratios rather than durations: they hold at any speed, and these are the only
 * runs that reach full scale.
 *
 * Dumps reports to test/.artifacts/Stress_Perf/
 */
import { describe, it, expect } from 'vitest';
import { testDir } from './helpers.js';
import { runChatConversation, runWorstCase, writeReport, type FrameTime } from './stress-helpers.js';

function percentile(frameTimes: FrameTime[], fraction: number): number {
	const sorted = frameTimes.map(f => f.ms).sort((a, b) => a - b);
	return sorted[Math.floor(sorted.length * fraction)]!;
}

function slowestFrames(frameTimes: FrameTime[]): string {
	return '\n' + [...frameTimes]
		.sort((a, b) => b.ms - a.ms)
		.slice(0, 10)
		.map(f => `  frame ${f.i}: ${f.ms.toFixed(2)}ms (~${f.lines} content lines)`)
		.join('\n');
}

describe('Stress Perf', () => {
	it('1000-message conversation with 50-line responses', async () => {
		const messageCount = 1000;
		const linesPerResponse = 50;
		const run = await runChatConversation({ messageCount, linesPerResponse });

		const dir = testDir('Stress_Perf', '1000_messages_50_line_responses');
		writeReport(dir, {
			scenario: '1000 messages × 50-line responses',
			'total messages': messageCount * 2,
			'total content lines': `~${messageCount * (linesPerResponse + 2)}`,
			'wall time': `${run.wallMs.toFixed(0)}ms`,
			'full pipeline (TUI.doRender)': {
				'total renders': run.renderCount,
				'avg render time': `${run.avgRenderMs.toFixed(2)}ms`,
				'max render time': `${run.maxRenderMs.toFixed(2)}ms`,
				'total render time': `${run.totalRenderMs.toFixed(0)}ms`,
				'p50': `${percentile(run.frameTimes, 0.5).toFixed(2)}ms`,
				'p95': `${percentile(run.frameTimes, 0.95).toFixed(2)}ms`,
				'p99': `${percentile(run.frameTimes, 0.99).toFixed(2)}ms`,
			},
			'top 10 slowest frames': slowestFrames(run.frameTimes),
			'frame output': {
				'total frames': run.frames.length,
				'full redraws': run.fullFrames,
				'differential frames': run.diffFrames,
				'diff ratio': `${(run.diffRatio * 100).toFixed(1)}%`,
				'total bytes written': `${(run.totalWriteBytes / 1024).toFixed(0)}KB`,
				'avg bytes/frame': `${(run.totalWriteBytes / run.frames.length).toFixed(0)}B`,
			},
			memory: {
				'heap before': `${(run.memBefore.heapUsed / 1024 / 1024).toFixed(1)}MB`,
				'heap after': `${(run.memAfter.heapUsed / 1024 / 1024).toFixed(1)}MB`,
				'heap growth': `${run.heapGrowthMB.toFixed(1)}MB`,
				'rss after': `${(run.memAfter.rss / 1024 / 1024).toFixed(1)}MB`,
			},
			flicker: {
				clean: run.flicker.clean,
				events: run.flicker.events.length,
			},
		});

		expect(run.flicker.clean).toBe(true);
		expect(run.diffRatio).toBeGreaterThan(0.9);
		expect(run.avgRenderMs).toBeLessThan(15);
		expect(run.maxRenderMs).toBeLessThan(150);
	});

	it('worst case: every line changes every frame', async () => {
		const run = await runWorstCase(200);

		const dir = testDir('Stress_Perf', 'worst_case_full_diff');
		writeReport(dir, {
			scenario: '200 frames, every line different each frame',
			frames: run.frames.length,
			'wall time': `${run.wallMs.toFixed(0)}ms`,
			'full pipeline (TUI.doRender)': {
				'avg render': `${run.avgRenderMs.toFixed(2)}ms`,
				'max render': `${run.maxRenderMs.toFixed(2)}ms`,
			},
			flicker: { clean: run.flicker.clean, events: run.flicker.events.length },
		});

		expect(run.flicker.clean).toBe(true);
		expect(run.maxRenderMs).toBeLessThan(50);
	});

	it('wide lines: 500-message conversation with long lines', async () => {
		const messageCount = 500;
		const linesPerResponse = 20;
		const run = await runChatConversation({ messageCount, linesPerResponse, wideLines: true });

		const dir = testDir('Stress_Perf', 'wide_lines_500_messages');
		writeReport(dir, {
			scenario: 'wide-lines: 500 messages × 20 long lines each (≈160 cols / 2 physical rows)',
			'total messages': messageCount * 2,
			'total content lines': `~${messageCount * (linesPerResponse + 2)}`,
			'wall time': `${run.wallMs.toFixed(0)}ms`,
			'full pipeline (TUI.doRender)': {
				'total renders': run.renderCount,
				'avg render time': `${run.avgRenderMs.toFixed(2)}ms`,
				'max render time': `${run.maxRenderMs.toFixed(2)}ms`,
				'total render time': `${run.totalRenderMs.toFixed(0)}ms`,
				'p50': `${percentile(run.frameTimes, 0.5).toFixed(2)}ms`,
				'p95': `${percentile(run.frameTimes, 0.95).toFixed(2)}ms`,
				'p99': `${percentile(run.frameTimes, 0.99).toFixed(2)}ms`,
			},
			'slowest frames': slowestFrames(run.frameTimes),
			'diff ratio': `${(run.diffRatio * 100).toFixed(1)}%`,
			'total write bytes': run.totalWriteBytes,
			flicker: { clean: run.flicker.clean, events: run.flicker.events.length },
			'heap growth': `${run.heapGrowthMB.toFixed(2)}MB`,
		});

		expect(run.flicker.clean).toBe(true);
		expect(run.diffRatio).toBeGreaterThan(0.9);
		// The wide-line path does per-line visibleWidth plus physical row math, so
		// it gets more budget than the narrow path.
		expect(run.avgRenderMs).toBeLessThan(120);
		expect(run.maxRenderMs).toBeLessThan(300);
	});
});
