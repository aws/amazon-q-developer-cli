/**
 * 02-counter.tsx — Interactive counter with keypress handling
 *
 * Run: npx tsx examples/02-counter.tsx
 *
 * Controls:
 *   ↑ / k    Increment
 *   ↓ / j    Decrement
 *   r        Reset
 *   q        Quit
 */
import React, { useState } from 'react';
import { render, Text, Box, useInput, useApp } from 'twinki';

const Counter = () => {
	const [count, setCount] = useState(0);
	const { exit } = useApp();

	useInput((input, key) => {
		if (key.upArrow || input === 'k') setCount(c => c + 1);
		if (key.downArrow || input === 'j') setCount(c => c - 1);
		if (input === 'r') setCount(0);
		if (input === 'q') exit();
	});

	const color = count > 0 ? 'green' : count < 0 ? 'red' : 'yellow';

	return (
		<Box flexDirection="column">
			<Text bold>Counter</Text>
			<Text> </Text>
			<Text>  Value: <Text color={color} bold>{String(count)}</Text></Text>
			<Text> </Text>
			<Text dimColor>  ↑/k increment  ↓/j decrement  r reset  q quit</Text>
		</Box>
	);
};

render(<Counter />);
