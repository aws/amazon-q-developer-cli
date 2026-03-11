/**
 * 21-overlay-tips.tsx — Floating overlay tips demo
 *
 * Run: npx tsx examples/21-overlay-tips.tsx
 *
 * Shows floating tip bubbles that appear and auto-hide.
 * Press Enter to cycle tips, 1-4 to show at different positions.
 */
import React, { useEffect, useRef } from 'react';
import { render, Text, Box, useApp, useInput, useOverlay, EditorInput } from 'twinki';
import type { OverlayHandle, OverlayAnchor } from 'twinki';

const EDITOR_CONTENT = `import React from "react";
import { render, Text, Box } from "twinki";

function App() {
  const [count, setCount] = React.useState(0);

  return (
    <Box flexDirection="column">
      <Text>Count: {count}</Text>
      <Text dimColor>Press + to increment</Text>
    </Box>
  );
}

render(<App />);`;

const TIPS = [
	{ text: 'Press Ctrl+S to save your work', color: 'cyan' },
	{ text: 'Try "kiro-cli chat" for AI assistance', color: 'green' },
	{ text: 'Use ← → to navigate between files', color: 'yellow' },
	{ text: 'Tip: You can click on any symbol to jump to definition', color: 'magenta' },
	{ text: 'Run tests with "npx vitest run"', color: 'cyan' },
	{ text: 'Ghost mode: your AI pair programmer is always watching', color: 'green' },
];

function Tip({ text, color }: { text: string; color: string }) {
	return (
		<Box borderStyle="round" borderColor={color} paddingX={1}>
			<Text color={color}>💡 {text}</Text>
		</Box>
	);
}

function App() {
	const { exit } = useApp();
	const tipIdx = useRef(0);
	const handle = useRef<OverlayHandle | null>(null);
	const hideTimer = useRef<ReturnType<typeof setTimeout> | null>(null);
	const [anchor, setAnchor] = React.useState<OverlayAnchor>('top-center');

	const showTip = useOverlay(
		() => {
			const tip = TIPS[tipIdx.current % TIPS.length]!;
			return <Tip text={tip.text} color={tip.color} />;
		},
		{ anchor, offsetY: 1 },
	);

	function displayTip() {
		handle.current?.hide();
		if (hideTimer.current) clearTimeout(hideTimer.current);
		handle.current = showTip();
		hideTimer.current = setTimeout(() => {
			handle.current?.hide();
			handle.current = null;
		}, 3000);
	}

	// Auto-show first tip on start
	useEffect(() => { displayTip(); }, []);

	useInput((ch, key) => {
		if (ch === 'q') { exit(); return; }
		if (key.return) {
			tipIdx.current++;
			displayTip();
		}
		if (ch === '1') { setAnchor('top-left'); tipIdx.current++; displayTip(); }
		if (ch === '2') { setAnchor('top-right'); tipIdx.current++; displayTip(); }
		if (ch === '3') { setAnchor('bottom-left'); tipIdx.current++; displayTip(); }
		if (ch === '4') { setAnchor('bottom-right'); tipIdx.current++; displayTip(); }
		if (ch === '5') { setAnchor('center'); tipIdx.current++; displayTip(); }
	});

	return (
		<Box flexDirection="column">
			<Box borderStyle="single" borderColor="gray" flexDirection="column" paddingX={1}>
				<Text bold color="white"> app.tsx</Text>
				<EditorInput
					value={EDITOR_CONTENT}
					isActive={false}
					syntaxHighlight="tsx"
					syntaxTheme="monokai"
					disableSubmit
				/>
			</Box>
			<Box paddingX={1} marginTop={1}>
				<Text dimColor>Enter=next tip  1-5=position (corners/center)  q=quit  Tips auto-hide in 3s</Text>
			</Box>
		</Box>
	);
}

render(<App />);
