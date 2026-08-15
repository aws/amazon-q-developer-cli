/**
 * Stress Test — Rendering Engine Correctness Under Load
 *
 * Asserts only properties that hold regardless of how fast the machine is:
 *   - repaints stay differential, never full-screen redraws
 *   - no flicker (blanked cells between frames)
 *   - diff bytes stay viewport-bounded, not buffer-bounded
 *   - heap does not grow linearly with mount/unmount cycles
 *
 * Dumps reports to test/.artifacts/Stress_Test/
 */
import { describe, it, expect } from 'vitest';
import { TUI } from '../src/renderer/tui.js';
import { TestTerminal, analyzeFlicker, wait, testDir } from './helpers.js';
import { StressChatApp, generateResponse, runChatConversation, runWorstCase, writeReport } from './stress-helpers.js';

// Enough scrollback to push the conversation far past the 24-row viewport while
// keeping the merge-gating suite quick.
const MESSAGE_COUNT = 60;

describe('Stress Test', () => {
	it('conversation with 50-line responses: differential frames + no flicker', async () => {
		const run = await runChatConversation({ messageCount: MESSAGE_COUNT, linesPerResponse: 50 });

		expect(run.flicker.clean).toBe(true);
		expect(run.diffRatio).toBeGreaterThan(0.9);
		expect(run.lastViewport.some(l => l.includes('messages'))).toBe(true);
	}, 30_000);

	it('wide lines (wrap="overflow" equivalent): differential frames + no flicker', async () => {
		const run = await runChatConversation({ messageCount: MESSAGE_COUNT, linesPerResponse: 20, wideLines: true });

		if (!run.flicker.clean) {
			run.flicker.events.slice(0, 10).forEach(e => {
				const p = run.frames[e.frameIndex - 1]?.viewport[e.row] ?? '';
				const c = run.frames[e.frameIndex]?.viewport[e.row] ?? '';
				const n = run.frames[e.frameIndex + 1]?.viewport[e.row] ?? '';
				console.log(`  f=${e.frameIndex} r=${e.row} c=${e.col}`);
				console.log(`    prev: ${JSON.stringify(p.slice(0, 60))}`);
				console.log(`    curr: ${JSON.stringify(c.slice(0, 60))}`);
				console.log(`    next: ${JSON.stringify(n.slice(0, 60))}`);
			});
		}
		expect(run.flicker.clean).toBe(true);
		expect(run.diffRatio).toBeGreaterThan(0.9);
	}, 30_000);

	it('worst case: every line changes every frame', async () => {
		const run = await runWorstCase(60);

		expect(run.flicker.clean).toBe(true);
	}, 15_000);

	it('50k-line streaming: diff bytes are viewport-bounded', async () => {
		const term = new TestTerminal(80, 24);
		const tui = new TUI(term);
		const app = new StressChatApp();
		tui.addChild(app);
		tui.start();

		await wait(); await term.flush();

		app.messages.push({ role: 'user', content: 'Generate a huge codebase' });
		app.messages.push({ role: 'assistant', content: '' });

		const totalLines = 50_000;
		const chunkSize = 500;
		const allLines: string[] = [];
		for (let i = 0; i < totalLines; i++) {
			allLines.push(`  line_${i}: const val_${i} = compute(${i}, ${i * 7});`);
		}

		let maxFrameBytes = 0;
		let minDiffBytes = Infinity;

		for (let chunk = 0; chunk < totalLines; chunk += chunkSize) {
			const end = Math.min(chunk + chunkSize, totalLines);
			app.messages[1]!.content = allLines.slice(0, end).join('\n');
			app.statusText = `Streaming ${end}/${totalLines}`;
			tui.requestRender();
			await wait(1);
			await term.flush();

			const frame = term.getLastFrame()!;
			if (frame.writeBytes > maxFrameBytes) maxFrameBytes = frame.writeBytes;
			if (!frame.isFull && frame.writeBytes < minDiffBytes) minDiffBytes = frame.writeBytes;
		}

		tui.stop();

		const frames = term.getFrames();
		const flicker = analyzeFlicker(frames);
		const diffFrames = frames.filter(f => !f.isFull && f.index > 0);

		expect(flicker.clean).toBe(true);

		// Diff bytes must be viewport-bounded, not buffer-bounded.
		// Buffer is ~3MB (50k × ~60 chars). Viewport is 80×24 = 1920 chars.
		// With ANSI overhead, allow up to 50KB per diff frame.
		if (diffFrames.length > 0) {
			const avgDiffBytes = diffFrames.reduce((s, f) => s + f.writeBytes, 0) / diffFrames.length;
			expect(avgDiffBytes).toBeLessThan(50_000);
		}

		const dir = testDir('Stress_Test', 'streaming_50k_lines');
		writeReport(dir, {
			scenario: 'Stream 50,000 lines in 500-line chunks',
			'total lines': totalLines,
			chunks: totalLines / chunkSize,
			'frame output': {
				'total frames': frames.length,
				'full frames': frames.filter(f => f.isFull).length,
				'diff frames': diffFrames.length,
				'max frame bytes': `${maxFrameBytes}B`,
				'min diff bytes': `${minDiffBytes === Infinity ? 'N/A' : minDiffBytes + 'B'}`,
				'avg diff bytes': diffFrames.length > 0
					? `${(diffFrames.reduce((s, f) => s + f.writeBytes, 0) / diffFrames.length).toFixed(0)}B`
					: 'N/A',
			},
			flicker: { clean: flicker.clean, events: flicker.events.length },
		});
	}, 60_000);

	it('memory stability: 500 mount/unmount cycles', async () => {
		const term = new TestTerminal(80, 24);
		const tui = new TUI(term);
		tui.start();

		await wait(); await term.flush();

		// Warm up — let V8 JIT and GC settle
		for (let i = 0; i < 10; i++) {
			const app = new StressChatApp();
			app.messages.push({ role: 'assistant', content: generateResponse(50, i) });
			tui.addChild(app);
			tui.requestRender();
			await wait(1); await term.flush();
			tui.removeChild(app);
		}

		// Snapshot after warmup
		if ((globalThis as any).gc) (globalThis as any).gc();
		const memBefore = process.memoryUsage().heapUsed;

		for (let i = 0; i < 500; i++) {
			const app = new StressChatApp();
			app.messages.push({ role: 'user', content: `Cycle ${i}` });
			app.messages.push({ role: 'assistant', content: generateResponse(100, i) });
			tui.addChild(app);
			tui.requestRender();
			await wait(1);
			await term.flush();
			tui.removeChild(app);
		}

		if ((globalThis as any).gc) (globalThis as any).gc();
		const memAfter = process.memoryUsage().heapUsed;
		const growthMB = (memAfter - memBefore) / 1024 / 1024;

		tui.stop();

		// Without --expose-gc, V8 GC is lazy so we allow generous headroom.
		// The key: growth should NOT be linear with cycle count.
		// 500 cycles × 100 lines = 50k lines created/destroyed.
		// If Yoga nodes leak, this would be 100MB+.
		expect(growthMB).toBeLessThan(100);

		const dir = testDir('Stress_Test', 'mount_unmount_memory');
		writeReport(dir, {
			scenario: '500 mount/unmount cycles × 100-line components',
			cycles: 500,
			note: 'Without --expose-gc, heap numbers are approximate',
			memory: {
				'heap before': `${(memBefore / 1024 / 1024).toFixed(1)}MB`,
				'heap after': `${(memAfter / 1024 / 1024).toFixed(1)}MB`,
				'growth': `${growthMB.toFixed(1)}MB`,
			},
		});
	}, 30_000);
});
