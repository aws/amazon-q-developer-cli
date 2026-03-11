/**
 * 03-spinner.tsx — Animated spinner with timer-driven updates
 *
 * Run: npx tsx examples/03-spinner.tsx
 *
 * Demonstrates:
 *   - setInterval-driven animation
 *   - Differential rendering (only spinner char changes)
 *   - Cleanup on exit
 */
import React, { useState, useEffect } from 'react';
import { render, Text, Box, useApp, useInput } from 'twinki';

const SPINNERS = ['⠋', '⠙', '⠹', '⠸', '⠼', '⠴', '⠦', '⠧', '⠇', '⠏'];

const TASKS = [
	{ name: 'Installing dependencies', duration: 2000 },
	{ name: 'Compiling TypeScript', duration: 1500 },
	{ name: 'Running tests', duration: 3000 },
	{ name: 'Building bundle', duration: 1000 },
];

const Spinner = () => {
	const [frame, setFrame] = useState(0);
	const [taskIdx, setTaskIdx] = useState(0);
	const [completed, setCompleted] = useState<string[]>([]);
	const { exit } = useApp();

	useInput((_, key) => {
		if (key.escape) exit();
	});

	useEffect(() => {
		const spin = setInterval(() => setFrame(f => f + 1), 80);
		return () => clearInterval(spin);
	}, []);

	useEffect(() => {
		if (taskIdx >= TASKS.length) {
			setTimeout(() => exit(), 500);
			return;
		}
		const timer = setTimeout(() => {
			setCompleted(c => [...c, TASKS[taskIdx]!.name]);
			setTaskIdx(i => i + 1);
		}, TASKS[taskIdx]!.duration);
		return () => clearTimeout(timer);
	}, [taskIdx, exit]);

	const done = taskIdx >= TASKS.length;
	const spinner = SPINNERS[frame % SPINNERS.length];

	return (
		<Box flexDirection="column">
			<Text bold>Build Pipeline</Text>
			<Text> </Text>
			{completed.map((name, i) => (
				<Text key={i}>  <Text color="green">✓</Text> {name}</Text>
			))}
			{!done && (
				<Text>  <Text color="yellow">{spinner}</Text> {TASKS[taskIdx]!.name}...</Text>
			)}
			{done && (
				<>
					<Text> </Text>
					<Text bold color="green">All tasks completed!</Text>
				</>
			)}
		</Box>
	);
};

render(<Spinner />);
