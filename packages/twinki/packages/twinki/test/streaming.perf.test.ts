import { describe, it, expect } from 'vitest';
import React, { useState } from 'react';
import { TestTerminal, wait } from './helpers.ts';
import { render } from '../src/reconciler/render.js';
import { Text } from '../src/components/Text.js';
import { Box } from '../src/components/Box.js';

function generateWords(count: number): string {
	const words = ['the', 'quick', 'brown', 'fox', 'jumps', 'over', 'lazy', 'dog', 'hello', 'world', 'function', 'const', 'return', 'import', 'export', 'async', 'await', 'class', 'interface', 'type'];
	const result: string[] = [];
	for (let i = 0; i < count; i++) result.push(words[i % words.length]!);
	return result.join(' ');
}

describe('Streaming performance', () => {
	for (const wordCount of [500, 1000, 2000, 4000, 10000]) {
		it(`${wordCount} words`, async () => {
			const term = new TestTerminal(120, 40);
			const words = generateWords(wordCount).split(' ');
			let setText!: (t: string) => void;

			function App() {
				const [text, _setText] = useState('');
				setText = _setText;
				return React.createElement(Box, { flexDirection: 'column' },
					React.createElement(Text, { wrap: 'wrap' }, text),
				);
			}

			const instance = render(React.createElement(App), { terminal: term, exitOnCtrlC: false });
			await wait(10);

			const chunkSize = Math.max(1, Math.floor(wordCount / 100));
			let accumulated = '';

			for (let i = 0; i < words.length; i += chunkSize) {
				accumulated += (accumulated ? ' ' : '') + words.slice(i, i + chunkSize).join(' ');
				setText(accumulated);
				await wait(1);
			}

			await wait(20);
			await term.flush();

			const m = instance.getMetrics();
			instance.unmount();

			console.log(`  ${wordCount} words: ${m.renderCount} renders, avg ${(m.totalRenderMs / m.renderCount).toFixed(2)}ms, max ${m.maxRenderMs.toFixed(2)}ms, total ${m.totalRenderMs.toFixed(0)}ms, full redraws ${m.fullRedrawCount}`);

			expect(m.renderCount).toBeGreaterThan(0);
		});
	}
});
