import React, { useEffect, useState } from 'react';
import { Box, Text, getHighlighter } from 'twinki';
import { palette } from '../lib/palette.js';

const RESET = '\x1b[0m';

/** True-color ANSI foreground from a #rrggbb hex, or '' if not parseable. */
function ansiFromHex(hex?: string): string {
	if (!hex || hex[0] !== '#' || hex.length < 7) return '';
	const r = parseInt(hex.slice(1, 3), 16);
	const g = parseInt(hex.slice(3, 5), 16);
	const b = parseInt(hex.slice(5, 7), 16);
	return `\x1b[38;2;${r};${g};${b}m`;
}

/** Highlight text into one ANSI string per line via shiki. */
async function highlight(text: string, lang: string, theme: string): Promise<string[]> {
	const hl = await getHighlighter(theme, lang);
	const { tokens } = hl.codeToTokens(text, { lang, theme });
	return (tokens as { color?: string; content: string }[][]).map((line) =>
		line.map((t) => `${ansiFromHex(t.color)}${t.content}${RESET}`).join(''),
	);
}

export interface FileViewProps {
	value: string;
	/** shiki language id, or undefined for plain text. */
	language: string | undefined;
	theme: string;
	/** Number of content rows to fill (the viewer pads to exactly this height). */
	height: number;
}

/**
 * Read-only, syntax-highlighted file viewer that fills the full pane height.
 * Unlike the chat-style EditorInput (which caps its viewport at ~30% of the
 * terminal), this renders from the top and pads with a continuous gutter so a
 * short file still occupies the whole editor area — like a real editor.
 */
export const FileView: React.FC<FileViewProps> = ({ value, language, theme, height }) => {
	const [lines, setLines] = useState<string[]>(() => value.split('\n'));

	useEffect(() => {
		if (!language) {
			setLines(value.split('\n'));
			return;
		}
		let cancelled = false;
		highlight(value, language, theme)
			.then((hl) => { if (!cancelled) setLines(hl); })
			.catch(() => { if (!cancelled) setLines(value.split('\n')); });
		return () => { cancelled = true; };
	}, [value, language, theme]);

	const dim = ansiFromHex(palette.dim);
	const gutterW = Math.max(2, String(lines.length).length);
	const bar = `${dim}${' '.repeat(gutterW)} │${RESET}`;
	const hasMore = lines.length > height;
	const codeRows = hasMore ? Math.max(0, height - 1) : Math.min(lines.length, height);

	const rows: React.ReactNode[] = [];
	for (let i = 0; i < height; i++) {
		if (i < codeRows) {
			const num = String(i + 1).padStart(gutterW);
			rows.push(
				<Text key={i} wrap="truncate">{`${dim}${num} │ ${RESET}${lines[i] ?? ''}`}</Text>,
			);
		} else if (hasMore && i === height - 1) {
			rows.push(
				<Text key={i} wrap="truncate">{`${dim}${' '.repeat(gutterW)} │ ⋯ ${lines.length - codeRows} more${RESET}`}</Text>,
			);
		} else {
			rows.push(<Text key={i} wrap="truncate">{bar}</Text>);
		}
	}

	return <Box flexDirection="column">{rows}</Box>;
};
