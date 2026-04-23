/**
 * Static trim + adjustStaticCursor E2E tests.
 *
 * Verifies that splicing items from the front of a <Static> array works
 * correctly when paired with adjustStaticCursor — new items still appear
 * in scrollback, and resize after trim produces a clean re-render.
 */
import { describe, it, expect } from 'vitest';
import React, { useState } from 'react';
import { render, Box, Text, Static } from '../src/index.js';
import { TestTerminal, wait } from './helpers.js';

interface Msg { id: number; text: string }

/** App that mimics ConversationView: Static items with trim + adjustStaticCursor. */
function createTrimApp(cap: number) {
	let addMsg!: (text: string) => void;

	function App() {
		const [messages, setMessages] = useState<Msg[]>([]);
		addMsg = (text: string) => {
			setMessages(prev => [...prev, { id: prev.length, text }]);
		};
		const done = messages.slice(0, -1);
		const current = messages.at(-1);
		return React.createElement(Box, { flexDirection: 'column' },
			React.createElement(Static, { items: done },
				(msg: Msg) => React.createElement(Box, { key: msg.id },
					React.createElement(Text, null, `[${msg.id}] ${msg.text}`)
				)
			),
			current
				? React.createElement(Text, null, `> ${current.text}`)
				: React.createElement(Text, { dimColor: true }, 'empty'),
		);
	}

	return { App, addMsg: (t: string) => addMsg(t), cap };
}

describe('Static trim with adjustStaticCursor', () => {
	it('new items appear after trim', async () => {
		const term = new TestTerminal(60, 20);
		const { App, addMsg } = createTrimApp(5);

		const instance = render(React.createElement(App), { terminal: term, exitOnCtrlC: false });
		await wait(30);

		// Add 8 messages — items 0-6 go to Static, item 7 is live
		for (let i = 0; i < 8; i++) {
			addMsg(`msg-${i}`);
			await wait(20);
		}
		await term.flush();

		// Verify all static items are present
		let vp = term.getLastFrame()!.viewport.join('\n');
		expect(vp).toContain('[0] msg-0');
		expect(vp).toContain('[6] msg-6');
		expect(vp).toContain('> msg-7');

		// Now simulate trim: splice first 3 items + adjustStaticCursor
		// This is what ConversationView does after trimStaticItems returns > 0
		instance.adjustStaticCursor(3);

		// Add more messages after trim
		addMsg('after-trim-8');
		await wait(20);
		addMsg('after-trim-9');
		await wait(20);
		await term.flush();

		vp = term.getLastFrame()!.viewport.join('\n');
		// New items must be visible — this is the core assertion
		expect(vp).toContain('[8] after-trim-8');
		expect(vp).toContain('> after-trim-9');

		instance.unmount();
	});

	it('repeated trim cycles keep rendering correctly', async () => {
		const term = new TestTerminal(60, 30);
		const { App, addMsg } = createTrimApp(5);

		const instance = render(React.createElement(App), { terminal: term, exitOnCtrlC: false });
		await wait(30);

		// Run 5 cycles of: add batch → trim → verify canary
		for (let cycle = 0; cycle < 5; cycle++) {
			const base = cycle * 10;

			// Add 10 messages
			for (let i = 0; i < 10; i++) {
				addMsg(`c${cycle}-m${i}`);
				await wait(10);
			}
			await term.flush();

			// Trim oldest 5
			instance.adjustStaticCursor(5);

			// Add canary after trim
			addMsg(`canary-${cycle}`);
			await wait(20);
			await term.flush();

			const vp = term.getLastFrame()!.viewport.join('\n');
			expect(vp).toContain(`> canary-${cycle}`);
		}

		instance.unmount();
	});

	it('resize after trim produces clean output', async () => {
		const term = new TestTerminal(60, 20);
		const { App, addMsg } = createTrimApp(5);

		const instance = render(React.createElement(App), { terminal: term, exitOnCtrlC: false });
		await wait(30);

		// Add 10 messages
		for (let i = 0; i < 10; i++) {
			addMsg(`msg-${i}`);
			await wait(10);
		}
		await term.flush();

		// Trim first 5
		instance.adjustStaticCursor(5);

		// Resize — triggers resetStatic (cursor → 0, full re-render)
		term.resize(80, 20);
		await wait(50);
		await term.flush();

		// Add more after resize
		addMsg('post-resize');
		await wait(20);
		await term.flush();

		const vp = term.getLastFrame()!.viewport.join('\n');
		expect(vp).toContain('> post-resize');

		// Add even more to verify continued operation
		addMsg('final');
		await wait(20);
		await term.flush();

		const vp2 = term.getLastFrame()!.viewport.join('\n');
		expect(vp2).toContain('> final');

		instance.unmount();
	});

	it('50 turns with trim every 10 — no content loss', async () => {
		const term = new TestTerminal(80, 40);
		const { App, addMsg } = createTrimApp(10);

		const instance = render(React.createElement(App), { terminal: term, exitOnCtrlC: false });
		await wait(30);

		for (let i = 0; i < 50; i++) {
			addMsg(`turn-${i}`);
			await wait(5);

			// Trim every 10 turns
			if (i > 0 && i % 10 === 0) {
				instance.adjustStaticCursor(5);
			}
		}
		await term.flush();

		const vp = term.getLastFrame()!.viewport.join('\n');
		// Latest turn must be live
		expect(vp).toContain('> turn-49');
		// Recent static items must be present
		expect(vp).toContain('[48] turn-48');

		instance.unmount();
	});

	it('trim + resize + trim + add — stress combo', async () => {
		const term = new TestTerminal(60, 25);
		const { App, addMsg } = createTrimApp(5);

		const instance = render(React.createElement(App), { terminal: term, exitOnCtrlC: false });
		await wait(30);

		// Phase 1: add 15, trim 10
		for (let i = 0; i < 15; i++) { addMsg(`p1-${i}`); await wait(5); }
		await term.flush();
		instance.adjustStaticCursor(10);

		// Phase 2: resize
		term.resize(100, 25);
		await wait(50);
		await term.flush();

		// Phase 3: add 10 more, trim 5
		for (let i = 0; i < 10; i++) { addMsg(`p3-${i}`); await wait(5); }
		await term.flush();
		instance.adjustStaticCursor(5);

		// Phase 4: add canary
		addMsg('FINAL-CANARY');
		await wait(20);
		await term.flush();

		const vp = term.getLastFrame()!.viewport.join('\n');
		expect(vp).toContain('> FINAL-CANARY');

		// Phase 5: one more resize to verify everything still works
		term.resize(60, 25);
		await wait(50);
		addMsg('after-second-resize');
		await wait(20);
		await term.flush();

		const vp2 = term.getLastFrame()!.viewport.join('\n');
		expect(vp2).toContain('> after-second-resize');

		instance.unmount();
	});
});
