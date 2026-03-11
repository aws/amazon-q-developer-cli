/**
 * 24-syntax-highlight.tsx — Syntax-highlighted editor with theme switcher
 *
 * Run: npx tsx examples/24-syntax-highlight.tsx
 *
 * Controls: e=edit, Escape=stop editing, Tab=next theme, Shift+Tab=prev theme, q=quit
 */
import React, { useState } from 'react';
import { render, Text, Box, useApp, useInput, EditorInput } from 'twinki';

const THEMES = [
	'monokai',
	'dracula',
	'github-dark',
	'github-light',
	'catppuccin-mocha',
	'catppuccin-latte',
	'one-dark-pro',
	'nord',
	'vitesse-dark',
	'tokyo-night',
] as const;

const SAMPLE = `import React, { useState } from 'react';
import { render, Text, Box, useApp, useInput } from 'twinki';

interface CounterProps {
  initial?: number;
}

const Counter: React.FC<CounterProps> = ({ initial = 0 }) => {
  const { exit } = useApp();
  const [count, setCount] = useState(initial);

  useInput((ch, key) => {
    if (ch === 'q' || key.escape) exit();
    if (ch === '+' || key.upArrow) setCount(n => n + 1);
    if (ch === '-' || key.downArrow) setCount(n => Math.max(0, n - 1));
  });

  const color = count > 10 ? 'red' : count > 5 ? 'yellow' : 'green';

  return (
    <Box flexDirection="column" padding={1}>
      <Text bold>Counter Demo</Text>
      <Text color={color}>Count: {count}</Text>
      <Text dimColor>+/↑ increment  -/↓ decrement  q quit</Text>
    </Box>
  );
};

render(<Counter initial={0} />);`;

function App() {
	const { exit } = useApp();
	const [themeIdx, setThemeIdx] = useState(0);
	const [editing, setEditing] = useState(false);
	const [value, setValue] = useState(SAMPLE);

	useInput((ch, key) => {
		if (editing) {
			if (key.escape) setEditing(false);
			return;
		}
		if (ch === 'q') exit();
		if (ch === 'e') { setEditing(true); return; }
		if (key.tab && !key.shift) setThemeIdx(i => (i + 1) % THEMES.length);
		if (key.tab && key.shift) setThemeIdx(i => (i - 1 + THEMES.length) % THEMES.length);
	});

	const theme = THEMES[themeIdx]!;

	return (
		<Box flexDirection="column">
			<Box paddingX={1} marginBottom={0}>
				<Text bold> counter.tsx  </Text>
				<Text dimColor>theme: </Text>
				<Text color="cyan">{theme}</Text>
				<Text dimColor>  ({themeIdx + 1}/{THEMES.length})</Text>
				{editing && <Text color="yellow">  [editing]</Text>}
			</Box>
			<EditorInput
				value={value}
				isActive={editing}
				onChange={setValue}
				syntaxHighlight="tsx"
				syntaxTheme={theme}
				lineNumbers
				disableSubmit
			/>
			<Box paddingX={1} marginTop={1}>
				{editing
					? <Text dimColor>Escape=stop editing</Text>
					: <Text dimColor>e=edit  Tab=next theme  Shift+Tab=prev  q=quit</Text>
				}
			</Box>
		</Box>
	);
}

render(<App />);
