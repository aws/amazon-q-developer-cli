import { describe, it, expect } from 'vitest';
import React, { useState } from 'react';
import { TestTerminal, wait, testDir } from './helpers.ts';
import { render } from '../src/reconciler/render.js';
import { Text } from '../src/components/Text.js';
import { Box } from '../src/components/Box.js';
import { Static } from '../src/components/Static.js';
import { writeFileSync } from 'node:fs';

function dumpFrames(dir: string, label: string, viewport: string[]) {
	const content = viewport.map((l, i) => `${String(i).padStart(2)}| ${l}`).join('\n');
	writeFileSync(`${dir}/${label}.txt`, content);
	return content;
}

describe('Resize Frame Capture', () => {
	it('shrink then expand with static + live content', async () => {
		const dir = testDir('Resize_Frame_Capture', 'shrink_then_expand_with_static_live');
		const term = new TestTerminal(60, 15);

		function App() {
			const items = ['Static msg 1', 'Static msg 2', 'Static msg 3'];
			return React.createElement(Box, { flexDirection: 'column' },
				React.createElement(Static, { items },
					(msg: string, i: number) => React.createElement(Text, { key: i }, `  ${msg}`),
				),
				React.createElement(Text, null, '─'.repeat(40)),
				React.createElement(Text, null, ' LIVE: current viewport content'),
				React.createElement(Text, null, '─'.repeat(40)),
				React.createElement(Text, null, ' status bar area'),
			);
		}

		const instance = render(React.createElement(App), { terminal: term, exitOnCtrlC: false });
		await wait(50); await term.flush();

		const frame1 = term.getViewport();
		dumpFrames(dir, '01_initial_60x15', frame1);
		console.log('\n=== FRAME 1: initial 60x15 ===');
		frame1.forEach((l, i) => console.log(`${String(i).padStart(2)}| ${l}`));

		// Count unique markers
		const staticCount1 = frame1.filter(l => l.includes('Static msg')).length;
		const liveCount1 = frame1.filter(l => l.includes('LIVE:')).length;
		expect(staticCount1).toBe(3);
		expect(liveCount1).toBe(1);

		// Shrink
		term.resize(30, 15);
		await wait(50); await term.flush();

		const frame2 = term.getViewport();
		dumpFrames(dir, '02_shrink_30x15', frame2);
		console.log('\n=== FRAME 2: shrink to 30x15 ===');
		frame2.forEach((l, i) => console.log(`${String(i).padStart(2)}| ${l}`));

		const liveCount2 = frame2.filter(l => l.includes('LIVE:')).length;
		expect(liveCount2).toBe(1); // no duplicate live content

		// Expand
		term.resize(80, 15);
		await wait(50); await term.flush();

		const frame3 = term.getViewport();
		dumpFrames(dir, '03_expand_80x15', frame3);
		console.log('\n=== FRAME 3: expand to 80x15 ===');
		frame3.forEach((l, i) => console.log(`${String(i).padStart(2)}| ${l}`));

		const liveCount3 = frame3.filter(l => l.includes('LIVE:')).length;
		const staticCount3 = frame3.filter(l => l.includes('Static msg')).length;
		expect(liveCount3).toBe(1);
		expect(staticCount3).toBe(3);

		instance.unmount();
	});

	it('multiple rapid resizes produce clean final frame', async () => {
		const dir = testDir('Resize_Frame_Capture', 'rapid_resizes_clean_final');
		const term = new TestTerminal(60, 10);

		const instance = render(
			React.createElement(Box, { flexDirection: 'column' },
				React.createElement(Text, null, 'MARKER_A'),
				React.createElement(Text, null, 'MARKER_B'),
				React.createElement(Text, null, 'MARKER_C'),
			),
			{ terminal: term, exitOnCtrlC: false },
		);

		await wait(50); await term.flush();

		// Rapid resize sequence
		term.resize(40, 10);
		await wait(10);
		term.resize(30, 10);
		await wait(10);
		term.resize(50, 10);
		await wait(50); await term.flush();

		const frame = term.getViewport();
		dumpFrames(dir, '01_after_rapid_resizes', frame);
		console.log('\n=== FRAME: after rapid resizes (final 50x10) ===');
		frame.forEach((l, i) => console.log(`${String(i).padStart(2)}| ${l}`));

		// Each marker should appear exactly once
		expect(frame.filter(l => l.includes('MARKER_A')).length).toBe(1);
		expect(frame.filter(l => l.includes('MARKER_B')).length).toBe(1);
		expect(frame.filter(l => l.includes('MARKER_C')).length).toBe(1);

		instance.unmount();
	});
});
