import { describe, it, expect } from 'vitest';
import React, { useState } from 'react';
import { TestTerminal, wait } from './helpers.ts';
import { render } from '../src/reconciler/render.js';
import { Text } from '../src/components/Text.js';
import { Box } from '../src/components/Box.js';
import { Static } from '../src/components/Static.js';

describe('!clear simulation', () => {
	it('live area shrinks while static stays — old live lines erased', async () => {
		const term = new TestTerminal(60, 20);
		let setLiveLines!: (lines: string[]) => void;
		// Static items never shrink (like kiro-cli staticItemsRef)
		const staticItems = ['STATIC_WELCOME', 'STATIC_MSG1'];

		function App() {
			const [liveLines, _setLiveLines] = useState([
				'DIVIDER_1', 'USER: hello', 'AI: response here',
				'DIVIDER_2', 'USER: another', 'AI: more response',
				'STATUS_BAR', 'PROMPT_INPUT',
			]);
			setLiveLines = _setLiveLines;

			return React.createElement(Box, { flexDirection: 'column' },
				React.createElement(Static, { items: staticItems },
					(msg: string, i: number) => React.createElement(Text, { key: i }, msg),
				),
				...liveLines.map((line, i) =>
					React.createElement(Text, { key: `live-${i}` }, line),
				),
			);
		}

		const instance = render(React.createElement(App), { terminal: term, exitOnCtrlC: false });
		await wait(50); await term.flush();

		const before = term.getViewport();
		expect(before.some(l => l.includes('USER: hello'))).toBe(true);
		expect(before.some(l => l.includes('USER: another'))).toBe(true);

		// !clear: keep only last turn + chrome
		setLiveLines(['DIVIDER_1', 'USER: another', 'AI: more response', 'STATUS_BAR', 'PROMPT_INPUT']);
		await wait(50); await term.flush();

		const after = term.getViewport();
		console.log('=== AFTER !clear ===');
		after.forEach((l, i) => console.log(`${String(i).padStart(2)}| ${l}`));

		// 'USER: hello' and 'AI: response here' should be gone
		const helloCount = after.filter(l => l.includes('USER: hello')).length;
		const responseCount = after.filter(l => l.includes('AI: response here')).length;
		// Current content should be present
		const anotherCount = after.filter(l => l.includes('USER: another')).length;
		const promptCount = after.filter(l => l.includes('PROMPT_INPUT')).length;

		instance.unmount();

		expect(helloCount).toBe(0);
		expect(responseCount).toBe(0);
		expect(anotherCount).toBe(1);
		expect(promptCount).toBe(1);
	});
});
