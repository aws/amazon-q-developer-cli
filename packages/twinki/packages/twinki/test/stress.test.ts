/**
 * Stress Test — Rendering Engine Performance Validation
 *
 * Simulates extreme workloads that real coding agents produce:
 *   - 1000-message conversations with large responses
 *   - Rapid streaming into massive scrollback
 *   - Memory stability over long sessions
 *
 * Measures the FULL render pipeline via TUI.perf* counters:
 *   Component.render() → line diffing → escape sequence build → terminal.write()
 *
 * Dumps reports to test/.artifacts/Stress_Test/
 */
import { describe, it, expect } from 'vitest';
import { TUI } from '../src/renderer/tui.js';
import type { Component } from '../src/renderer/component.js';
import { TestTerminal, analyzeFlicker, wait, testDir } from './helpers.js';
import { writeFileSync } from 'node:fs';
import { join } from 'node:path';

// --- Chat component ---

class StressChatApp implements Component {
	messages: { role: string; content: string }[] = [];
	statusText = 'Ready';

	render(width: number): string[] {
		const lines: string[] = [];
		for (const msg of this.messages) {
			if (msg.role === 'user') {
				lines.push(`> ${msg.content}`);
			} else {
				for (const ml of msg.content.split('\n')) {
					lines.push(`  ${ml}`);
				}
			}
			lines.push('');
		}
		lines.push('');
		lines.push('─'.repeat(width));
		lines.push(` ${this.statusText}  •  ${this.messages.length} messages`);
		return lines;
	}

	invalidate() {}
}

function generateResponse(lineCount: number, msgIndex: number): string {
	const lines: string[] = [];
	for (let i = 0; i < lineCount; i++) {
		if (i % 20 === 0) lines.push(`// === Section ${Math.floor(i / 20) + 1} of message ${msgIndex} ===`);
		else if (i % 5 === 0) lines.push(`  export function handler_${msgIndex}_${i}(req: Request, res: Response): void {`);
		else if (i % 5 === 1) lines.push(`    const data = await db.query('SELECT * FROM table_${i} WHERE id = ?', [req.params.id]);`);
		else if (i % 5 === 2) lines.push(`    if (!data) { res.status(404).json({ error: 'Not found', code: ${i} }); return; }`);
		else if (i % 5 === 3) lines.push(`    res.json({ success: true, data, timestamp: Date.now(), requestId: crypto.randomUUID() });`);
		else lines.push(`  }`);
	}
	return lines.join('\n');
}

function writeReport(dir: string, report: Record<string, any>): void {
	const lines: string[] = ['=== STRESS TEST REPORT ===', ''];
	for (const [key, val] of Object.entries(report)) {
		if (typeof val === 'object' && val !== null) {
			lines.push(`${key}:`);
			for (const [k, v] of Object.entries(val)) {
				lines.push(`  ${k}: ${v}`);
			}
		} else {
			lines.push(`${key}: ${val}`);
		}
		lines.push('');
	}
	writeFileSync(join(dir, 'report.txt'), lines.join('\n'));
}

describe('Stress Test', () => {
	it('1000-message conversation with 50-line responses: timing + flicker + memory', async () => {
		const term = new TestTerminal(80, 24);
		const tui = new TUI(term);
		const app = new StressChatApp();
		tui.addChild(app);
		tui.start();

		await wait(); await term.flush();

		const memBefore = process.memoryUsage();
		const wallStart = performance.now();
		// Reset perf counters after initial render
		tui.perfTotalRenderMs = 0;
		tui.perfMaxRenderMs = 0;
		tui.perfRenderCount = 0;

		const messageCount = 1000;
		const linesPerResponse = 50;
		let totalFullFrames = 0;
		let totalDiffFrames = 0;
		let totalWriteBytes = 0;
		const frameTimes: { i: number; ms: number; lines: number }[] = [];

		for (let i = 0; i < messageCount; i++) {
			app.messages.push({ role: 'user', content: `Question ${i + 1}: explain handler pattern ${i}` });
			app.messages.push({ role: 'assistant', content: generateResponse(linesPerResponse, i) });
			app.statusText = `Ready (${i + 1}/${messageCount})`;
			tui.requestRender();
			await wait(1);
			await term.flush();

			frameTimes.push({ i, ms: tui.perfLastRenderMs, lines: app.messages.length * 26 });

			const frame = term.getLastFrame()!;
			if (frame.isFull) totalFullFrames++;
			else totalDiffFrames++;
			totalWriteBytes += frame.writeBytes;
		}

		const wallMs = performance.now() - wallStart;
		const memAfter = process.memoryUsage();

		tui.stop();

		const frames = term.getFrames();
		const flicker = analyzeFlicker(frames);

		// --- Assertions ---
		expect(flicker.clean).toBe(true);

		const diffRatio = totalDiffFrames / (totalFullFrames + totalDiffFrames);
		expect(diffRatio).toBeGreaterThan(0.9);

		// Full pipeline avg render < 10ms (component.render + diff + escape build + write)
		const avgRenderMs = tui.perfTotalRenderMs / tui.perfRenderCount;
		expect(avgRenderMs).toBeLessThan(10);

		// No catastrophic spikes
		expect(tui.perfMaxRenderMs).toBeLessThan(100);

		const vp = term.getLastFrame()!.viewport;
		expect(vp.some(l => l.includes('messages'))).toBe(true);

		// --- Report ---
		const dir = testDir('Stress_Test', '1000_messages_50_line_responses');
		const heapGrowthMB = (memAfter.heapUsed - memBefore.heapUsed) / 1024 / 1024;

		// Identify slow frames
		const sorted = [...frameTimes].sort((a, b) => b.ms - a.ms);
		const p50 = frameTimes.map(f => f.ms).sort((a, b) => a - b)[Math.floor(frameTimes.length * 0.5)]!;
		const p95 = frameTimes.map(f => f.ms).sort((a, b) => a - b)[Math.floor(frameTimes.length * 0.95)]!;
		const p99 = frameTimes.map(f => f.ms).sort((a, b) => a - b)[Math.floor(frameTimes.length * 0.99)]!;

		const slowFrameLines: string[] = [];
		for (const f of sorted.slice(0, 10)) {
			slowFrameLines.push(`  frame ${f.i}: ${f.ms.toFixed(2)}ms (~${f.lines} content lines)`);
		}

		writeReport(dir, {
			scenario: '1000 messages × 50-line responses',
			'total messages': messageCount * 2,
			'total content lines': `~${messageCount * (linesPerResponse + 2)}`,
			'wall time': `${wallMs.toFixed(0)}ms`,
			'full pipeline (TUI.doRender)': {
				'total renders': tui.perfRenderCount,
				'avg render time': `${avgRenderMs.toFixed(2)}ms`,
				'max render time': `${tui.perfMaxRenderMs.toFixed(2)}ms`,
				'total render time': `${tui.perfTotalRenderMs.toFixed(0)}ms`,
				'p50': `${p50.toFixed(2)}ms`,
				'p95': `${p95.toFixed(2)}ms`,
				'p99': `${p99.toFixed(2)}ms`,
			},
			'top 10 slowest frames': '\n' + slowFrameLines.join('\n'),
			'frame output': {
				'total frames': frames.length,
				'full redraws': totalFullFrames,
				'differential frames': totalDiffFrames,
				'diff ratio': `${(diffRatio * 100).toFixed(1)}%`,
				'total bytes written': `${(totalWriteBytes / 1024).toFixed(0)}KB`,
				'avg bytes/frame': `${(totalWriteBytes / frames.length).toFixed(0)}B`,
			},
			memory: {
				'heap before': `${(memBefore.heapUsed / 1024 / 1024).toFixed(1)}MB`,
				'heap after': `${(memAfter.heapUsed / 1024 / 1024).toFixed(1)}MB`,
				'heap growth': `${heapGrowthMB.toFixed(1)}MB`,
				'rss after': `${(memAfter.rss / 1024 / 1024).toFixed(1)}MB`,
			},
			flicker: {
				clean: flicker.clean,
				events: flicker.events.length,
			},
		});
	}, 30_000);

	it('50k-line streaming: diff bytes are viewport-bounded', async () => {
		const term = new TestTerminal(80, 24);
		const tui = new TUI(term);
		const app = new StressChatApp();
		tui.addChild(app);
		tui.start();

		await wait(); await term.flush();
		tui.perfTotalRenderMs = 0;
		tui.perfMaxRenderMs = 0;
		tui.perfRenderCount = 0;

		app.messages.push({ role: 'user', content: 'Generate a huge codebase' });
		app.messages.push({ role: 'assistant', content: '' });

		const totalLines = 50_000;
		const chunkSize = 500;
		const allLines: string[] = [];
		for (let i = 0; i < totalLines; i++) {
			allLines.push(`  line_${i}: const val_${i} = compute(${i}, ${i * 7});`);
		}

		const wallStart = performance.now();
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

		const wallMs = performance.now() - wallStart;

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
		const avgRenderMs = tui.perfTotalRenderMs / tui.perfRenderCount;
		writeReport(dir, {
			scenario: 'Stream 50,000 lines in 500-line chunks',
			'total lines': totalLines,
			chunks: totalLines / chunkSize,
			'wall time': `${wallMs.toFixed(0)}ms`,
			'full pipeline (TUI.doRender)': {
				'total renders': tui.perfRenderCount,
				'avg render time': `${avgRenderMs.toFixed(2)}ms`,
				'max render time': `${tui.perfMaxRenderMs.toFixed(2)}ms`,
			},
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

	it('worst case: every line changes every frame', async () => {
		const term = new TestTerminal(80, 24);
		const tui = new TUI(term);
		const app = new StressChatApp();
		tui.addChild(app);
		tui.start();

		await wait(); await term.flush();
		tui.perfTotalRenderMs = 0;
		tui.perfMaxRenderMs = 0;
		tui.perfRenderCount = 0;

		const wallStart = performance.now();
		for (let i = 0; i < 200; i++) {
			app.messages = [
				{ role: 'user', content: `Unique question ${i} at ${Date.now()}` },
				{ role: 'assistant', content: generateResponse(50, i * 1000) },
			];
			app.statusText = `Frame ${i}`;
			tui.requestRender();
			await wait(1);
			await term.flush();
		}
		const wallMs = performance.now() - wallStart;

		tui.stop();

		const frames = term.getFrames();
		const flicker = analyzeFlicker(frames);

		expect(flicker.clean).toBe(true);
		expect(tui.perfMaxRenderMs).toBeLessThan(50);

		const avgRenderMs = tui.perfTotalRenderMs / tui.perfRenderCount;
		const dir = testDir('Stress_Test', 'worst_case_full_diff');
		writeReport(dir, {
			scenario: '200 frames, every line different each frame',
			frames: frames.length,
			'wall time': `${wallMs.toFixed(0)}ms`,
			'full pipeline (TUI.doRender)': {
				'avg render': `${avgRenderMs.toFixed(2)}ms`,
				'max render': `${tui.perfMaxRenderMs.toFixed(2)}ms`,
			},
			flicker: { clean: flicker.clean, events: flicker.events.length },
		});
	}, 15_000);
});
