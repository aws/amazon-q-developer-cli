/**
 * Typewriter Degradation Test
 *
 * Compares two approaches for streaming markdown:
 *   1. <Markdown> component (marked + shiki + many React nodes)
 *   2. Single <Text> component (pre-rendered string, one React node)
 *
 * Measures per-frame render time as visible text grows to detect
 * linear degradation.
 */
import { describe, it, expect } from 'vitest';
import React from 'react';
import { render } from '../src/reconciler/render.js';
import { Markdown } from '../src/components/Markdown.js';
import { Box } from '../src/components/Box.js';
import { Text } from '../src/components/Text.js';
import { TUI } from '../src/renderer/tui.js';
import { TestTerminal, wait, testDir } from './helpers.js';
import { writeFileSync } from 'node:fs';
import { join } from 'node:path';

function generateMarkdown(wordCount: number): string {
	const sections: string[] = [];
	sections.push('# DataGrid Component Implementation\n');
	sections.push('Here is the full implementation with sorting, filtering, and pagination.\n');
	sections.push('```typescript');
	for (let i = 0; i < Math.floor(wordCount / 10); i++) {
		if (i % 20 === 0) sections.push(`\n// === Section ${Math.floor(i / 20) + 1} ===`);
		else if (i % 5 === 0) sections.push(`export function handler_${i}(req: Request): void {`);
		else if (i % 5 === 1) sections.push(`  const data = await db.query('SELECT * FROM t${i}');`);
		else if (i % 5 === 2) sections.push(`  if (!data) { res.status(404).json({ error: 'Not found' }); }`);
		else if (i % 5 === 3) sections.push(`  return res.json({ ok: true, data, ts: Date.now() });`);
		else sections.push(`}`);
	}
	sections.push('```\n');
	sections.push('The component handles **sorting**, *filtering*, and `pagination` efficiently.\n');
	return sections.join('\n');
}

interface FrameTime {
	wordCount: number;
	renderMs: number;
	pipelineMs: number;
}

async function measureDegradation(
	label: string,
	AppComponent: React.FC<{ text: string }>,
): Promise<{ frameTimes: FrameTime[]; degradationRatio: number }> {
	const term = new TestTerminal(80, 24);
	const tui = new TUI(term);

	const fullText = generateMarkdown(2000);
	const words = fullText.split(/(?<=\s)/);
	const totalWords = words.length;
	const batchSize = 3;
	const frameTimes: FrameTime[] = [];
	let wordIdx = 0;

	const instance = render(React.createElement(AppComponent, { text: '' }), { tui });
	tui.start();
	await wait(); await term.flush();

	tui.perfTotalRenderMs = 0;
	tui.perfMaxRenderMs = 0;
	tui.perfRenderCount = 0;

	while (wordIdx < totalWords) {
		const end = Math.min(wordIdx + batchSize, totalWords);
		let currentText = words.slice(0, end).join('');
		wordIdx = end;

		const fenceCount = (currentText.match(/^```/gm) || []).length;
		if (fenceCount % 2 === 1) currentText += '\n```';

		const before = performance.now();
		instance.rerender(React.createElement(AppComponent, { text: currentText }));
		await wait(1);
		await term.flush();
		const after = performance.now();

		frameTimes.push({
			wordCount: end,
			renderMs: tui.perfLastRenderMs,
			pipelineMs: after - before,
		});
	}

	instance.unmount();
	tui.stop();

	const q1End = Math.floor(frameTimes.length * 0.25);
	const q4Start = Math.floor(frameTimes.length * 0.75);
	const q1Avg = frameTimes.slice(0, q1End).reduce((s, f) => s + f.pipelineMs, 0) / q1End;
	const q4Avg = frameTimes.slice(q4Start).reduce((s, f) => s + f.pipelineMs, 0) / (frameTimes.length - q4Start);

	return { frameTimes, degradationRatio: q4Avg / q1Avg };
}

describe('Typewriter Degradation', () => {
	it('compare: <Markdown> vs <Markdown highlight> vs <Text> for growing content', async () => {
		// Approach 1: <Markdown> (default — fast ANSI strings)
		const MarkdownApp: React.FC<{ text: string }> = ({ text }) => {
			if (!text) return React.createElement(Text, null, 'Empty');
			return React.createElement(Box, { flexDirection: 'column' },
				React.createElement(Markdown, { children: text }),
			);
		};

		// Approach 2: Single <Text> (one React node, plain string)
		const PlainTextApp: React.FC<{ text: string }> = ({ text }) => {
			if (!text) return React.createElement(Text, null, 'Empty');
			return React.createElement(Text, null, text);
		};

		// Approach 3: <Markdown highlight> (shiki, many React nodes)
		const HighlightApp: React.FC<{ text: string }> = ({ text }) => {
			if (!text) return React.createElement(Text, null, 'Empty');
			return React.createElement(Box, { flexDirection: 'column' },
				React.createElement(Markdown, { children: text, highlight: true }),
			);
		};

		const mdResult = await measureDegradation('Markdown', MarkdownApp);
		const hlResult = await measureDegradation('Markdown highlight', HighlightApp);
		const txtResult = await measureDegradation('PlainText', PlainTextApp);

		// Sample frames for report
		const sampleIndices = [0, 10, 25, 50, 75, 100, 150, 200, -1];
		const sampleFrame = (ft: FrameTime[], i: number) => {
			const idx = i === -1 ? ft.length - 1 : i;
			return ft[idx];
		};

		const dir = testDir('Typewriter_Degradation', 'markdown_vs_plaintext');
		const lines: string[] = [
			'=== TYPEWRITER DEGRADATION: MARKDOWN vs PLAIN TEXT ===',
			'',
			'--- <Markdown> (default — fast ANSI strings, minimal React nodes) ---',
			`  Degradation ratio: ${mdResult.degradationRatio.toFixed(2)}x`,
			'  Frame | Words | TUI render | Full pipeline',
			'  ------|-------|------------|-------------',
		];
		for (const i of sampleIndices) {
			const f = sampleFrame(mdResult.frameTimes, i);
			if (f) lines.push(`  ${String(i === -1 ? mdResult.frameTimes.length - 1 : i).padStart(5)} | ${String(f.wordCount).padStart(5)} | ${f.renderMs.toFixed(2).padStart(8)}ms | ${f.pipelineMs.toFixed(2).padStart(8)}ms`);
		}

		lines.push('');
		lines.push('--- Single <Text> component (one React node, plain string) ---');
		lines.push(`  Degradation ratio: ${txtResult.degradationRatio.toFixed(2)}x`);
		lines.push('  Frame | Words | TUI render | Full pipeline');
		lines.push('  ------|-------|------------|-------------');
		for (const i of sampleIndices) {
			const f = sampleFrame(txtResult.frameTimes, i);
			if (f) lines.push(`  ${String(i === -1 ? txtResult.frameTimes.length - 1 : i).padStart(5)} | ${String(f.wordCount).padStart(5)} | ${f.renderMs.toFixed(2).padStart(8)}ms | ${f.pipelineMs.toFixed(2).padStart(8)}ms`);
		}

		lines.push('');
		lines.push('--- <Markdown highlight> (shiki syntax highlighting, many React nodes) ---');
		lines.push(`  Degradation ratio: ${hlResult.degradationRatio.toFixed(2)}x`);
		lines.push('  Frame | Words | TUI render | Full pipeline');
		lines.push('  ------|-------|------------|-------------');
		for (const i of sampleIndices) {
			const f = sampleFrame(hlResult.frameTimes, i);
			if (f) lines.push(`  ${String(i === -1 ? hlResult.frameTimes.length - 1 : i).padStart(5)} | ${String(f.wordCount).padStart(5)} | ${f.renderMs.toFixed(2).padStart(8)}ms | ${f.pipelineMs.toFixed(2).padStart(8)}ms`);
		}

		lines.push('');
		lines.push('--- Comparison ---');
		const mdQ4 = mdResult.frameTimes.slice(Math.floor(mdResult.frameTimes.length * 0.75));
		const hlQ4 = hlResult.frameTimes.slice(Math.floor(hlResult.frameTimes.length * 0.75));
		const txtQ4 = txtResult.frameTimes.slice(Math.floor(txtResult.frameTimes.length * 0.75));
		const mdAvg = mdQ4.reduce((s, f) => s + f.pipelineMs, 0) / mdQ4.length;
		const hlAvg = hlQ4.reduce((s, f) => s + f.pipelineMs, 0) / hlQ4.length;
		const txtAvg = txtQ4.reduce((s, f) => s + f.pipelineMs, 0) / txtQ4.length;
		lines.push(`  Markdown Q4 avg:           ${mdAvg.toFixed(2)}ms`);
		lines.push(`  Markdown highlight Q4 avg: ${hlAvg.toFixed(2)}ms`);
		lines.push(`  PlainText Q4 avg:          ${txtAvg.toFixed(2)}ms`);
		lines.push(`  Markdown is ${(hlAvg / mdAvg).toFixed(1)}x faster than Markdown highlight`);

		writeFileSync(join(dir, 'report.txt'), lines.join('\n') + '\n');

		// Assertions
		// Markdown highlight degradation should be < 6x (known, shiki + many nodes)
		expect(hlResult.degradationRatio).toBeLessThan(6);
		// Default Markdown should be much better than highlight mode
		expect(mdResult.degradationRatio).toBeLessThan(hlResult.degradationRatio);
		// PlainText should degrade least
		expect(txtResult.degradationRatio).toBeLessThan(2);
		// No frame > 600ms (shiki cold start can spike once)
		expect(Math.max(...hlResult.frameTimes.map(f => f.pipelineMs))).toBeLessThan(600);
	}, 60_000);
});
