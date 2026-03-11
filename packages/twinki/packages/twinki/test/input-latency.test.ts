import { describe, it, expect } from 'vitest';
import React, { useState } from 'react';
import { TestTerminal, wait } from './helpers.ts';
import { render } from '../src/reconciler/render.js';
import { Text } from '../src/components/Text.js';
import { Box } from '../src/components/Box.js';
import { Static } from '../src/components/Static.js';

function generateWords(count: number): string {
	const words = ['the', 'quick', 'brown', 'fox', 'jumps', 'over', 'lazy', 'dog', 'hello', 'world', 'function', 'const', 'return', 'import', 'export'];
	const result: string[] = [];
	for (let i = 0; i < count; i++) result.push(words[i % words.length]!);
	return result.join(' ');
}

describe('Input latency with large buffer', () => {
	for (const wordCount of [2000, 4000, 8000]) {
		it(`${wordCount} words in static + typing in input`, async () => {
			const term = new TestTerminal(120, 40);
			let setInput!: (v: string) => void;

			// Pre-generate static content as multiple messages
			const messages = [];
			const wordsPerMsg = 200;
			for (let i = 0; i < wordCount; i += wordsPerMsg) {
				messages.push(generateWords(Math.min(wordsPerMsg, wordCount - i)));
			}

			function App() {
				const [input, _setInput] = useState('');
				setInput = _setInput;
				return React.createElement(Box, { flexDirection: 'column' },
					React.createElement(Static, { items: messages },
						(msg: string, i: number) => React.createElement(Text, { key: i, wrap: 'wrap' }, msg),
					),
					React.createElement(Text, null, '─'.repeat(80)),
					React.createElement(Text, null, 'status bar'),
					React.createElement(Text, null, input || 'type here...'),
				);
			}

			const instance = render(React.createElement(App), { terminal: term, exitOnCtrlC: false });
			await wait(50);
			await term.flush();

			// Reset metrics after initial render
			const m0 = instance.getMetrics();

			// Simulate 20 keystrokes
			const keystrokes = 'hello world testing';
			const renderTimes: number[] = [];

			for (const char of keystrokes) {
				const before = instance.getMetrics();
				setInput((prev: string) => prev + char);
				await wait(5);
				await term.flush();
				const after = instance.getMetrics();
				if (after.renderCount > before.renderCount) {
					renderTimes.push(after.lastRenderMs);
				}
			}

			instance.unmount();

			const avg = renderTimes.reduce((a, b) => a + b, 0) / renderTimes.length;
			const max = Math.max(...renderTimes);
			const p95 = renderTimes.sort((a, b) => a - b)[Math.floor(renderTimes.length * 0.95)]!;

			console.log(`  ${wordCount} words buffer: avg ${avg.toFixed(2)}ms, max ${max.toFixed(2)}ms, p95 ${p95.toFixed(2)}ms per keystroke (${renderTimes.length} renders)`);

			// Input should feel responsive — under 16ms for 60fps
			if (wordCount <= 4000) expect(avg).toBeLessThan(50);
		});
	}
});
