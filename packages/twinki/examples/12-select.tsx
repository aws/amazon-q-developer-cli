/**
 * 12-select.tsx — Select list demo
 *
 * Run: npx tsx examples/12-select.tsx
 *
 * Controls:
 *   ↑/↓      Navigate
 *   Enter     Select item
 *   Escape    Quit
 */
import React, { useState } from 'react';
import { render, Text, Box, Select, useApp } from 'twinki';

const LANGUAGES = [
	{ value: 'typescript', label: 'TypeScript', description: 'Typed JavaScript' },
	{ value: 'rust', label: 'Rust', description: 'Systems programming' },
	{ value: 'go', label: 'Go', description: 'Simple and fast' },
	{ value: 'python', label: 'Python', description: 'General purpose' },
	{ value: 'zig', label: 'Zig', description: 'Low-level control' },
	{ value: 'elixir', label: 'Elixir', description: 'Functional and concurrent' },
	{ value: 'swift', label: 'Swift', description: 'Apple ecosystem' },
	{ value: 'kotlin', label: 'Kotlin', description: 'Modern JVM language' },
];

const App = () => {
	const { exit } = useApp();
	const [selected, setSelected] = useState<string | null>(null);

	if (selected) {
		return (
			<Box flexDirection="column">
				<Text color="green">You selected: {selected}</Text>
			</Box>
		);
	}

	return (
		<Box flexDirection="column">
			<Text bold>Pick your favorite language:</Text>
			<Text> </Text>
			<Select
				items={LANGUAGES}
				maxVisible={5}
				onSelect={(item) => setSelected(item.label)}
				onCancel={() => exit()}
			/>
		</Box>
	);
};

render(<App />);
