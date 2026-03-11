/**
 * 18-fullscreen.tsx — Alternate screen demo
 *
 * Run: npx tsx examples/18-fullscreen.tsx
 *
 * Enters alternate screen buffer (scrollback preserved).
 * Shows a centered box with a counter. Press space to increment.
 * Escape to quit — returns to normal screen with scrollback intact.
 */
import React, { useState } from 'react';
import { render, Text, Box, useInput, useApp } from 'twinki';

const c = { dim: '#727072', fg: '#fcfcfa', green: '#a9dc76', yellow: '#ffd866', purple: '#ab9df2' };

const App = () => {
	const { exit } = useApp();
	const [count, setCount] = useState(0);

	useInput((input, key) => {
		if (key.escape) exit();
		if (input === ' ') setCount(c => c + 1);
	});

	return (
		<Box flexDirection="column">
			<Box borderStyle="round" borderColor={c.purple} flexDirection="column" paddingX={2} paddingY={1}>
				<Text color={c.yellow} bold>Fullscreen Demo</Text>
				<Text> </Text>
				<Text color={c.fg}>This runs in the alternate screen buffer.</Text>
				<Text color={c.fg}>Your scrollback is preserved — press Escape to return.</Text>
				<Text> </Text>
				<Text color={c.fg}>Counter: <Text color={c.green} bold>{count}</Text></Text>
				<Text> </Text>
				<Text color={c.dim}>Space to increment, Escape to quit.</Text>
			</Box>
		</Box>
	);
};

render(<App />, { fullscreen: true });
