import { describe, it, expect } from 'vitest';
import React, { useState } from 'react';
import { TestTerminal, wait } from './helpers.ts';
import { render } from '../src/reconciler/render.js';
import { Text } from '../src/components/Text.js';
import { Box } from '../src/components/Box.js';
import { Static } from '../src/components/Static.js';

describe('External clear detection', () => {
	it('recovers after external \\x1b[2J clear sequence', async () => {
		const term = new TestTerminal(60, 15);
		const staticItems = ['WELCOME_LOGO'];

		function App() {
			return React.createElement(Box, { flexDirection: 'column' },
				React.createElement(Static, { items: staticItems },
					(msg: string, i: number) => React.createElement(Text, { key: i }, msg),
				),
				React.createElement(Text, null, '─'.repeat(40)),
				React.createElement(Text, null, 'status: model · 1%'),
				React.createElement(Text, null, 'ask a question'),
			);
		}

		const instance = render(React.createElement(App), { terminal: term, exitOnCtrlC: false });
		await wait(50); await term.flush();

		const before = term.getViewport();
		console.log('=== BEFORE clear ===');
		before.forEach((l, i) => console.log(`${String(i).padStart(2)}| ${l}`));

		expect(before.some(l => l.includes('WELCOME_LOGO'))).toBe(true);
		expect(before.some(l => l.includes('status: model'))).toBe(true);
		expect(before.filter(l => l.includes('ask a question')).length).toBe(1);

		// Simulate external clear (like shell `clear` command)
		// 1. Clear the test terminal's xterm buffer (simulates what clear does to the real terminal)
		term.write('\x1b[2J\x1b[H');
		// 2. Trigger the stdout intercept (simulates the clear going through process.stdout)
		process.stdout.write('\x1b[2J\x1b[H');

		await wait(50); await term.flush();

		const after = term.getViewport();
		console.log('=== AFTER clear ===');
		after.forEach((l, i) => console.log(`${String(i).padStart(2)}| ${l}`));

		// Content should be redrawn exactly once — no duplication
		const promptCount = after.filter(l => l.includes('ask a question')).length;
		const statusCount = after.filter(l => l.includes('status: model')).length;

		instance.unmount();

		expect(promptCount).toBe(1);
		expect(statusCount).toBe(1);
	});

	it('does not trigger on internal clear sequences', async () => {
		const term = new TestTerminal(60, 10);

		function App() {
			return React.createElement(Box, { flexDirection: 'column' },
				React.createElement(Text, null, 'CONTENT_LINE'),
			);
		}

		const instance = render(React.createElement(App), { terminal: term, exitOnCtrlC: false });
		await wait(50); await term.flush();

		// Resize triggers internal CLEAR_ALL — should not cause issues
		term.resize(80, 10);
		await wait(50); await term.flush();

		const after = term.getViewport();
		const contentCount = after.filter(l => l.includes('CONTENT_LINE')).length;

		instance.unmount();

		expect(contentCount).toBe(1);
	});

	it('accumulatedStaticOutput is cleared after external clear', async () => {
		const term = new TestTerminal(60, 15);
		let addItem!: () => void;
		let itemCount = 0;

		function App() {
			const [items, setItems] = useState(['STATIC_1', 'STATIC_2']);
			addItem = () => {
				itemCount++;
				setItems(prev => [...prev, `STATIC_NEW_${itemCount}`]);
			};
			return React.createElement(Box, { flexDirection: 'column' },
				React.createElement(Static, { items },
					(msg: string, i: number) => React.createElement(Text, { key: i }, msg),
				),
				React.createElement(Text, null, 'LIVE_AREA'),
			);
		}

		const instance = render(React.createElement(App), { terminal: term, exitOnCtrlC: false });
		await wait(50); await term.flush();

		const before = term.getViewport();
		expect(before.some(l => l.includes('STATIC_1'))).toBe(true);
		expect(before.some(l => l.includes('STATIC_2'))).toBe(true);

		// External clear
		term.write('\x1b[2J\x1b[H');
		process.stdout.write('\x1b[2J\x1b[H');
		await wait(50); await term.flush();

		const afterClear = term.getViewport();
		console.log('=== AFTER external clear ===');
		afterClear.forEach((l, i) => console.log(`${String(i).padStart(2)}| ${l}`));

		// Static content should NOT reappear (accumulatedStaticOutput was cleared)
		const static1Count = afterClear.filter(l => l.includes('STATIC_1')).length;
		const static2Count = afterClear.filter(l => l.includes('STATIC_2')).length;
		const liveCount = afterClear.filter(l => l.includes('LIVE_AREA')).length;

		instance.unmount();

		// Static is gone (cleared), live is redrawn
		expect(static1Count).toBe(0);
		expect(static2Count).toBe(0);
		expect(liveCount).toBe(1);
	});
});
