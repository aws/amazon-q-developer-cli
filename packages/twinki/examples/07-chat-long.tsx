/**
 * 07-chat-long.tsx — AI Coding Agent Simulation
 *
 * Run: npx tsx examples/07-chat-long.tsx
 *
 * Simulates an LLM coding agent that:
 *   - Reads files, searches code, runs commands (tool calls)
 *   - Generates very large code blocks (100+ line classes)
 *   - Streams responses token-by-token
 *   - Handles multiple back-to-back exchanges
 *
 * This is a stress test for the rendering engine — the kind of
 * workload that causes Ink to flicker and corrupt scrollback.
 *
 * Controls:
 *   Press Enter to send the next pre-scripted prompt
 *   Ctrl+C to quit
 */
import React, { useState, useEffect, useCallback, useRef } from 'react';
import { render, Text, Box, Static, Markdown, Typewriter, useInput, useApp } from 'twinki';

interface Message {
	role: 'user' | 'assistant';
	content: string;
}

// --- Simulated tool call outputs ---

const TOOL_READ_FILE = `\`\`\`typescript
// src/renderer/tui.ts (lines 1-85)
import type { Terminal } from '../terminal/terminal.js';
import type { Component, InputListener, OverlayOptions } from './component.js';
import { Container } from './component.js';

interface OverlayEntry {
  component: Component;
  options: OverlayOptions;
  preFocus: Component | null;
  hidden: boolean;
}

export class TUI extends Container {
  public terminal: Terminal;
  private previousLines: string[] = [];
  private previousWidth = 0;
  private focusedComponent: Component | null = null;
  private inputListeners = new Set<InputListener>();
  private renderRequested = false;
  private cursorRow = 0;
  private hardwareCursorRow = 0;
  private maxLinesRendered = 0;
  private previousViewportTop = 0;
  private fullRedrawCount = 0;
  private stopped = false;
  private overlayStack: OverlayEntry[] = [];

  constructor(terminal: Terminal) {
    super();
    this.terminal = terminal;
  }

  start(): void {
    this.terminal.start(
      (data) => this.handleInput(data),
      () => this.requestRender(),
    );
    this.terminal.hideCursor();
    this.requestRender();
  }

  stop(): void {
    if (this.stopped) return;
    this.stopped = true;
    // Move cursor to end of content
    const diff = this.maxLinesRendered - this.hardwareCursorRow;
    if (diff > 0) this.terminal.write(\`\\x1b[\${diff}B\`);
    this.terminal.write('\\r\\n');
    this.terminal.showCursor();
    this.terminal.stop();
  }

  requestRender(force?: boolean): void {
    if (force) {
      this.previousLines = [];
      this.previousWidth = 0;
      this.maxLinesRendered = 0;
      this.previousViewportTop = 0;
      this.cursorRow = 0;
      this.hardwareCursorRow = 0;
      this.fullRedrawCount = 0;
    }
    if (this.renderRequested) return;
    this.renderRequested = true;
    process.nextTick(() => {
      this.renderRequested = false;
      this.doRender();
    });
  }
  // ... (continues for 600+ more lines)
}
\`\`\``;

const TOOL_SEARCH = `Found 12 matches across 5 files:

\`src/reconciler/host-config.ts\`:
  Line 37: \`function appendChild(parent: TwinkiNode | RootContainer, child: TwinkiNode)\`
  Line 49: \`function removeChild(parent: TwinkiNode | RootContainer, child: TwinkiNode)\`
  Line 58: \`function insertBefore(parent: TwinkiNode | RootContainer, child: TwinkiNode, before: TwinkiNode)\`

\`src/reconciler/render.ts\`:
  Line 30: \`class ReactBridge implements Component\`
  Line 67: \`export function render(element: React.ReactElement, options: TwinkiRenderOptions = {}): Instance\`

\`src/renderer/tui.ts\`:
  Line 27: \`export class TUI extends Container\`
  Line 468: \`private doRender(): void\`

\`src/renderer/component.ts\`:
  Line 1: \`export interface Component\`
  Line 61: \`export class Container implements Component\`

\`src/layout/yoga.ts\`:
  Line 7: \`export function createYogaNode(): YogaNode\`
  Line 11: \`export function applyYogaProps(node: YogaNode, props: Record<string, any>): void\`
  Line 130: \`export function getComputedLayout(node: YogaNode)\``;

const TOOL_RUN_TESTS = `$ npx vitest run test/chat-app.test.ts

 ✓ test/chat-app.test.ts (11 tests) 1928ms
   ✓ renders initial empty state with status bar
   ✓ user message appears in history
   ✓ typing indicator animates without flicker
   ✓ full conversation: user → typing → AI response
   ✓ multiple conversation turns preserve history (324ms)
   ✓ zero flicker across entire conversation (428ms)
   ✓ status bar never overlaps with message content
   ✓ differential updates during typing only change spinner line
   ✓ multi-line AI response renders correctly
   ✓ long conversation with scrollback beyond viewport
   ✓ rapid message burst (simulating streaming)

 Test Files  1 passed (1)
      Tests  11 passed (11)`;

// --- Generated large class ---

function generateLargeClass(): string {
	return `\`\`\`typescript
// src/components/DataGrid.tsx — Full implementation

import React, { useState, useEffect, useCallback, useMemo, useRef } from 'react';
import { Text, Box, useInput } from 'twinki';

export interface Column<T> {
  key: keyof T & string;
  header: string;
  width?: number;
  align?: 'left' | 'center' | 'right';
  render?: (value: T[keyof T], row: T, index: number) => React.ReactNode;
  sortable?: boolean;
  filterable?: boolean;
}

export interface DataGridProps<T> {
  data: T[];
  columns: Column<T>[];
  pageSize?: number;
  selectable?: boolean;
  onSelect?: (row: T, index: number) => void;
  onSort?: (column: string, direction: 'asc' | 'desc') => void;
  emptyMessage?: string;
  borderStyle?: 'single' | 'double' | 'round' | 'bold';
  highlightColor?: string;
  headerColor?: string;
  stripeColor?: string;
}

interface SortState {
  column: string;
  direction: 'asc' | 'desc';
}

interface FilterState {
  column: string;
  value: string;
}

export function DataGrid<T extends Record<string, any>>({
  data,
  columns,
  pageSize = 20,
  selectable = true,
  onSelect,
  onSort,
  emptyMessage = 'No data',
  borderStyle = 'single',
  highlightColor = 'cyan',
  headerColor = 'white',
  stripeColor = 'gray',
}: DataGridProps<T>): React.ReactElement {
  const [cursor, setCursor] = useState(0);
  const [page, setPage] = useState(0);
  const [sort, setSort] = useState<SortState | null>(null);
  const [filter, setFilter] = useState<FilterState | null>(null);
  const [filterInput, setFilterInput] = useState('');
  const [filterMode, setFilterMode] = useState(false);
  const scrollRef = useRef(0);

  // Compute column widths
  const colWidths = useMemo(() => {
    return columns.map(col => {
      if (col.width) return col.width;
      let max = col.header.length;
      for (const row of data) {
        const val = String(row[col.key] ?? '');
        max = Math.max(max, val.length);
      }
      return Math.min(max + 2, 40);
    });
  }, [columns, data]);

  // Sort data
  const sortedData = useMemo(() => {
    if (!sort) return data;
    const col = columns.find(c => c.key === sort.column);
    if (!col) return data;
    return [...data].sort((a, b) => {
      const va = a[col.key];
      const vb = b[col.key];
      const cmp = va < vb ? -1 : va > vb ? 1 : 0;
      return sort.direction === 'asc' ? cmp : -cmp;
    });
  }, [data, sort, columns]);

  // Filter data
  const filteredData = useMemo(() => {
    if (!filter || !filter.value) return sortedData;
    return sortedData.filter(row => {
      const val = String(row[filter.column] ?? '').toLowerCase();
      return val.includes(filter.value.toLowerCase());
    });
  }, [sortedData, filter]);

  // Paginate
  const totalPages = Math.ceil(filteredData.length / pageSize);
  const pageData = filteredData.slice(page * pageSize, (page + 1) * pageSize);

  // Clamp cursor
  useEffect(() => {
    if (cursor >= pageData.length) setCursor(Math.max(0, pageData.length - 1));
  }, [pageData.length, cursor]);

  // Input handling
  useInput((ch, key) => {
    if (filterMode) {
      if (key.return) {
        setFilter(filter ? { ...filter, value: filterInput } : null);
        setFilterMode(false);
        return;
      }
      if (key.escape) { setFilterMode(false); setFilterInput(''); return; }
      if (key.backspace) { setFilterInput(v => v.slice(0, -1)); return; }
      if (ch && ch.length === 1) { setFilterInput(v => v + ch); return; }
      return;
    }

    if (key.upArrow) setCursor(c => Math.max(0, c - 1));
    if (key.downArrow) setCursor(c => Math.min(pageData.length - 1, c + 1));
    if (key.leftArrow) setPage(p => Math.max(0, p - 1));
    if (key.rightArrow) setPage(p => Math.min(totalPages - 1, p + 1));

    if (key.return && selectable && pageData[cursor]) {
      onSelect?.(pageData[cursor]!, page * pageSize + cursor);
    }

    if (ch === 's' && columns.some(c => c.sortable)) {
      const sortableCols = columns.filter(c => c.sortable);
      const currentIdx = sort ? sortableCols.findIndex(c => c.key === sort.column) : -1;
      const nextCol = sortableCols[(currentIdx + 1) % sortableCols.length]!;
      const dir = sort?.column === nextCol.key && sort.direction === 'asc' ? 'desc' : 'asc';
      setSort({ column: nextCol.key, direction: dir });
      onSort?.(nextCol.key, dir);
    }

    if (ch === '/' && columns.some(c => c.filterable)) {
      setFilterMode(true);
      setFilterInput('');
      const filterableCol = columns.find(c => c.filterable);
      if (filterableCol) setFilter({ column: filterableCol.key, value: '' });
    }
  });

  // Render helpers
  const pad = (text: string, width: number, align: string = 'left') => {
    const stripped = text.slice(0, width);
    const space = Math.max(0, width - stripped.length);
    if (align === 'right') return ' '.repeat(space) + stripped;
    if (align === 'center') {
      const left = Math.floor(space / 2);
      return ' '.repeat(left) + stripped + ' '.repeat(space - left);
    }
    return stripped + ' '.repeat(space);
  };

  const separator = colWidths.map(w => '─'.repeat(w)).join('┼');

  if (filteredData.length === 0) {
    return React.createElement(Box, { flexDirection: 'column' },
      React.createElement(Text, { dimColor: true }, emptyMessage),
    );
  }

  return React.createElement(Box, { flexDirection: 'column' },
    // Header
    React.createElement(Text, { bold: true, color: headerColor },
      columns.map((col, i) => pad(
        col.header + (sort?.column === col.key ? (sort.direction === 'asc' ? ' ▲' : ' ▼') : ''),
        colWidths[i]!,
        col.align,
      )).join('│'),
    ),
    React.createElement(Text, { dimColor: true }, separator),

    // Rows
    ...pageData.map((row, ri) => {
      const isSelected = ri === cursor;
      const isStriped = ri % 2 === 1;
      return React.createElement(Text, {
        key: ri,
        color: isSelected ? highlightColor : isStriped ? stripeColor : undefined,
        bold: isSelected,
      },
        isSelected ? '▸ ' : '  ',
        columns.map((col, ci) => {
          const val = col.render
            ? String(col.render(row[col.key], row, page * pageSize + ri))
            : String(row[col.key] ?? '');
          return pad(val, colWidths[ci]!, col.align);
        }).join('│'),
      );
    }),

    // Footer
    React.createElement(Text, { dimColor: true }, separator),
    React.createElement(Text, { dimColor: true },
      \` Page \${page + 1}/\${totalPages} · \${filteredData.length} rows\` +
      (sort ? \` · sorted by \${sort.column} \${sort.direction}\` : '') +
      (filter?.value ? \` · filter: \${filter.value}\` : ''),
    ),

    // Filter input
    filterMode ? React.createElement(Text, null,
      React.createElement(Text, { color: 'yellow' }, 'Filter: '),
      filterInput,
      React.createElement(Text, { color: 'gray' }, '│'),
    ) : null,
  );
}
\`\`\``;
}

// --- Scripted conversation ---

const PROMPTS = [
	'Can you look at the TUI rendering engine and explain how doRender works?',
	'Search for all the places where children are added to the Yoga tree',
	'Run the chat app tests and show me the results',
	'Now create a full DataGrid component with sorting, filtering, pagination, and keyboard navigation',
];

function buildResponse(index: number): string {
	switch (index) {
		case 0:
			return `I'll read the TUI source to understand the rendering engine.

**Reading file:** \`src/renderer/tui.ts\`

${TOOL_READ_FILE}

The \`TUI\` class is the core rendering engine. Here's how \`doRender()\` works:

It implements **four rendering strategies** selected automatically:

1. **First render** — writes all lines with \\r\\n separators, no cursor movement needed
2. **Width changed** — full clear (\\x1b[3J\\x1b[2J\\x1b[H) + rewrite everything, because terminal resize invalidates all layout
3. **Shrink clear** — when content shrinks and \`clearOnShrink\` is enabled, full clear to remove ghost lines
4. **Differential** (the common case) — scans for \`firstChanged\`/\`lastChanged\` via string equality, only rewrites that range

Every strategy builds a single buffer string and calls \`terminal.write(buffer)\` exactly once, wrapped in synchronized output markers (\`CSI ?2026h/l\`). This is the key invariant that prevents flicker — the terminal sees the entire frame atomically.

The \`process.nextTick\` debouncing in \`requestRender()\` means multiple state changes within the same tick coalesce into a single render. So even rapid streaming updates produce minimal terminal writes.`;

		case 1:
			return `I'll search for all Yoga tree manipulation points.

**Searching:** \`appendChild\\|insertChild\\|removeChild\` in \`src/\`

${TOOL_SEARCH}

There are **12 call sites** across 5 files where the Yoga tree is modified:

**Reconciler (\`host-config.ts\`)** — the main entry points:
- \`appendChild\` — adds a child to a parent node's Yoga tree
- \`removeChild\` — removes and frees the Yoga node (calls \`freeNodeTree\`)
- \`insertBefore\` — inserts at a specific position, calculates correct Yoga index

**Key detail:** \`#text\` nodes and children of \`twinki-text\` nodes are **not** added to the Yoga tree. Text nodes have \`yogaNode: null\` because the parent \`twinki-text\` uses a measure function instead — Yoga doesn't allow children on measured nodes.

**ReactBridge (\`render.ts\`)** — creates the root Yoga node with \`flexDirection: column\`.

**TUI (\`tui.ts\`)** — doesn't touch Yoga directly, it works with the \`Component\` interface.

**Layout (\`yoga.ts\`)** — provides \`createYogaNode\`, \`applyYogaProps\`, and \`getComputedLayout\` utilities.`;

		case 2:
			return `Running the tests now.

**Running:** \`npx vitest run test/chat-app.test.ts\`

${TOOL_RUN_TESTS}

All **11 tests pass**. Key results:

- **Zero flicker** across the entire conversation (428ms test)
- **Differential updates** during typing — only the spinner line changes
- **Scrollback preserved** even with 15+ messages in a 10-row viewport
- **Status bar isolation** — never corrupted by message content
- **Streaming simulation** — word-by-word append with zero flicker

The test suite validates the full rendering pipeline: React → Reconciler → Yoga layout → TUI differential rendering → xterm headless verification. Each frame is captured and analyzed for flicker patterns (non-blank → blank → non-blank across consecutive frames).`;

		case 3:
			return `I'll create a full DataGrid component with all the features you asked for.

Here's the implementation — it's a substantial component (~180 lines) with:
- **Sorting** — press \`s\` to cycle through sortable columns
- **Filtering** — press \`/\` to enter filter mode, type to filter, Enter to apply
- **Pagination** — left/right arrows to change pages
- **Keyboard navigation** — up/down to move cursor, Enter to select
- **Striped rows** — alternating colors for readability
- **Column alignment** — left, center, or right per column

${generateLargeClass()}

**Usage example:**

\`\`\`tsx
import { DataGrid } from './components/DataGrid';

const data = [
  { id: 1, name: 'Alice', role: 'Engineer', status: 'Active' },
  { id: 2, name: 'Bob', role: 'Designer', status: 'Away' },
  // ... hundreds of rows
];

const columns = [
  { key: 'id', header: 'ID', width: 6, sortable: true },
  { key: 'name', header: 'Name', width: 20, sortable: true, filterable: true },
  { key: 'role', header: 'Role', width: 15 },
  { key: 'status', header: 'Status', width: 10, render: (v) => v === 'Active' ? '🟢' : '🔴' },
];

<DataGrid data={data} columns={columns} pageSize={15} />
\`\`\`

This component renders efficiently because Twinki's differential rendering means only the rows that actually change get rewritten. Scrolling through a 1000-row dataset with a 20-row page size produces minimal terminal writes — just the visible rows plus the footer.`;

		default:
			return 'All done!';
	}
}

// --- App ---

const SPINNERS = ['⠋', '⠙', '⠹', '⠸', '⠼', '⠴', '⠦', '⠧', '⠇', '⠏'];

const App = () => {
	const SPEED_STEPS = [
		{ label: 'typing (3 WPS)', value: 'typing' as const },
		{ label: 'natural (20 WPS)', value: 'natural' as const },
		{ label: 'fast (50 WPS)', value: 'fast' as const },
		{ label: 'instant', value: 'instant' as const },
	];
	const [speedIdx, setSpeedIdx] = useState(1);

	const [messages, setMessages] = useState<Message[]>([]);
	const [input, setInput] = useState('');
	const [activeResponse, setActiveResponse] = useState<string | null>(null);
	const [spinFrame, setSpinFrame] = useState(0);
	const [status, setStatus] = useState('Type a message and press Enter');
	const [thinking, setThinking] = useState(false);
	const responseIdx = useRef(0);
	const { exit } = useApp();

	const busy = activeResponse !== null || thinking;

	useEffect(() => {
		if (!thinking) return;
		const timer = setInterval(() => setSpinFrame(f => f + 1), 80);
		return () => clearInterval(timer);
	}, [thinking]);

	const simulateResponse = useCallback(() => {
		setThinking(true);
		setStatus('Agent working...');
		const idx = responseIdx.current++ % PROMPTS.length;
		setTimeout(() => {
			setThinking(false);
			setActiveResponse(buildResponse(idx));
			setStatus('Streaming...');
		}, 300);
	}, []);

	const handleTypewriterComplete = useCallback(() => {
		if (activeResponse === null) return;
		setMessages(m => [...m, { role: 'assistant', content: activeResponse }]);
		setActiveResponse(null);
		setStatus('Type a message and press Enter');
	}, [activeResponse]);

	useInput((ch, key) => {
		// Speed control — always works
		if (key.leftArrow) { setSpeedIdx(i => Math.max(0, i - 1)); return; }
		if (key.rightArrow) { setSpeedIdx(i => Math.min(SPEED_STEPS.length - 1, i + 1)); return; }

		// Block text input while streaming
		if (busy) return;

		if (key.return && input.trim()) {
			const userMsg = input.trim();
			setMessages(m => [...m, { role: 'user', content: userMsg }]);
			setInput('');
			simulateResponse();
			return;
		}

		if (key.backspace) {
			setInput(i => i.slice(0, -1));
			return;
		}

		if (ch && ch.length === 1 && ch.charCodeAt(0) >= 32) {
			setInput(i => i + ch);
		}
	});

	const spinner = SPINNERS[spinFrame % SPINNERS.length];
	const msgCount = messages.length;

	return (
		<Box flexDirection="column">
			<Static items={messages.map((msg, i) => ({ id: i, msg }))}>
				{({ id, msg }) => (
					<Box key={id} flexDirection="column">
						{id === 0 && (
							<>
								<Box borderStyle="round" borderColor="magenta">
									<Text bold color="magenta"> Agent Chat — Long Duration Stress Test </Text>
									{process.env['TWINKI_NO_SYNC'] && <Text bold color="red"> [NO SYNC — FLICKER MODE] </Text>}
								</Box>
								<Text> </Text>
							</>
						)}
						{msg.role === 'user' ? (
							<Box flexDirection="column">
								<Text color="cyan" bold>  You:</Text>
								<Text>  {msg.content}</Text>
							</Box>
						) : (
							<Box flexDirection="column">
								<Text color="green" bold>  Agent:</Text>
								<Box paddingLeft={2}>
									<Markdown>{msg.content}</Markdown>
								</Box>
							</Box>
						)}
						<Text> </Text>
					</Box>
				)}
			</Static>

			{messages.length === 0 && (
				<>
					<Box borderStyle="round" borderColor="magenta">
						<Text bold color="magenta"> Agent Chat — Long Duration Stress Test </Text>
						{process.env['TWINKI_NO_SYNC'] && <Text bold color="red"> [NO SYNC — FLICKER MODE] </Text>}
					</Box>
					<Text> </Text>
				</>
			)}

			{activeResponse !== null && (
				<Box flexDirection="column">
					<Text color="green" bold>  Agent:</Text>
					<Box paddingLeft={2}>
						<Typewriter speed={SPEED_STEPS[speedIdx]!.value} onComplete={handleTypewriterComplete}>
							{activeResponse}
						</Typewriter>
					</Box>
					<Text> </Text>
				</Box>
			)}

			{thinking && (
				<Text>  <Text color="yellow">{spinner}</Text> Agent is working...</Text>
			)}

			{!busy && (
				<Box flexDirection="column">
					<Text>{'─'.repeat(60)}</Text>
					<Text>  <Text color="cyan" bold>{'>'}</Text> {input}<Text color="gray">│</Text></Text>
				</Box>
			)}

			<Text> </Text>
			<Text dimColor>  {status}  •  {msgCount} messages  •  Speed: {SPEED_STEPS[speedIdx]!.label} (←/→)  •  Sync: {process.env['TWINKI_NO_SYNC'] ? 'OFF' : 'ON'}  •  Ctrl+C to quit</Text>
		</Box>
	);
};

render(<App />);
