/**
 * Shared harness for the stress scenarios.
 *
 * The same runs are driven at small sizes by the correctness suite and at full
 * size by the perf suite, so the workload generators and the measurement
 * plumbing live here instead of being duplicated per suite.
 */
import { TUI } from '../src/renderer/tui.js';
import type { Component } from '../src/renderer/component.js';
import { TestTerminal, analyzeFlicker, wait } from './helpers.js';
import { writeFileSync } from 'node:fs';
import { join } from 'node:path';

export class StressChatApp implements Component {
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

export function generateResponse(lineCount: number, msgIndex: number): string {
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

// Roughly 2× an 80-column terminal, so every line soft-wraps into 2 physical
// rows and exercises the physicalize cursor/row math.
const WIDE_LINE =
	'  const result = await service.processRequestWithValidation(req.body, { timeout: 30000, retries: 3, fallbackEnabled: true, metricsTag: ';

function generateWideResponse(lineCount: number, msgIndex: number): string {
	const lines: string[] = [];
	for (let j = 0; j < lineCount; j++) {
		// Fixed-length suffixes: flicker analysis compares column by column, so
		// varying token lengths would read as blanked cells.
		const suffix = `msg${String(msgIndex).padStart(4, '0')}_line${String(j).padStart(2, '0')}_tokenXYZ01234567`;
		lines.push(`${WIDE_LINE}'${suffix}' });`);
	}
	return lines.join('\n');
}

export function writeReport(dir: string, report: Record<string, any>): void {
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

export interface FrameTime {
	i: number;
	ms: number;
	lines: number;
}

export interface StressRunResult {
	frames: ReturnType<TestTerminal['getFrames']>;
	flicker: ReturnType<typeof analyzeFlicker>;
	lastViewport: string[];
	fullFrames: number;
	diffFrames: number;
	diffRatio: number;
	frameTimes: FrameTime[];
	avgRenderMs: number;
	maxRenderMs: number;
	totalRenderMs: number;
	renderCount: number;
	wallMs: number;
	totalWriteBytes: number;
	heapGrowthMB: number;
	memBefore: NodeJS.MemoryUsage;
	memAfter: NodeJS.MemoryUsage;
}

interface RunSpec {
	frameCount: number;
	linesPerResponse: number;
	wideLines: boolean;
	updateApp(app: StressChatApp, frame: number): void;
}

async function runFrames(spec: RunSpec): Promise<StressRunResult> {
	const term = new TestTerminal(80, 24);
	const tui = new TUI(term, spec.wideLines ? { wideLines: true } : undefined);
	const app = new StressChatApp();
	tui.addChild(app);
	tui.start();

	await wait(); await term.flush();

	const memBefore = process.memoryUsage();
	const wallStart = performance.now();
	// Reset so the initial full render is not attributed to the measured run.
	tui.perfTotalRenderMs = 0;
	tui.perfMaxRenderMs = 0;
	tui.perfRenderCount = 0;

	let fullFrames = 0;
	let diffFrames = 0;
	let totalWriteBytes = 0;
	const frameTimes: FrameTime[] = [];

	for (let i = 0; i < spec.frameCount; i++) {
		spec.updateApp(app, i);
		tui.requestRender();
		await wait(1);
		await term.flush();

		frameTimes.push({ i, ms: tui.perfLastRenderMs, lines: app.messages.length * (spec.linesPerResponse + 2) });

		const frame = term.getLastFrame()!;
		if (frame.isFull) fullFrames++;
		else diffFrames++;
		totalWriteBytes += frame.writeBytes;
	}

	const wallMs = performance.now() - wallStart;
	const memAfter = process.memoryUsage();
	const lastViewport = term.getLastFrame()!.viewport;

	tui.stop();

	const frames = term.getFrames();

	return {
		frames,
		flicker: analyzeFlicker(frames),
		lastViewport,
		fullFrames,
		diffFrames,
		diffRatio: diffFrames / (fullFrames + diffFrames),
		frameTimes,
		avgRenderMs: tui.perfTotalRenderMs / tui.perfRenderCount,
		maxRenderMs: tui.perfMaxRenderMs,
		totalRenderMs: tui.perfTotalRenderMs,
		renderCount: tui.perfRenderCount,
		wallMs,
		totalWriteBytes,
		heapGrowthMB: (memAfter.heapUsed - memBefore.heapUsed) / 1024 / 1024,
		memBefore,
		memAfter,
	};
}

export interface ChatRunOptions {
	messageCount: number;
	linesPerResponse: number;
	/** Emit lines wider than the terminal so each one soft-wraps. */
	wideLines?: boolean;
}

/** Appends a user + assistant message per frame and captures every frame. */
export function runChatConversation(opts: ChatRunOptions): Promise<StressRunResult> {
	return runFrames({
		frameCount: opts.messageCount,
		linesPerResponse: opts.linesPerResponse,
		wideLines: opts.wideLines ?? false,
		updateApp: (app, i) => {
			const content = opts.wideLines
				? generateWideResponse(opts.linesPerResponse, i)
				: generateResponse(opts.linesPerResponse, i);
			app.messages.push({ role: 'user', content: `Question ${i + 1}: explain handler pattern ${i}` });
			app.messages.push({ role: 'assistant', content });
			app.statusText = `Ready (${i + 1}/${opts.messageCount})`;
		},
	});
}

/** Replaces the whole conversation every frame, so no line survives a diff. */
export function runWorstCase(frameCount: number): Promise<StressRunResult> {
	return runFrames({
		frameCount,
		linesPerResponse: 50,
		wideLines: false,
		updateApp: (app, i) => {
			app.messages = [
				{ role: 'user', content: `Unique question ${i} at ${Date.now()}` },
				{ role: 'assistant', content: generateResponse(50, i * 1000) },
			];
			app.statusText = `Frame ${i}`;
		},
	});
}
