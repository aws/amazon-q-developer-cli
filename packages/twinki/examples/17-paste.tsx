/**
 * 17-paste.tsx — Paste event demo
 *
 * Run: npx tsx examples/17-paste.tsx
 *
 * Paste text into the terminal (Cmd+V / Ctrl+Shift+V).
 * Shows line count, char count, and a preview of pasted content.
 * Escape to quit.
 */
import React, { useState } from 'react';
import { render, Text, Box, useInput, usePaste, useApp } from 'twinki';

const c = { dim: '#727072', fg: '#fcfcfa', green: '#a9dc76', blue: '#78dce8', yellow: '#ffd866', purple: '#ab9df2' };

const App = () => {
	const { exit } = useApp();
	const [pastes, setPastes] = useState<string[]>([]);

	useInput((_, key) => { if (key.escape) exit(); });

	usePaste((content) => {
		setPastes(prev => [...prev, content]);
	});

	const last = pastes[pastes.length - 1];
	const lines = last?.split('\n') ?? [];
	const preview = lines.slice(0, 8);

	return (
		<Box flexDirection="column">
			<Text color={c.yellow} bold>Paste Demo</Text>
			<Text color={c.dim}>Paste text into the terminal. Escape to quit.</Text>
			<Text> </Text>
			<Text color={c.fg}>Pastes received: <Text color={c.blue}>{pastes.length}</Text></Text>
			{last ? (
				<Box flexDirection="column">
					<Text color={c.fg}>Last paste: <Text color={c.green}>{lines.length} lines</Text>, <Text color={c.green}>{last.length} chars</Text></Text>
					<Text> </Text>
					<Text color={c.purple} bold>Preview:</Text>
					{preview.map((line, i) => (
						<Text key={i} color={c.dim}>  {line.slice(0, 70)}{line.length > 70 ? '...' : ''}</Text>
					))}
					{lines.length > 8 ? <Text color={c.dim}>  ... ({lines.length - 8} more lines)</Text> : null}
				</Box>
			) : null}
		</Box>
	);
};

render(<App />);
