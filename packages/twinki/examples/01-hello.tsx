/**
 * 01-hello.tsx — Minimal Twinki app
 *
 * Run: npx tsx examples/01-hello.tsx
 */
import React from 'react';
import { render, Text, Box } from 'twinki';

const App = () => (
	<Box flexDirection="column">
		<Text bold color="green">Hello, Twinki! 🎉</Text>
		<Text>A high-performance React renderer for terminal UIs.</Text>
		<Text dimColor>Press Ctrl+C to exit</Text>
	</Box>
);

render(<App />);
