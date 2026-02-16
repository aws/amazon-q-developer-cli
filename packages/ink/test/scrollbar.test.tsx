import React from 'react';
import {describe, it, expect} from 'bun:test';
import {EventEmitter} from 'node:events';
import {render, Box, Text, Scrollbar} from '../src/index.js';

function createFakeStdout(columns = 80) {
	let lastWrite = '';
	const stdout = new EventEmitter() as any;
	stdout.columns = columns;
	stdout.write = (data: string) => { lastWrite = data; return true; };
	stdout.get = () => lastWrite;
	return stdout;
}

function renderToString(node: React.JSX.Element, columns = 80): string {
	const stdout = createFakeStdout(columns);
	render(node, {stdout, debug: true});
	return stdout.get();
}

describe('Scrollbar', () => {
	it('returns null when content fits viewport', () => {
		const output = renderToString(
			<Scrollbar scrollTop={0} totalLines={5} viewportHeight={10} />,
			20,
		);
		expect(output).not.toContain('█');
		expect(output).not.toContain('░');
	});

	it('renders track and thumb chars when content overflows', () => {
		const output = renderToString(
			<Scrollbar scrollTop={0} totalLines={20} viewportHeight={5} />,
			5,
		);
		expect(output).toContain('█');
		expect(output).toContain('░');
	});

	it('thumb at top when scrollTop=0', () => {
		const output = renderToString(
			<Scrollbar scrollTop={0} totalLines={20} viewportHeight={5} />,
			5,
		);
		const lines = output.split('\n').filter(l => l.trim());
		expect(lines[0]).toContain('▲');
		expect(lines[1]).toContain('█');
	});

	it('thumb at bottom when scrolled to end', () => {
		const output = renderToString(
			<Scrollbar scrollTop={15} totalLines={20} viewportHeight={5} />,
			5,
		);
		const lines = output.split('\n').filter(l => l.trim());
		expect(lines[lines.length - 1]).toContain('▼');
		expect(lines[lines.length - 2]).toContain('█');
	});

	it('thumb position reflects scroll position', () => {
		const output = renderToString(
			<Scrollbar scrollTop={7} totalLines={20} viewportHeight={5} />,
			5,
		);
		const lines = output.split('\n').filter(l => l.trim());
		expect(lines[0]).toContain('▲');
		expect(lines[lines.length - 1]).toContain('▼');
		expect(output).toContain('█');
	});

	it('thumb size is proportional to viewport/content ratio', () => {
		const smallOutput = renderToString(
			<Scrollbar scrollTop={0} totalLines={100} viewportHeight={5} />,
			5,
		);
		const largeOutput = renderToString(
			<Scrollbar scrollTop={0} totalLines={10} viewportHeight={5} />,
			5,
		);
		const smallThumbs = (smallOutput.match(/█/g) || []).length;
		const largeThumbs = (largeOutput.match(/█/g) || []).length;
		expect(largeThumbs).toBeGreaterThan(smallThumbs);
	});

	it('scrollbar visible at rightmost column in row layout with width=100% sibling', () => {
		const content = Array.from({length: 10}, (_, i) => `Line ${i+1}`).join('\n');
		const output = renderToString(
			<Box flexDirection="column" width="100%">
				<Box flexDirection="row">
					<Box flexGrow={1} flexDirection="column">
						<Box flexDirection="row" width="100%">
							<Box width={1} flexDirection="column">
								{Array.from({length: 10}, (_, i) => <Text key={i}> </Text>)}
							</Box>
							<Box flexGrow={1} marginLeft={1}><Text>{content}</Text></Box>
						</Box>
					</Box>
					<Scrollbar scrollTop={5} totalLines={50} viewportHeight={10} />
				</Box>
			</Box>,
			80,
		);
		const lines = output.split('\n');
		// Scrollbar should be at the rightmost column
		expect(lines.some(l => l.trimEnd().endsWith('█'))).toBe(true);
		expect(lines.some(l => l.trimEnd().endsWith('░'))).toBe(true);
	});
});