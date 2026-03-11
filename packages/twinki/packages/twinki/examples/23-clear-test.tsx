/**
 * Reproduces the kiro-cli !clear scenario:
 * - <Static> with append-only items (welcome + messages)
 * - Live area with active turn + status bar + prompt
 * - Press 'c' to simulate !clear (remove old messages, keep last turn)
 * - Press 'n' to add a new message turn
 * - Press 'q' to quit
 */
import React, { useState, useRef } from 'react';
import { render, Box, Text, Static, useInput } from '../src/index.js';

interface StaticItem {
	id: string;
	text: string;
}

let turnCounter = 0;

function App() {
	const staticItemsRef = useRef<StaticItem[]>([
		{ id: 'welcome', text: '=== WELCOME LOGO ===' },
	]);
	const [liveMessages, setLiveMessages] = useState<string[]>([
		'User: hello',
		'AI: Hi there!',
	]);
	const [, forceRender] = useState(0);

	useInput((input) => {
		if (input === 'q') {
			process.exit(0);
		}
		if (input === 'n') {
			// Add new turn — flush current live to static, start new live
			turnCounter++;
			for (const msg of liveMessages) {
				staticItemsRef.current.push({
					id: `msg-${turnCounter}-${msg}`,
					text: msg,
				});
			}
			staticItemsRef.current.push({
				id: `divider-${turnCounter}`,
				text: '─'.repeat(40),
			});
			setLiveMessages([
				`User: message ${turnCounter}`,
				`AI: response ${turnCounter}`,
			]);
		}
		if (input === 'c') {
			// Simulate !clear — static items NEVER shrink (like kiro-cli)
			// Just replace live messages with minimal content
			setLiveMessages(['User: (after clear)']);
			forceRender((n) => n + 1);
		}
	});

	const staticItems = [...staticItemsRef.current];

	return (
		<Box flexDirection="column">
			<Static items={staticItems}>
				{(item) => (
					<Text key={item.id}>{item.text}</Text>
				)}
			</Static>

			{/* Live area — like kiro-cli's active turn + chrome */}
			<Text>{'─'.repeat(50)}</Text>
			{liveMessages.map((msg, i) => (
				<Text key={`live-${i}`}>{msg}</Text>
			))}
			<Text>{'─'.repeat(50)}</Text>
			<Text color="green">status: model · 1%</Text>
			<Text dimColor>ask a question ↵</Text>
		</Box>
	);
}

const instance = render(<App />, { exitOnCtrlC: true });
