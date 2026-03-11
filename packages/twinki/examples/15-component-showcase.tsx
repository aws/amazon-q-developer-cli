/**
 * 15-component-showcase.tsx — Full component showcase
 *
 * Run: npx tsx examples/15-component-showcase.tsx
 *
 * Controls:
 *   Ctrl+N / Ctrl+P    Switch tabs
 *   Escape             Quit
 *
 * Shows every twinki component with prop permutations.
 * Theme: Monokai Pro inspired palette.
 */
import React, { useState, useEffect } from 'react';
import {
	render, Text, Box, Newline, Spacer,
	TextInput, Select, EditorInput, Markdown, Typewriter, DiffView,
	useApp, useTwinkiContext, matchesKey,
} from 'twinki';
import type { AutocompleteProvider, SelectItem } from 'twinki';

// ── Monokai Pro palette ──────────────────────────────────────────

const c = {
	fg:      '#fcfcfa',
	dim:     '#727072',
	red:     '#ff6188',
	orange:  '#fc9867',
	yellow:  '#ffd866',
	green:   '#a9dc76',
	blue:    '#78dce8',
	purple:  '#ab9df2',
	bg:      '#2d2a2e',
	bgLight: '#403e41',
};

// ── Tab definitions ──────────────────────────────────────────────

const TABS = ['Text', 'Box', 'Layout', 'Markdown', 'Typewriter', 'DiffView', 'TextInput', 'Select', 'EditorInput'] as const;
type Tab = typeof TABS[number];

// ── Shared data ──────────────────────────────────────────────────

const FRAMEWORKS: SelectItem[] = [
	{ value: 'react', label: 'React', description: 'UI library' },
	{ value: 'vue', label: 'Vue', description: 'Progressive framework' },
	{ value: 'svelte', label: 'Svelte', description: 'Compiled framework' },
	{ value: 'solid', label: 'SolidJS', description: 'Fine-grained reactivity' },
	{ value: 'angular', label: 'Angular', description: 'Full platform' },
	{ value: 'preact', label: 'Preact', description: 'Lightweight React' },
	{ value: 'qwik', label: 'Qwik', description: 'Resumable framework' },
	{ value: 'htmx', label: 'htmx', description: 'HTML-first approach' },
];

const EMOJIS: SelectItem[] = [
	{ value: 'smile', label: 'smile' },
	{ value: 'wave', label: 'wave' },
	{ value: 'fire', label: 'fire' },
	{ value: 'rocket', label: 'rocket' },
];

const emojiProvider: AutocompleteProvider = {
	getSuggestions(lines, cursorLine, cursorCol) {
		const line = lines[cursorLine] || '';
		const before = line.slice(0, cursorCol);
		const match = before.match(/:(\w*)$/);
		if (!match) return null;
		const prefix = match[0];
		const items = EMOJIS.filter(e => e.value.startsWith(match[1]));
		return items.length > 0 ? { items, prefix } : null;
	},
	applyCompletion(lines, cursorLine, cursorCol, item, prefix) {
		const line = lines[cursorLine] || '';
		const newLine = line.slice(0, cursorCol - prefix.length) + ':' + item.value + ': ' + line.slice(cursorCol);
		const newLines = [...lines];
		newLines[cursorLine] = newLine;
		return { lines: newLines, cursorLine, cursorCol: cursorCol - prefix.length + item.value.length + 3 };
	},
};

const Hint = ({ children }: { children: string }) => <Text color={c.dim}>{children}</Text>;
const Section = ({ label, children }: { label: string; children: React.ReactNode }) => (
	<Box flexDirection="column">
		<Text color={c.dim}>{label}</Text>
		{children}
	</Box>
);

// ── Text ─────────────────────────────────────────────────────────

const TextDemo = () => (
	<Box flexDirection="column">
		<Text color={c.orange} bold>Text</Text>
		<Hint>Styling props: bold, dim, italic, underline, strikethrough, inverse, color, backgroundColor</Hint>
		<Text> </Text>
		<Box flexDirection="column">
			<Text bold color={c.fg}>bold</Text>
			<Text dimColor>dimColor</Text>
			<Text italic color={c.fg}>italic</Text>
			<Text underline color={c.fg}>underline</Text>
			<Text strikethrough color={c.fg}>strikethrough</Text>
			<Text inverse> inverse </Text>
		</Box>
		<Text> </Text>
		<Hint>Colors:</Hint>
		<Box flexDirection="row">
			<Text color={c.red}>{'  red  '}</Text>
			<Text color={c.orange}>{'  orange  '}</Text>
			<Text color={c.yellow}>{'  yellow  '}</Text>
			<Text color={c.green}>{'  green  '}</Text>
			<Text color={c.blue}>{'  blue  '}</Text>
			<Text color={c.purple}>{'  purple  '}</Text>
		</Box>
		<Text> </Text>
		<Text bold italic underline color={c.purple}>combined: bold + italic + underline</Text>
		<Text> </Text>
		<Hint>wrap modes (width=30):</Hint>
		<Box width={32} borderStyle="round" borderColor={c.dim}>
			<Text wrap="wrap" color={c.fg}>wrap: This text wraps to the next line at word boundaries</Text>
		</Box>
		<Box width={32} borderStyle="round" borderColor={c.dim}>
			<Text wrap="truncate" color={c.fg}>truncate: This text gets cut off at the end...</Text>
		</Box>
		<Box width={32} borderStyle="round" borderColor={c.dim}>
			<Text wrap="truncate-middle" color={c.fg}>truncate-middle: Cut in the middle of the text here</Text>
		</Box>
	</Box>
);

// ── Box ──────────────────────────────────────────────────────────

const BoxDemo = () => (
	<Box flexDirection="column">
		<Text color={c.orange} bold>Box</Text>
		<Hint>Layout container with flex, borders, padding, margin</Hint>
		<Text> </Text>
		<Section label="flexDirection=row:">
			<Box flexDirection="row" width={36} borderStyle="round" borderColor={c.dim}>
				<Box borderStyle="round" borderColor={c.red}><Text color={c.red}> A </Text></Box>
				<Box borderStyle="round" borderColor={c.green}><Text color={c.green}> B </Text></Box>
				<Box borderStyle="round" borderColor={c.blue}><Text color={c.blue}> C </Text></Box>
			</Box>
		</Section>
		<Section label="flexDirection=column:">
			<Box flexDirection="column" width={36} borderStyle="round" borderColor={c.dim}>
				<Box borderStyle="round" borderColor={c.red}><Text color={c.red}> A </Text></Box>
				<Box borderStyle="round" borderColor={c.green}><Text color={c.green}> B </Text></Box>
				<Box borderStyle="round" borderColor={c.blue}><Text color={c.blue}> C </Text></Box>
			</Box>
		</Section>
		<Section label="justifyContent=space-between:">
			<Box flexDirection="row" justifyContent="space-between" width={36} borderStyle="round" borderColor={c.dim}>
				<Text color={c.yellow}>Left</Text>
				<Text color={c.purple}>Right</Text>
			</Box>
		</Section>
		<Section label="borderStyle variants:">
			<Box flexDirection="row">
				<Box borderStyle="single" borderColor={c.blue} width={11}><Text color={c.fg}>single</Text></Box>
				<Box borderStyle="round" borderColor={c.green} width={11}><Text color={c.fg}>round</Text></Box>
				<Box borderStyle="double" borderColor={c.orange} width={11}><Text color={c.fg}>double</Text></Box>
				<Box borderStyle="bold" borderColor={c.red} width={11}><Text color={c.fg}>bold</Text></Box>
			</Box>
		</Section>
		<Section label="padding=1:">
			<Box borderStyle="round" borderColor={c.dim} width={28} padding={1}>
				<Text color={c.green}>Padded content</Text>
			</Box>
		</Section>
	</Box>
);

// ── Layout (Newline / Spacer) ────────────────────────────────────

const LayoutDemo = () => (
	<Box flexDirection="column">
		<Text color={c.orange} bold>Newline + Spacer</Text>
		<Text> </Text>
		<Section label="Newline inserts vertical space:">
			<Text color={c.fg}>Line 1</Text>
			<Newline />
			<Text color={c.fg}>Line 3 (after Newline)</Text>
		</Section>
		<Text> </Text>
		<Section label="Spacer pushes content apart:">
			<Box flexDirection="row" width={40} borderStyle="round" borderColor={c.dim}>
				<Text color={c.green}>Left</Text>
				<Spacer />
				<Text color={c.red}>Right</Text>
			</Box>
		</Section>
		<Section label="Multiple spacers distribute evenly:">
			<Box flexDirection="row" width={40} borderStyle="round" borderColor={c.dim}>
				<Text color={c.red}>A</Text>
				<Spacer />
				<Text color={c.yellow}>B</Text>
				<Spacer />
				<Text color={c.blue}>C</Text>
			</Box>
		</Section>
	</Box>
);

// ── Markdown ─────────────────────────────────────────────────────

const MD_SAMPLE = `# Heading 1
## Heading 2

**Bold** and *italic* and \`inline code\`.

- First item
- Second item
- Third item

\`\`\`typescript
const greeting: string = "hello";
console.log(greeting);
\`\`\`

> A blockquote
`;

const MarkdownDemo = () => (
	<Box flexDirection="column">
		<Text color={c.orange} bold>Markdown</Text>
		<Hint>Props: children (string), highlight, theme</Hint>
		<Text> </Text>
		<Markdown>{MD_SAMPLE}</Markdown>
	</Box>
);

// ── Typewriter ───────────────────────────────────────────────────

const TypewriterDemo = () => {
	const [key] = useState(0);
	return (
		<Box flexDirection="column">
			<Text color={c.orange} bold>Typewriter</Text>
			<Hint>Props: children (string), speed, markdown, onComplete</Hint>
			<Text> </Text>
			<Hint>speed=fast:</Hint>
			<Typewriter key={`f-${key}`} speed="fast">Streaming in quickly, word by word.</Typewriter>
			<Text> </Text>
			<Hint>speed=natural:</Hint>
			<Typewriter key={`n-${key}`} speed="natural">A natural reading pace, like someone typing.</Typewriter>
			<Text> </Text>
			<Hint>speed=slow:</Hint>
			<Typewriter key={`s-${key}`} speed="slow">Slow and deliberate.</Typewriter>
		</Box>
	);
};

// ── DiffView ─────────────────────────────────────────────────────

const DIFF_OLD = `function greet(name) {
  console.log("Hello " + name);
  return true;
}`;

const DIFF_NEW = `function greet(name: string) {
  console.log(\`Hello \${name}\`);
  console.log("Done");
  return true;
}`;

const DiffViewDemo = () => (
	<Box flexDirection="column">
		<Text color={c.orange} bold>DiffView</Text>
		<Hint>Props: values, layout, highlight, lang, theme</Hint>
		<Text> </Text>
		<DiffView values={[DIFF_OLD, DIFF_NEW]} layout="vertical" />
	</Box>
);

// ── TextInput ────────────────────────────────────────────────────

const TextInputDemo = () => {
	const [log, setLog] = useState<string[]>([]);
	return (
		<Box flexDirection="column">
			<Text color={c.orange} bold>TextInput</Text>
			<Hint>Props: value, placeholder, onSubmit, onEscape, onChange, isActive</Hint>
			<Text> </Text>
			<TextInput
				placeholder="Type something and press Enter..."
				onSubmit={(v) => { if (v.trim()) setLog(p => [...p.slice(-3), v]); }}
			/>
			{log.length > 0 && <Text color={c.green}>Submitted: {log.join(' | ')}</Text>}
			<Text> </Text>
			<Hint>Ctrl+A/E jump | Ctrl+K kill | Ctrl+Y yank | Ctrl+- undo</Hint>
		</Box>
	);
};

// ── Select ───────────────────────────────────────────────────────

const SelectDemo = () => {
	const [picked, setPicked] = useState<string | null>(null);
	return (
		<Box flexDirection="column">
			<Text color={c.orange} bold>Select</Text>
			<Hint>Props: items, maxVisible, onSelect, onCancel, onChange, filter, isActive</Hint>
			<Text> </Text>
			<Select
				items={FRAMEWORKS}
				maxVisible={5}
				onSelect={(item) => setPicked(item.label)}
			/>
			{picked && <Text color={c.green}>Selected: {picked}</Text>}
		</Box>
	);
};

// ── EditorInput ──────────────────────────────────────────────────

const EditorInputDemo = () => {
	const [log, setLog] = useState<string[]>([]);
	return (
		<Box flexDirection="column">
			<Text color={c.orange} bold>EditorInput</Text>
			<Hint>Props: value, onSubmit, onChange, disableSubmit, autocompleteProvider,</Hint>
			<Hint>       autocompleteMaxVisible, paddingX, isActive</Hint>
			<Text> </Text>
			<EditorInput
				autocompleteProvider={emojiProvider}
				paddingX={1}
				onSubmit={(v) => { if (v.trim()) setLog(p => [...p.slice(-2), v.replace(/\n/g, '\\n')]); }}
			/>
			{log.map((msg, i) => <Text key={i} color={c.green}>Submitted: {msg}</Text>)}
			<Text> </Text>
			<Hint>Shift+Enter new line | type :smi for autocomplete</Hint>
		</Box>
	);
};

// ── Tab bar ──────────────────────────────────────────────────────

const TabBar = ({ active }: { active: Tab }) => (
	<Box flexDirection="column">
		<Box>
			{TABS.map((t) => (
				<Box key={t} marginRight={1}>
					{t === active
						? <Text backgroundColor={c.purple} color={c.fg} bold>{` ${t} `}</Text>
						: <Text color={c.dim}>{` ${t} `}</Text>
					}
				</Box>
			))}
		</Box>
		<Text color={c.dim}>Ctrl+N next | Ctrl+P prev | Escape quit</Text>
	</Box>
);

// ── App ──────────────────────────────────────────────────────────

const DEMOS: Record<Tab, React.FC> = {
	'Text': TextDemo,
	'Box': BoxDemo,
	'Layout': LayoutDemo,
	'Markdown': MarkdownDemo,
	'Typewriter': TypewriterDemo,
	'DiffView': DiffViewDemo,
	'TextInput': TextInputDemo,
	'Select': SelectDemo,
	'EditorInput': EditorInputDemo,
};

const App = () => {
	const { exit } = useApp();
	const { tui } = useTwinkiContext();
	const [tabIdx, setTabIdx] = useState(0);
	const tab = TABS[tabIdx];

	useEffect(() => {
		const unsub = tui.addInputListener((data) => {
			if (matchesKey(data, 'ctrl+n')) {
				setTabIdx(i => (i + 1) % TABS.length);
				return { consume: true };
			}
			if (matchesKey(data, 'ctrl+p')) {
				setTabIdx(i => (i - 1 + TABS.length) % TABS.length);
				return { consume: true };
			}
			if (matchesKey(data, 'escape')) {
				exit();
				return { consume: true };
			}
		});
		return unsub;
	}, [tui]);

	const Demo = DEMOS[tab];

	return (
		<Box flexDirection="column">
			<Text color={c.yellow} bold>Twinki Component Showcase</Text>
			<TabBar active={tab} />
			<Text> </Text>
			<Demo />
		</Box>
	);
};

render(<App />, { exitOnCtrlC: true });
