import { describe, it, expect } from 'vitest';
import React, { useState } from 'react';
import { TestTerminal, wait } from './helpers.ts';
import { render } from '../src/reconciler/render.js';
import { Text } from '../src/components/Text.js';
import { Box } from '../src/components/Box.js';
import { Static } from '../src/components/Static.js';

describe('Resize E2E', () => {
	it('live content re-renders at new width after resize', async () => {
		const term = new TestTerminal(60, 10);
		const instance = render(
			React.createElement(Box, { flexDirection: 'column' },
				React.createElement(Text, null, 'Hello World'),
				React.createElement(Text, null, '─'.repeat(50)),
			),
			{ terminal: term, exitOnCtrlC: false },
		);

		await wait(50); await term.flush();
		const before = term.getViewport();
		expect(before.some(l => l.includes('─'))).toBe(true);

		term.resize(30, 10);
		await wait(50); await term.flush();
		const after = term.getViewport();

		instance.unmount();
		expect(after.some(l => l.includes('Hello'))).toBe(true);
	});

	it('static + live content both render correctly after resize', async () => {
		const term = new TestTerminal(60, 10);
		let addMsg!: () => void;

		function App() {
			const [messages, setMessages] = useState(['Message 1', 'Message 2']);
			addMsg = () => setMessages(m => [...m, `Message ${m.length + 1}`]);

			return React.createElement(Box, { flexDirection: 'column' },
				React.createElement(Static, { items: messages },
					(msg: string, i: number) => React.createElement(Text, { key: i }, msg),
				),
				React.createElement(Text, null, '--- live area ---'),
			);
		}

		const instance = render(React.createElement(App), { terminal: term, exitOnCtrlC: false });
		await wait(50); await term.flush();

		const viewport0 = term.getViewport();
		expect(viewport0.some(l => l.includes('Message 1'))).toBe(true);
		expect(viewport0.some(l => l.includes('live area'))).toBe(true);

		// Resize
		term.resize(30, 10);
		await wait(50); await term.flush();

		const viewport1 = term.getViewport();
		const hasLive = viewport1.some(l => l.includes('live area'));
		expect(hasLive).toBe(true);

		instance.unmount();
	});

	it('no duplicate content in viewport after resize', async () => {
		const term = new TestTerminal(60, 10);
		const instance = render(
			React.createElement(Box, { flexDirection: 'column' },
				React.createElement(Text, null, 'UNIQUE_MARKER'),
				React.createElement(Text, null, 'Line two'),
			),
			{ terminal: term, exitOnCtrlC: false },
		);

		await wait(50); await term.flush();

		term.resize(40, 10);
		await wait(50); await term.flush();

		const viewport = term.getViewport();
		const markerCount = viewport.filter(l => l.includes('UNIQUE_MARKER')).length;

		instance.unmount();
		expect(markerCount).toBe(1);
	});

	it('static content is not duplicated after resize', async () => {
		const term = new TestTerminal(60, 10);

		function App() {
			const items = ['Static A', 'Static B'];
			return React.createElement(Box, { flexDirection: 'column' },
				React.createElement(Static, { items },
					(msg: string, i: number) => React.createElement(Text, { key: i }, msg),
				),
				React.createElement(Text, null, 'LIVE_CONTENT'),
			);
		}

		const instance = render(React.createElement(App), { terminal: term, exitOnCtrlC: false });
		await wait(50); await term.flush();

		term.resize(40, 10);
		await wait(50); await term.flush();

		const viewport = term.getViewport();
		const liveCount = viewport.filter(l => l.includes('LIVE_CONTENT')).length;
		const staticACount = viewport.filter(l => l.includes('Static A')).length;

		instance.unmount();

		expect(liveCount).toBe(1);
		// Static content may appear 0 or 1 times in viewport (it scrolls into scrollback)
		// but should NOT appear more than once
		expect(staticACount).toBeLessThanOrEqual(1);
	});
});
