import { describe, it, expect } from 'vitest';
import React, { useState } from 'react';
import { TestTerminal, wait } from './helpers.ts';
import { render } from '../src/reconciler/render.js';
import { Text } from '../src/components/Text.js';
import { Box } from '../src/components/Box.js';
import { Static } from '../src/components/Static.js';

/** Read all non-empty lines from the xterm buffer (scrollback + viewport). */
function readFullBuffer(term: TestTerminal): string[] {
	const xterm = (term as any).xterm;
	const buf = xterm.buffer.active;
	const lines: string[] = [];
	for (let i = 0; i < buf.length; i++) {
		const line = buf.getLine(i);
		if (line) lines.push(line.translateToString(true));
	}
	return lines;
}

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

	it('scrollback static content re-wraps to new width on shrink', async () => {
		const term = new TestTerminal(60, 5);
		// 71 chars — wraps to 2 lines at 60, 3 lines at 30
		const longLine = 'This is a long static line that should reflow when the terminal shrinks';

		function App() {
			return React.createElement(Box, { flexDirection: 'column' },
				React.createElement(Static, { items: [longLine, 'Short'] },
					(msg: string, i: number) => React.createElement(Text, { key: i }, msg),
				),
				React.createElement(Text, null, 'LIVE'),
			);
		}

		const instance = render(React.createElement(App), { terminal: term, exitOnCtrlC: false });
		await wait(50); await term.flush();

		// At 60 cols: long line wraps to 2 lines
		const before = readFullBuffer(term);
		const longBefore = before.filter(l => l.includes('long static') || l.includes('terminal shrinks'));
		expect(longBefore).toHaveLength(2);

		// Shrink to 30 cols — should re-wrap to 3 lines
		term.resize(30, 5);
		await wait(50); await term.flush();

		const after = readFullBuffer(term);

		// Every non-empty line must fit within 30 cols
		for (const line of after) {
			if (line.trim()) {
				expect(line.trimEnd().length).toBeLessThanOrEqual(30);
			}
		}

		// Content must still be fully present
		const allText = after.join(' ');
		expect(allText).toContain('long static');
		expect(allText).toContain('terminal shrinks');
		expect(allText).toContain('Short');

		// Live content still visible in viewport
		expect(term.getViewport().some(l => l.includes('LIVE'))).toBe(true);

		instance.unmount();
	});

	it('static content re-wraps from 3 lines to 2 when terminal grows', async () => {
		const term = new TestTerminal(30, 5);
		// At 30 cols this wraps to 3 lines; at 60 cols it fits in 2
		const longLine = 'This is a long static line that should reflow when the terminal shrinks';

		function App() {
			return React.createElement(Box, { flexDirection: 'column' },
				React.createElement(Static, { items: [longLine] },
					(msg: string, i: number) => React.createElement(Text, { key: i }, msg),
				),
				React.createElement(Text, null, 'LIVE'),
			);
		}

		const instance = render(React.createElement(App), { terminal: term, exitOnCtrlC: false });
		await wait(50); await term.flush();

		// At 30 cols: wraps to 3 lines
		const before = readFullBuffer(term);
		const contentBefore = before.filter(l => l.trim() && !l.includes('LIVE'));
		expect(contentBefore.length).toBeGreaterThanOrEqual(3);

		// Grow to 60 cols — should re-wrap to 2 lines
		term.resize(60, 5);
		await wait(50); await term.flush();

		const after = readFullBuffer(term);
		const contentAfter = after.filter(l => l.trim() && !l.includes('LIVE'));
		expect(contentAfter.length).toBeLessThan(contentBefore.length);

		// Content preserved
		const allText = after.join(' ');
		expect(allText).toContain('long static');
		expect(allText).toContain('terminal shrinks');

		instance.unmount();
	});

	it('multiple static items with mixed lengths re-wrap independently', async () => {
		const term = new TestTerminal(60, 10);
		const items = [
			'Short bullet point',
			'A much longer bullet point that definitely wraps at sixty columns but not at one hundred and twenty columns wide',
			'Another short one',
		];

		function App() {
			return React.createElement(Box, { flexDirection: 'column' },
				React.createElement(Static, { items },
					(msg: string, i: number) => React.createElement(Text, { key: i }, msg),
				),
				React.createElement(Text, null, 'LIVE'),
			);
		}

		const instance = render(React.createElement(App), { terminal: term, exitOnCtrlC: false });
		await wait(50); await term.flush();

		// Shrink to 30 cols
		term.resize(30, 10);
		await wait(50); await term.flush();

		const after = readFullBuffer(term);

		// All lines fit within 30 cols
		for (const line of after) {
			if (line.trim()) {
				expect(line.trimEnd().length).toBeLessThanOrEqual(30);
			}
		}

		// All content preserved
		const allText = after.join(' ');
		expect(allText).toContain('Short bullet');
		expect(allText).toContain('longer bullet');
		expect(allText).toContain('Another short');

		instance.unmount();
	});

	it('static content is in scrollback buffer not live area after flush', async () => {
		const term = new TestTerminal(60, 5);
		let addMsg!: (text: string) => void;

		function App() {
			const [messages, setMessages] = useState<string[]>([]);
			addMsg = (text: string) => setMessages(m => [...m, text]);

			return React.createElement(Box, { flexDirection: 'column' },
				React.createElement(Static, { items: messages },
					(msg: string, i: number) => React.createElement(Text, { key: i }, msg),
				),
				React.createElement(Text, null, 'LIVE'),
			);
		}

		const instance = render(React.createElement(App), { terminal: term, exitOnCtrlC: false });
		await wait(50); await term.flush();

		// Add a long message that wraps
		addMsg('First message is long enough to wrap at sixty columns for sure and then some more text');
		await wait(50); await term.flush();

		// Verify it's in the buffer
		const bufferBefore = readFullBuffer(term);
		expect(bufferBefore.join(' ')).toContain('First message');

		// Resize — static content should re-render at new width
		term.resize(30, 5);
		await wait(50); await term.flush();

		const bufferAfter = readFullBuffer(term);

		// Content preserved and fits new width
		for (const line of bufferAfter) {
			if (line.trim()) {
				expect(line.trimEnd().length).toBeLessThanOrEqual(30);
			}
		}
		expect(bufferAfter.join(' ')).toContain('First message');

		// Live area still works
		expect(term.getViewport().some(l => l.includes('LIVE'))).toBe(true);

		instance.unmount();
	});
});
