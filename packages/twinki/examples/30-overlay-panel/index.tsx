/**
 * 30-overlay-panel — Tabbed panel app with a command-palette overlay.
 *
 * Run: npx tsx examples/30-overlay-panel/index.tsx
 *
 * Demonstrates the core twinki primitives for building a multi-pane app:
 * - Tabs + useTabs: tab strip with keyboard cycling (Ctrl+1..9, Ctrl+Tab)
 * - Split: two-pane layout with adjustable ratio
 * - useOverlay: a floating command-palette panel (Ctrl+p to toggle)
 * - useInput: full keyboard handling (Kitty protocol aware)
 * - Box position="absolute": manual overlay positioning
 *
 * This is a standalone reference for how to compose twinki's new primitives
 * into a real panel-based TUI — no external dependencies beyond twinki.
 */
import React, { useState, useCallback } from 'react';
import { render, Box, Text, Tabs, Split, useTabs, useInput, useApp } from 'twinki';
import type { Tab, InkKey } from 'twinki';

// --- Data --------------------------------------------------------------------

const NOTES = [
	{ id: 'welcome', title: 'Welcome', body: 'This is a tabbed panel app.\n\nCtrl+p opens the command palette.\nCtrl+Tab cycles tabs.\nCtrl+w closes the active tab.\n← → adjusts the split ratio.\nq quits.' },
	{ id: 'todo', title: 'TODO', body: '- [ ] Ship the overlay panel\n- [ ] Write unit tests\n- [x] Build the prototype' },
	{ id: 'ideas', title: 'Ideas', body: 'Floating notifications\nDrag-to-resize splits\nSession multiplexer\nAI chat pane' },
	{ id: 'log', title: 'Log', body: '12:00 — started\n12:05 — opened ideas\n12:10 — adjusted split\n12:15 — done' },
];

// --- Command Palette (the overlay content) -----------------------------------

interface PaletteProps {
	items: typeof NOTES;
	width: number;
	onSelect: (id: string) => void;
	onDismiss: () => void;
}

const Palette: React.FC<PaletteProps> = ({ items, width, onSelect, onDismiss }) => {
	const [query, setQuery] = useState('');
	const [selected, setSelected] = useState(0);

	const filtered = items.filter((n) =>
		n.title.toLowerCase().includes(query.toLowerCase()),
	);

	useInput((input: string, key: InkKey) => {
		if (key.escape) { onDismiss(); return; }
		if (key.return) {
			const item = filtered[selected];
			if (item) onSelect(item.id);
			return;
		}
		if (key.upArrow) { setSelected((s) => Math.max(0, s - 1)); return; }
		if (key.downArrow) { setSelected((s) => Math.min(filtered.length - 1, s + 1)); return; }
		if (key.backspace) { setQuery((q) => q.slice(0, -1)); setSelected(0); return; }
		if (input && !key.ctrl && !key.meta && input >= ' ') {
			setQuery((q) => q + input);
			setSelected(0);
		}
	});

	const paletteWidth = Math.min(width - 4, 50);
	return (
		<Box
			flexDirection="column"
			width={paletteWidth}
			borderStyle="round"
			borderColor="cyan"
			paddingX={1}
		>
			<Text color="cyan" bold>{`> ${query}█`}</Text>
			{filtered.map((item, i) => (
				<Text
					key={item.id}
					color={i === selected ? 'white' : 'gray'}
					bold={i === selected}
				>
					{i === selected ? '▸ ' : '  '}{item.title}
				</Text>
			))}
			{filtered.length === 0 && <Text color="gray">no matches</Text>}
			<Text color="gray">{`${filtered.length}/${items.length} · ↑↓ nav · Enter select · Esc close`}</Text>
		</Box>
	);
};

// --- Main App ----------------------------------------------------------------

function App() {
	const { exit } = useApp();
	const tabs = useTabs({
		initial: [{ id: 'welcome', title: 'Welcome', closable: true }],
	});
	const [splitRatio, setSplitRatio] = useState(0.4);
	const [paletteOpen, setPaletteOpen] = useState(false);
	const [termCols, setTermCols] = useState(process.stdout.columns || 80);
	const [termRows, setTermRows] = useState(process.stdout.rows || 24);

	// Track terminal size
	React.useEffect(() => {
		const onResize = () => {
			setTermCols(process.stdout.columns || 80);
			setTermRows(process.stdout.rows || 24);
		};
		process.stdout.on('resize', onResize);
		return () => { process.stdout.off('resize', onResize); };
	}, []);

	const openNote = useCallback((id: string) => {
		const note = NOTES.find((n) => n.id === id);
		if (!note) return;
		const tab: Tab = { id: note.id, title: note.title, closable: true };
		tabs.open(tab);
		setPaletteOpen(false);
	}, [tabs]);

	useInput((input: string, key: InkKey) => {
		if (paletteOpen) return; // palette captures its own input
		if (input === 'q' && !key.ctrl) { exit(); return; }
		if (input === 'p' && key.ctrl) { setPaletteOpen(true); return; }
		if (key.tab && key.ctrl && key.shift) { tabs.cyclePrev(); return; }
		if (key.tab && key.ctrl) { tabs.cycleNext(); return; }
		if (input === 'w' && key.ctrl) { if (tabs.activeId) tabs.close(tabs.activeId); return; }
		if (key.leftArrow) { setSplitRatio((r) => Math.max(0.2, r - 0.05)); return; }
		if (key.rightArrow) { setSplitRatio((r) => Math.min(0.8, r + 0.05)); return; }
		// Ctrl+1..9 jump to tab by index
		if (key.ctrl && input >= '1' && input <= '9') {
			tabs.jumpTo(parseInt(input, 10) - 1);
		}
	});

	const activeNote = NOTES.find((n) => n.id === tabs.activeId);

	return (
		<Box flexDirection="column" width={termCols} height={termRows}>
			{/* Tab strip */}
			<Tabs
				tabs={tabs.tabs}
				activeId={tabs.activeId}
				onActivate={tabs.activate}
				onClose={tabs.close}
				showIndexes
			/>

			{/* Split: sidebar (note list) + content */}
			<Box flexGrow={1}>
				<Split
					direction="row"
					ratio={splitRatio}
					width={termCols}
					height={termRows - 3}
					activePane="b"
				>
					{/* Pane A: note list (sidebar) */}
					<Box flexDirection="column" paddingX={1}>
						<Text bold color="yellow">Notes</Text>
						{NOTES.map((note) => (
							<Text
								key={note.id}
								color={note.id === tabs.activeId ? 'white' : 'gray'}
								bold={note.id === tabs.activeId}
							>
								{note.id === tabs.activeId ? '▸ ' : '  '}{note.title}
							</Text>
						))}
						<Box marginTop={1}>
							<Text color="gray">Ctrl+p palette · q quit</Text>
						</Box>
					</Box>

					{/* Pane B: active note content */}
					<Box flexDirection="column" paddingX={1}>
						{activeNote ? (
							<>
								<Text bold color="green">{activeNote.title}</Text>
								<Text color="white">{activeNote.body}</Text>
							</>
						) : (
							<Text color="gray">No note selected. Press Ctrl+p to open one.</Text>
						)}
					</Box>
				</Split>
			</Box>

			{/* Footer */}
			<Box paddingX={1}>
				<Text color="gray">
					{`Ctrl+p palette · Ctrl+Tab/1-9 switch · Ctrl+w close · ←→ resize · q quit`}
				</Text>
			</Box>

			{/* Command palette overlay (absolute positioning) */}
			{paletteOpen && (
				<Box
					position="absolute"
					left={Math.max(0, Math.floor((termCols - 50) / 2))}
					top={Math.max(1, Math.floor(termRows / 4))}
				>
					<Palette
						items={NOTES}
						width={termCols}
						onSelect={openNote}
						onDismiss={() => setPaletteOpen(false)}
					/>
				</Box>
			)}
		</Box>
	);
}

render(<App />);
