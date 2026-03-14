/**
 * Test OSC 8 hyperlinks and OSC 9;4 progress in twinki.
 * Run from packages/twinki: bun run examples/27-osc-test.tsx
 */
import React, { useState, useEffect } from 'react';
import { render, Box, Text, ProcessTerminal } from 'twinki';

const terminal = new ProcessTerminal();

// OSC 8 hyperlink
const hyperlink = (url: string, text: string) =>
	`\x1b]8;;${url}\x07${text}\x1b]8;;\x07`;

// OSC 9;4 progress helpers
const setProgress = (state: number, value?: number) => {
	const seq = value !== undefined
		? `\x1b]9;4;${state};${value}\x07`
		: `\x1b]9;4;${state}\x07`;
	process.stdout.write(seq);
};

const App = () => {
	const [step, setStep] = useState(0);

	useEffect(() => {
		// Cycle through progress states
		const timer = setInterval(() => {
			setStep(s => (s + 1) % 5);
		}, 2000);
		return () => {
			clearInterval(timer);
			setProgress(0); // clear progress on exit
		};
	}, []);

	// Set progress based on step
	useEffect(() => {
		switch (step) {
			case 0: setProgress(3); break;           // indeterminate spinner
			case 1: setProgress(1, 50); break;        // 50% progress
			case 2: setProgress(1, 100); break;       // 100% progress
			case 3: setProgress(4, 75); break;        // warning at 75%
			case 4: setProgress(2); break;            // error
		}
	}, [step]);

	const states = ['Spinner', '50%', '100%', 'Warning 75%', 'Error'];

	return (
		<Box flexDirection="column">
			<Text bold>OSC Test — check terminal tab/title bar</Text>
			<Text> </Text>
			<Text>Progress state: {states[step]} (cycles every 2s)</Text>
			<Text> </Text>
			<Text bold>Hyperlinks (cmd+click or ctrl+click):</Text>
			<Text>  {hyperlink('https://github.com', 'GitHub')}</Text>
			<Text>  {hyperlink('https://example.com', 'Example Site')}</Text>
			<Text>  Mixed: before {hyperlink('https://google.com', 'Google')} after</Text>
			<Text> </Text>
			<Box backgroundColor="#1e4a28">
				<Text>  {hyperlink('https://example.com', 'Link inside bg box')}</Text>
			</Box>
			<Text> </Text>
			<Text dimColor>Press Ctrl+C to exit</Text>
		</Box>
	);
};

const instance = render(<App />, { terminal });

process.stdin.setRawMode(true);
process.stdin.resume();
process.stdin.on('data', (d: Buffer) => {
	if (d[0] === 3) {
		setProgress(0);
		instance.unmount();
		terminal.stop();
		process.exit(0);
	}
});
