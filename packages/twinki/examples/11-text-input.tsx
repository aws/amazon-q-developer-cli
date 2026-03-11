/**
 * 11-text-input.tsx — Single-line text input demo
 *
 * Run: npx tsx examples/11-text-input.tsx
 *
 * Features:
 *   Typing        Insert characters at cursor
 *   Ctrl+A/E      Jump to start/end
 *   Ctrl+K/U      Kill to end/start of line
 *   Ctrl+Y        Yank (paste from kill ring)
 *   Ctrl+W        Delete word backward
 *   Ctrl+-        Undo
 *   Enter         Submit
 *   Escape        Quit
 */
import React, { useState } from 'react';
import { render, Text, Box, TextInput, useApp } from 'twinki';

const App = () => {
	const { exit } = useApp();
	const [messages, setMessages] = useState<string[]>([]);

	return (
		<Box flexDirection="column">
			<Text bold>Chat Input Demo</Text>
			<Text dimColor>Type a message and press Enter. Escape to quit.</Text>
			<Text> </Text>
			{messages.map((msg, i) => (
				<Text key={i} color="green">{'  '}{msg}</Text>
			))}
			<TextInput
				placeholder="Type a message..."
				onSubmit={(value) => {
					if (value.trim()) {
						setMessages(prev => [...prev, value]);
					}
				}}
				onEscape={() => exit()}
			/>
		</Box>
	);
};

render(<App />);
