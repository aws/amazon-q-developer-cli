/**
 * 22-resize.tsx — Resize stress test with static + live content
 *
 * Run: bun examples/22-resize.tsx
 *
 * Tests resize behavior with:
 *   - Static content (scrollback) with clear markers
 *   - Live content (viewport) with clear markers
 *   - Dividers to show width boundaries
 *   - Terminal dimensions display
 *
 * Resize the terminal window and verify:
 *   1. No duplicate content in viewport
 *   2. Dividers re-render at new width
 *   3. Static markers stay in scrollback
 *   4. Live markers stay in viewport
 */
import React, { useState, useEffect } from 'react';
import { render, Text, Box, Static, useInput } from '../packages/twinki/src/index.ts';

function App() {
	const [cols, setCols] = useState(process.stdout.columns);
	const [rows, setRows] = useState(process.stdout.rows);
	const [messages, setMessages] = useState<string[]>([]);
	const [msgCount, setMsgCount] = useState(0);

	useEffect(() => {
		const handler = () => {
			setCols(process.stdout.columns);
			setRows(process.stdout.rows);
		};
		process.stdout.on('resize', handler);
		return () => { process.stdout.off('resize', handler); };
	}, []);

	useInput((input, key) => {
		if (key.return) {
			const n = msgCount + 1;
			setMsgCount(n);
			setMessages(m => [...m, `[STATIC #${n}] This message should scroll into scrollback and re-render at new width on resize`]);
		}
	});

	const divider = '─'.repeat(Math.min(cols, 120));
	const marker = '█'.repeat(Math.min(cols, 120));

	return (
		<Box flexDirection="column" width="100%">
			{/* Static content — scrolls into scrollback */}
			{messages.length > 0 && (
				<Static items={messages}>
					{(msg: string, i: number) => (
						<Box key={i} flexDirection="column">
							<Text color="yellow">{'▓ STATIC ZONE ▓'.padEnd(cols, '▓')}</Text>
							<Text>  {msg}</Text>
							<Text color="yellow">{'▓'.repeat(Math.min(cols, 120))}</Text>
						</Box>
					)}
				</Static>
			)}

			{/* Live content — always in viewport */}
			<Text color="cyan">{'░ LIVE ZONE START ░'.padEnd(cols, '░')}</Text>
			<Text>{divider}</Text>
			<Text bold> Terminal: {cols}x{rows}  |  Messages: {msgCount}</Text>
			<Text>{divider}</Text>
			<Text> Line A: The quick brown fox jumps over the lazy dog</Text>
			<Text> Line B: Pack my box with five dozen liquor jugs</Text>
			<Text> Line C: How vexingly quick daft zebras jump</Text>
			<Text>{divider}</Text>
			<Text color="green"> {marker.slice(0, 20)} WIDTH MARKER {marker.slice(0, 20)}</Text>
			<Text>{divider}</Text>
			<Text dimColor> Enter = add static message  |  Ctrl+C = quit  |  Resize terminal to test</Text>
			<Text color="cyan">{'░ LIVE ZONE END ░'.padEnd(cols, '░')}</Text>
		</Box>
	);
}

const instance = render(<App />, { exitOnCtrlC: true });
await instance.waitUntilExit();
