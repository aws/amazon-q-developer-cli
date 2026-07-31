import React, { useLayoutEffect } from 'react';
import { describe, expect, it, vi } from 'vitest';
import { Text } from '../src/components/Text.js';
import { useTwinkiContext } from '../src/hooks/context.js';
import { render } from '../src/reconciler/render.js';
import { TestTerminal, wait } from './helpers.js';

const ConsumingApp = ({ onInput }: { onInput: () => void }) => {
	const { tui } = useTwinkiContext();
	useLayoutEffect(
		() =>
			tui.addInputListener(() => {
				onInput();
				return { consume: true };
			}),
		[tui, onInput],
	);
	return <Text>ready</Text>;
};

describe('exitOnCtrlC', () => {
	it('runs before application listeners that consume input', async () => {
		const terminal = new TestTerminal(30, 5);
		const onInput = vi.fn();
		const instance = render(<ConsumingApp onInput={onInput} />, {
			terminal,
			exitOnCtrlC: true,
		});
		await wait(20);

		terminal.sendInput('\x03');
		await instance.waitUntilExit();

		expect(onInput).not.toHaveBeenCalled();
	});
});
