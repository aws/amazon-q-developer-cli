import React from 'react';
import { describe, expect, it } from 'vitest';
import { Box, EditorInput, Text, render } from '../src/index.js';
import { TestTerminal, wait } from './helpers.js';

describe('initial React render', () => {
	it('commits content before the first synchronized terminal frame', async () => {
		const terminal = new TestTerminal(60, 8);
		const writes: string[] = [];
		const write = terminal.write.bind(terminal);
		terminal.write = (data: string) => {
			writes.push(data);
			write(data);
		};
		const instance = render(
			<Box flexDirection="column">
				<Text>initial header</Text>
				<EditorInput value="initial draft" visibleLines={1} />
			</Box>,
			{ terminal, exitOnCtrlC: false },
		);

		try {
			await wait(30);
			await terminal.flush();
			const frames = writes.filter((data) => data.includes('\x1b[?2026l'));

			expect(frames).not.toHaveLength(0);
			expect(frames[0]).toContain('initial header');
			expect(frames[0]).toContain('initial draft');
		} finally {
			instance.unmount();
		}
	});
});
