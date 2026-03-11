/**
 * 16-mouse.tsx — Mouse event demo
 *
 * Run: npx tsx examples/16-mouse.tsx
 *
 * Four sections:
 *   1. Live debug panel — raw mouse event + hit-test node tracking
 *   2. Clickable color boxes with hover highlight
 *   3. Sidebar menu — click items to show content on the right
 *   4. Yes/No confirmation dialog
 *
 * Escape to quit.
 */
import React, { useState, useCallback } from 'react';
import { render, Text, Box, useInput, useMouse, useTwinkiContext } from 'twinki';
import type { MouseEvent } from 'twinki';

const c = {
	red: '#ff6188', green: '#a9dc76', blue: '#78dce8',
	yellow: '#ffd866', purple: '#ab9df2', orange: '#fc9867',
	dim: '#727072', fg: '#fcfcfa', bg: '#2d2a2e', bgLight: '#403e41',
};

// --- Debug state shared across components ---

type DebugState = {
	raw: MouseEvent | null;
	hoveredNode: string;
	clickedNode: string;
	handler: string;
};

// --- Reusable hover button with debug reporting ---

const HoverBox = ({ children, color, active, nodeId, onClick: click, onDebug }: {
	children: React.ReactNode; color: string; active?: boolean; nodeId: string;
	onClick: () => void; onDebug: (hoveredNode: string, handler: string) => void;
}) => {
	const [hover, setHover] = useState(false);
	return (
		<Box
			borderStyle="round"
			borderColor={active ? color : hover ? c.fg : c.dim}
			paddingX={1}
			onClick={() => { click(); onDebug(nodeId, `onClick → ${nodeId}`); }}
			onMouseEnter={() => { setHover(true); onDebug(nodeId, `onMouseEnter → ${nodeId}`); }}
			onMouseLeave={() => { setHover(false); onDebug('', `onMouseLeave → ${nodeId}`); }}
		>
			{children}
		</Box>
	);
};

// --- Debug panel ---

const DebugPanel = ({ debug }: { debug: DebugState }) => {
	const e = debug.raw;
	return (
		<Box borderStyle="round" borderColor={c.dim} flexDirection="column" paddingX={1}>
			<Text color={c.yellow} bold>Hit-Test Debug</Text>
			<Box flexDirection="row">
				<Box flexDirection="column" width={32}>
					<Text color={c.dim}>{'Raw Event:'}</Text>
					<Text color={c.fg}>  pos     <Text color={c.blue}>{e ? `(${e.x}, ${e.y})` : '—'}</Text></Text>
					<Text color={c.fg}>  button  <Text color={c.blue}>{e?.button ?? '—'}</Text></Text>
					<Text color={c.fg}>  type    <Text color={c.blue}>{e?.type ?? '—'}</Text></Text>
					<Text color={c.fg}>  mods    <Text color={c.blue}>{e ? [e.shift && 'shift', e.ctrl && 'ctrl', e.alt && 'alt'].filter(Boolean).join('+') || 'none' : '—'}</Text></Text>
				</Box>
				<Box flexDirection="column">
					<Text color={c.dim}>{'Hit-Test Resolution:'}</Text>
					<Text color={c.fg}>  hovered <Text color={c.purple}>{debug.hoveredNode || '(none)'}</Text></Text>
					<Text color={c.fg}>  clicked <Text color={c.orange}>{debug.clickedNode || '(none)'}</Text></Text>
					<Text color={c.fg}>  handler <Text color={c.green}>{debug.handler || '—'}</Text></Text>
				</Box>
			</Box>
		</Box>
	);
};

// --- Section 1: Color boxes ---

const ColorBoxes = ({ onDebug }: { onDebug: (h: string, handler: string) => void }) => {
	const [last, setLast] = useState('');
	const colors = [
		{ label: 'Red', color: c.red },
		{ label: 'Green', color: c.green },
		{ label: 'Blue', color: c.blue },
		{ label: 'Purple', color: c.purple },
	];
	return (
		<Box flexDirection="column">
			<Text color={c.yellow} bold>Color Boxes</Text>
			<Box flexDirection="row">
				{colors.map(({ label, color }) => (
					<HoverBox key={label} nodeId={`ColorBox:${label}`} color={color} onDebug={onDebug} onClick={() => setLast(label)}>
						<Text color={color}>{label}</Text>
					</HoverBox>
				))}
			</Box>
			{last ? <Text color={c.dim}>Last clicked: <Text color={c.green}>{last}</Text></Text> : null}
		</Box>
	);
};

// --- Section 2: Sidebar menu ---

const menuItems = [
	{ label: 'Overview', body: 'Twinki is a high-performance React renderer for terminal UIs with flicker-free differential rendering.' },
	{ label: 'Mouse',    body: 'SGR mouse protocol with hit testing against the Yoga layout tree. onClick, onMouseEnter, onMouseLeave on Box and Text.' },
	{ label: 'Rendering', body: 'Four render strategies: first render, width change, shrink clear, and line-based differential updates.' },
	{ label: 'Testing',  body: 'Frame-accurate E2E testing with @xterm/headless. Flicker detection, collision analysis, byte efficiency checks.' },
];

const SidebarMenu = ({ onDebug }: { onDebug: (h: string, handler: string) => void }) => {
	const [selected, setSelected] = useState(0);
	return (
		<Box flexDirection="column">
			<Text color={c.yellow} bold>Sidebar Menu</Text>
			<Box flexDirection="row">
				<Box flexDirection="column" width={14}>
					{menuItems.map((item, i) => (
						<HoverBox key={item.label} nodeId={`Menu:${item.label}`} color={c.blue} active={i === selected} onDebug={onDebug} onClick={() => setSelected(i)}>
							<Text color={i === selected ? c.blue : c.dim}>{item.label}</Text>
						</HoverBox>
					))}
				</Box>
				<Box borderStyle="round" borderColor={c.dim} paddingX={1} flexGrow={1}>
					<Text color={c.fg} wrap="wrap">{menuItems[selected].body}</Text>
				</Box>
			</Box>
		</Box>
	);
};

// --- Section 3: Yes/No dialog ---

const ConfirmDialog = ({ onDebug }: { onDebug: (h: string, handler: string) => void }) => {
	const [answer, setAnswer] = useState<'yes' | 'no' | null>(null);
	return (
		<Box flexDirection="column">
			<Text color={c.yellow} bold>Confirm Dialog</Text>
			<Box borderStyle="round" borderColor={c.purple} paddingX={1} flexDirection="column">
				<Text color={c.fg}>Deploy to production?</Text>
				<Box flexDirection="row" marginTop={1}>
					<HoverBox nodeId="Dialog:Yes" color={c.green} active={answer === 'yes'} onDebug={onDebug} onClick={() => setAnswer('yes')}>
						<Text color={c.green}> Yes </Text>
					</HoverBox>
					<HoverBox nodeId="Dialog:No" color={c.red} active={answer === 'no'} onDebug={onDebug} onClick={() => setAnswer('no')}>
						<Text color={c.red}> No </Text>
					</HoverBox>
				</Box>
				{answer && (
					<Text color={answer === 'yes' ? c.green : c.red} bold>
						{answer === 'yes' ? 'Deploying...' : 'Cancelled.'}
					</Text>
				)}
			</Box>
		</Box>
	);
};

// --- App ---

const App = () => {
	const { tui } = useTwinkiContext();
	const [mouseOn, setMouseOn] = useState(true);
	const [debug, setDebug] = useState<DebugState>({ raw: null, hoveredNode: '', clickedNode: '', handler: '' });

	useInput((input) => {
		if (input === 'm') {
			if (mouseOn) tui.disableMouse();
			else tui.enableMouse();
			setMouseOn(!mouseOn);
		}
	});

	useMouse((e: MouseEvent) => {
		setDebug(prev => ({ ...prev, raw: e, clickedNode: e.type === 'mouseup' ? prev.clickedNode : prev.clickedNode }));
	});

	const onDebug = useCallback((hoveredNode: string, handler: string) => {
		setDebug(prev => ({
			...prev,
			hoveredNode: hoveredNode || prev.hoveredNode,
			clickedNode: handler.startsWith('onClick') ? hoveredNode : prev.clickedNode,
			handler,
		}));
	}, []);

	return (
		<Box flexDirection="column">
			<Text color={mouseOn ? c.green : c.red} bold>Mouse: {mouseOn ? 'ON' : 'OFF'} <Text color={c.dim}>(press m to toggle, Escape to quit)</Text></Text>
			<DebugPanel debug={debug} />
			<Text> </Text>
			<ColorBoxes onDebug={onDebug} />
			<Text> </Text>
			<SidebarMenu onDebug={onDebug} />
			<Text> </Text>
			<ConfirmDialog onDebug={onDebug} />
			<Text> </Text>
			<Text color={c.dim}>Escape to quit.</Text>
		</Box>
	);
};

render(<App />, { exitOnCtrlC: true });
