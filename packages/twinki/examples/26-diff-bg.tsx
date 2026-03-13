/**
 * Diff background color test — matches Write.tsx layout 1:1.
 * Run from packages/twinki: bun run examples/26-diff-bg.tsx
 */
import React from 'react';
import { render, Box, Text, ProcessTerminal } from 'twinki';

const terminal = new ProcessTerminal();

type Change = { value: string; added?: boolean; removed?: boolean };

const changes: Change[] = [
	{ value: 'const old1 = true;\nconst old2 = false;\n', removed: true },
	{ value: 'const new1 = true;\nconst new2 = false;\nconst new3 = null;\n', added: true },
	{ value: 'const unchanged = 1;\nconst also = 2;\n' },
	{ value: 'const removed = 3;\n', removed: true },
];

const getColor = (path: string) => {
	if (path === 'diff.removed.background') return { hex: '#4a2030' };
	if (path === 'diff.added.background') return { hex: '#1e4a28' };
	// Return chalk-like functions that wrap text in ANSI codes (simulating real getColor)
	if (path === 'primary') return (s: string) => `\x1b[37m${s}\x1b[0m`;
	if (path === 'secondary') return (s: string) => `\x1b[90m${s}\x1b[0m`;
	return (s: string) => s;
};

const DiffTest = () => {
	let oldLineNum = 1;
	let newLineNum = 1;

	// Exact same structure as Write.tsx standalone diff view
	return (
		<Box flexDirection="column" width="90%">
			{changes.map((change, index) => {
				const lines = change.value.split('\n');
				if (lines[lines.length - 1] === '') lines.pop();

				return lines.map((line, lineIdx) => {
					if (change.removed) {
						const currentOldLine = oldLineNum++;
						const lineNumber = String(currentOldLine).padStart(4);
						const lineContent = `-  ${line}`;
						return (
							<Box
								key={`${index}-${lineIdx}`}
								backgroundColor={getColor('diff.removed.background').hex}
							>
								<Text>{getColor('primary')(lineNumber)}</Text>
								<Text>{lineContent}</Text>
							</Box>
						);
					} else if (change.added) {
						const currentNewLine = newLineNum++;
						const lineNumber = String(currentNewLine).padStart(4);
						const lineContent = `+  ${line}`;
						return (
							<Box
								key={`${index}-${lineIdx}`}
								backgroundColor={getColor('diff.added.background').hex}
							>
								<Text>{getColor('primary')(lineNumber)}</Text>
								<Text>{lineContent}</Text>
							</Box>
						);
					} else {
						const currentLine = oldLineNum++;
						newLineNum++;
						const lineNumber = String(currentLine).padStart(4);
						const lineContent = `   ${line}`;
						return (
							<Text key={`${index}-${lineIdx}`}>
								<Text>{getColor('secondary')(lineNumber)}</Text>
								<Text>{lineContent}</Text>
							</Text>
						);
					}
				});
			})}
		</Box>
	);
};

const instance = render(<DiffTest />, { terminal });
setTimeout(() => { instance.unmount(); terminal.stop(); process.exit(0); }, 5000);
