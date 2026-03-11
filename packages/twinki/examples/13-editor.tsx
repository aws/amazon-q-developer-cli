/**
 * 13-editor.tsx — Multi-line editor demo
 *
 * Run: npx tsx examples/13-editor.tsx
 *
 * Controls:
 *   Typing          Insert text
 *   Shift+Enter     New line (or \ then Enter)
 *   Enter           Submit
 *   Ctrl+A/E        Jump to start/end of line
 *   Ctrl+K/U        Kill to end/start of line
 *   Ctrl+Y          Yank (paste from kill ring)
 *   Ctrl+W          Delete word backward
 *   Ctrl+-          Undo
 *   ↑/↓             Navigate lines (or history when empty)
 *   Ctrl+]          Character jump forward
 */
import React, { useState } from 'react';
import { render, Text, Box, EditorInput, useApp } from 'twinki';

const App = () => {
	const { exit } = useApp();
	const [messages, setMessages] = useState<string[]>([]);

	return (
		<Box flexDirection="column">
			<Text bold>Multi-line Editor Demo</Text>
			<Text dimColor>Shift+Enter for new line, Enter to submit, Ctrl+C to quit.</Text>
			<Text> </Text>
			{messages.map((msg, i) => (
				<Box key={i} flexDirection="column">
					<Text color="green">Message {i + 1}:</Text>
					<Text>  {msg}</Text>
				</Box>
			))}
			<EditorInput
				onSubmit={(value) => {
					if (value.trim()) setMessages(prev => [...prev, value]);
				}}
			/>
		</Box>
	);
};

render(<App />, { exitOnCtrlC: true });
