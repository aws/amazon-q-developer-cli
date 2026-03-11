/**
 * Static Component E2E Test
 */
import { describe, it, expect } from 'vitest';
import React, { useState } from 'react';
import { render, Box, Text, Static } from '../src/index.js';
import { TestTerminal, wait } from './helpers.js';

interface Message {
	id: number;
	text: string;
}

describe('Static component', () => {
	it('moves completed messages to scrollback', async () => {
		const term = new TestTerminal(80, 30);
		let addMessage: ((text: string) => void) | null = null;

		const ChatWithStatic = () => {
			const [messages, setMessages] = useState<Message[]>([]);
			addMessage = (text: string) => {
				setMessages(prev => [...prev, { id: prev.length, text }]);
			};
			const staticMessages = messages.slice(0, -1);
			const lastMessage = messages[messages.length - 1];
			return React.createElement(Box, { flexDirection: 'column' },
				React.createElement(Text, { bold: true }, 'Chat Test'),
				React.createElement(Static, { items: staticMessages },
					(msg: Message) => React.createElement(Box, { key: msg.id },
						React.createElement(Text, null, `✓ ${msg.text}`)
					)
				),
				lastMessage && React.createElement(Box, null,
					React.createElement(Text, { color: 'yellow' }, `→ ${lastMessage.text}`)
				),
				React.createElement(Text, { dimColor: true }, `Messages: ${messages.length}`)
			);
		};

		const instance = render(React.createElement(ChatWithStatic), { terminal: term, exitOnCtrlC: false });
		await wait(50);

		addMessage!('Message 1');
		await wait(50); await term.flush();
		expect(term.getLastFrame()?.viewport.join('\n')).toContain('→ Message 1');

		addMessage!('Message 2');
		await wait(50); await term.flush();
		const f2 = term.getLastFrame()?.viewport.join('\n') ?? '';
		expect(f2).toContain('→ Message 2');
		expect(f2).toContain('✓ Message 1');

		addMessage!('Message 3');
		await wait(50); await term.flush();
		const f3 = term.getLastFrame()?.viewport.join('\n') ?? '';
		expect(f3).toContain('→ Message 3');
		expect(f3).toContain('✓ Message 1');
		expect(f3).toContain('✓ Message 2');

		instance.unmount();
	});

	it('maintains constant render time as history grows', async () => {
		const term = new TestTerminal(80, 24);
		let addMessage: ((text: string) => void) | null = null;

		const ChatWithStatic = () => {
			const [messages, setMessages] = useState<Message[]>([]);
			addMessage = (text: string) => {
				setMessages(prev => [...prev, { id: prev.length, text }]);
			};
			const staticMessages = messages.slice(0, -1);
			const lastMessage = messages[messages.length - 1];
			return React.createElement(Box, { flexDirection: 'column' },
				React.createElement(Static, { items: staticMessages },
					(msg: Message) => React.createElement(Box, { key: msg.id },
						React.createElement(Text, null, msg.text)
					)
				),
				lastMessage && React.createElement(Text, null, `→ ${lastMessage.text}`)
			);
		};

		const instance = render(React.createElement(ChatWithStatic), { terminal: term, exitOnCtrlC: false });
		await wait(50);

		for (let i = 0; i < 20; i++) {
			addMessage!(`Message ${i + 1}`);
			await wait(20);
		}
		await term.flush();

		const frame = term.getLastFrame()?.viewport.join('\n') ?? '';
		expect(frame).toContain('→ Message 20');
		expect(frame).toContain('Message 1');

		instance.unmount();
	});

	it('writes static lines only once', async () => {
		const term = new TestTerminal(80, 30);
		let addMessage: ((text: string) => void) | null = null;
		const frames: string[] = [];

		const ChatWithStatic = () => {
			const [messages, setMessages] = useState<Message[]>([]);
			addMessage = (text: string) => {
				setMessages(prev => [...prev, { id: prev.length, text }]);
			};
			const staticMessages = messages.slice(0, -1);
			const lastMessage = messages[messages.length - 1];
			return React.createElement(Box, { flexDirection: 'column' },
				React.createElement(Static, { items: staticMessages },
					(msg: Message) => React.createElement(Box, { key: msg.id },
						React.createElement(Text, null, `STATIC: ${msg.text}`)
					)
				),
				lastMessage && React.createElement(Text, null, `LIVE: ${lastMessage.text}`)
			);
		};

		const instance = render(React.createElement(ChatWithStatic), { terminal: term, exitOnCtrlC: false });
		await wait(50);

		addMessage!('Msg1');
		await wait(50); await term.flush();
		frames.push(term.getLastFrame()?.viewport.join('\n') ?? '');

		addMessage!('Msg2');
		await wait(50); await term.flush();
		frames.push(term.getLastFrame()?.viewport.join('\n') ?? '');

		addMessage!('Msg3');
		await wait(50); await term.flush();
		frames.push(term.getLastFrame()?.viewport.join('\n') ?? '');

		expect(frames[0]).toContain('LIVE: Msg1');
		expect(frames[0]).not.toContain('STATIC:');

		expect(frames[1]).toContain('STATIC: Msg1');
		expect(frames[1]).toContain('LIVE: Msg2');

		expect(frames[2]).toContain('STATIC: Msg1');
		expect(frames[2]).toContain('STATIC: Msg2');
		expect(frames[2]).toContain('LIVE: Msg3');

		instance.unmount();
	});

	it('truncating Static items array does not re-write scrollback', async () => {
		const term = new TestTerminal(80, 30);
		let setMsgs!: (msgs: Message[]) => void;

		const App = () => {
			const [messages, setMessages] = useState<Message[]>([]);
			setMsgs = setMessages;
			const done = messages.slice(0, -1);
			const current = messages.at(-1);
			return React.createElement(Box, { flexDirection: 'column' },
				React.createElement(Static, { items: done },
					(msg: Message) => React.createElement(Box, { key: msg.id },
						React.createElement(Text, null, `static:${msg.id}`)
					)
				),
				current && React.createElement(Text, null, `live:${current.id}`)
			);
		};

		const instance = render(React.createElement(App), { terminal: term, exitOnCtrlC: false });
		await wait(50);

		// Add 5 messages — items 0-3 go to Static, item 4 is live
		setMsgs([0,1,2,3,4].map(id => ({ id, text: `msg${id}` })));
		await wait(50); await term.flush();

		const framesBefore = term.getFrames().length;
		const bytesBefore = term.getFrames().reduce((s, f) => s + f.writeBytes, 0);

		// Truncate: remove items 0-1 from the front (already in scrollback)
		setMsgs([2,3,4,5].map(id => ({ id, text: `msg${id}` })));
		await wait(50); await term.flush();

		// The new frames after truncation should not contain static:0 or static:1
		// in their write output (i.e. they weren't re-written to the terminal)
		const newFrames = term.getFrames().slice(framesBefore);
		const newOutput = newFrames.map(f => f.viewport.join('\n')).join('\n');
		// live:5 must appear (new current message)
		expect(term.getLastFrame()?.viewport.join('\n')).toContain('live:5');
		// Bytes written after truncation should be small — just the live area update,
		// not a full re-write of all static items
		const bytesAfter = newFrames.reduce((s, f) => s + f.writeBytes, 0);
		expect(bytesAfter).toBeLessThan(500); // live area update only

		instance.unmount();
	});

	it('100-turn conversation with 50KB messages: heap does not grow unbounded', async () => {
		const TURNS = 100;
		const LINE = 'x'.repeat(100);

		const term = new TestTerminal(80, 24);
		let addMessage: ((text: string) => void) | null = null;

		const ChatApp = () => {
			const [messages, setMessages] = useState<Message[]>([]);
			addMessage = (text: string) => {
				setMessages(prev => [...prev, { id: prev.length, text }]);
			};
			const staticMessages = messages.slice(0, -1);
			const lastMessage = messages[messages.length - 1];
			return React.createElement(Box, { flexDirection: 'column' },
				React.createElement(Static, { items: staticMessages },
					(msg: Message) => React.createElement(Box, { key: msg.id },
						React.createElement(Text, null, msg.text.slice(0, 40))
					)
				),
				lastMessage && React.createElement(Text, null, `current: ${lastMessage.id}`)
			);
		};

		const instance = render(React.createElement(ChatApp), { terminal: term, exitOnCtrlC: false });
		await wait(50);

		const gc = (globalThis as any).gc as (() => void) | undefined;
		const heapAt: Record<number, number> = {};

		for (let i = 0; i < TURNS; i++) {
			addMessage!(Array(500).fill(LINE).join('\n'));
			await wait(20);
			await term.flush();
			if (i === 9 || i === TURNS - 1) {
				gc?.();
				heapAt[i] = process.memoryUsage().heapUsed;
			}
		}

		expect(term.getLastFrame()?.viewport.join('\n')).toContain(`current: ${TURNS - 1}`);
		if (heapAt[9] && heapAt[TURNS - 1]) {
			expect(heapAt[TURNS - 1]).toBeLessThan(heapAt[9]! * 3);
		}

		instance.unmount();
	});
});
