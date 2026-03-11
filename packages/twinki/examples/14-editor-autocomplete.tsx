/**
 * 14-editor-autocomplete.tsx — Editor with autocomplete demo
 *
 * Run: npx tsx examples/14-editor-autocomplete.tsx
 *
 * Controls:
 *   Tab             Trigger autocomplete
 *   ↑/↓             Navigate suggestions
 *   Enter/Tab       Accept suggestion
 *   Escape          Cancel autocomplete
 *   Enter           Submit (when no autocomplete)
 */
import React, { useState } from 'react';
import { render, Text, Box, EditorInput, useApp } from 'twinki';
import type { AutocompleteProvider, SelectItem } from 'twinki';

const COMMANDS: SelectItem[] = [
	{ value: '/help', label: '/help', description: 'Show available commands' },
	{ value: '/clear', label: '/clear', description: 'Clear message history' },
	{ value: '/exit', label: '/exit', description: 'Exit the application' },
	{ value: '/theme', label: '/theme', description: 'Change color theme' },
	{ value: '/export', label: '/export', description: 'Export conversation' },
	{ value: '/model', label: '/model', description: 'Switch AI model' },
];

const commandProvider: AutocompleteProvider = {
	getSuggestions(lines, cursorLine, cursorCol) {
		const line = lines[cursorLine] || '';
		const before = line.slice(0, cursorCol);
		if (!before.startsWith('/')) return null;
		const prefix = before;
		const items = COMMANDS.filter(c => c.value.startsWith(prefix.toLowerCase()));
		return items.length > 0 ? { items, prefix } : null;
	},
	applyCompletion(lines, cursorLine, cursorCol, item, prefix) {
		const line = lines[cursorLine] || '';
		const newLine = item.value + line.slice(cursorCol);
		const newLines = [...lines];
		newLines[cursorLine] = newLine;
		return { lines: newLines, cursorLine, cursorCol: item.value.length };
	},
};

const App = () => {
	const { exit } = useApp();
	const [output, setOutput] = useState<string[]>([]);

	return (
		<Box flexDirection="column">
			<Text bold>Editor with Autocomplete</Text>
			<Text dimColor>Type / to see commands, Tab to complete. Enter to submit.</Text>
			<Text> </Text>
			{output.map((msg, i) => (
				<Text key={i} color="cyan">{'> '}{msg}</Text>
			))}
			<EditorInput
				autocompleteProvider={commandProvider}
				onSubmit={(value) => {
					if (value === '/exit') { exit(); return; }
					if (value === '/clear') { setOutput([]); return; }
					if (value.trim()) setOutput(prev => [...prev, value]);
				}}
			/>
		</Box>
	);
};

render(<App />, { exitOnCtrlC: true });
