/**
 * Markdown component for rendering markdown to styled terminal output.
 *
 * Fast by default: renders to pre-styled ANSI strings with minimal
 * React nodes (one <Text> per block). Code blocks render in gray.
 *
 * With `highlight` prop: code blocks get full shiki syntax highlighting
 * (async WASM-based, 332 languages, 65 themes). Use for final/static
 * content where the extra cost is worth it.
 *
 * Supported elements:
 *   Block: headings, paragraphs, code blocks, lists, blockquotes, tables, hr
 *   Inline: **bold**, *italic*, ~~strikethrough~~, `code`, [links](url)
 * 
 * @param props - The component props
 * @param props.children - Markdown content to render
 * @param props.highlight - Enable shiki syntax highlighting for code blocks (default: false)
 * @param props.theme - Theme for syntax highlighting (default: 'monokai')
 * @returns A React element representing rendered markdown content
 * 
 * @example
 * ```tsx
 * <Markdown>{text}</Markdown>                    // fast, ANSI colors
 * <Markdown highlight>{text}</Markdown>           // shiki highlighting
 * <Markdown highlight theme="dracula">{text}</Markdown>
 * ```
 */
import React, { useState, useEffect, useMemo } from 'react';
import { marked, type Token, type Tokens } from 'marked';
import { Text } from './Text.js';
import { Box } from './Box.js';
import { sanitizeTerminalText } from '../utils/sanitize-terminal.js';

/**
 * Supported themes for syntax highlighting in code blocks.
 */
export type MarkdownTheme =
	| 'monokai' | 'dracula' | 'github-dark' | 'github-light'
	| 'catppuccin-mocha' | 'catppuccin-latte' | 'one-dark-pro'
	| 'nord' | 'vitesse-dark' | 'tokyo-night'
	| (string & {}); // any shiki theme name

/**
 * Props for the Markdown component.
 */
export interface MarkdownProps {
	/** Markdown content to render */
	children: string;
	/** Enable shiki syntax highlighting for code blocks. Default: false */
	highlight?: boolean;
	/** Theme for syntax highlighting */
	theme?: MarkdownTheme;
}

// --- ANSI escape helpers ---
const RESET = '\x1b[0m';
const BOLD = '\x1b[1m';
const DIM = '\x1b[2m';
const ITALIC = '\x1b[3m';
const UNDERLINE = '\x1b[4m';
const STRIKE = '\x1b[9m';
const CYAN = '\x1b[36m';
const GREEN = '\x1b[32m';
const YELLOW = '\x1b[33m';
const BLUE = '\x1b[34m';
const MAGENTA = '\x1b[35m';
const GRAY = '\x1b[90m';
const HEADING_COLORS = [CYAN, GREEN, YELLOW, BLUE, MAGENTA, ''] as const;

// --- Shiki (lazy, shared singleton) ---
export { getHighlighter } from '../utils/shiki.js';
import {
	canonicalShikiLanguage,
	getHighlighter,
	getHighlighterSync,
	loadedLangs,
	loadedThemes,
} from '../utils/shiki.js';

// --- Main component ---

export const Markdown: React.FC<MarkdownProps> = ({ children, highlight = false, theme = 'monokai' }) => {
	const tokens = marked.lexer(sanitizeTerminalText(children));

	// Render each token: with `highlight`, code blocks with a language get
	// shiki; everything else (and the default) renders as fast ANSI strings.
	return React.createElement(Box, { flexDirection: 'column' },
		...tokens.map((token, i) => {
			if (highlight && token.type === 'code' && (token as Tokens.Code).lang) {
				return React.createElement(HighlightedCodeBlock, { key: i, token: token as Tokens.Code, theme });
			}
			const s = blockToString(token);
			return s !== null ? React.createElement(Text, { key: i }, s) : null;
		}),
	);
};

function codeLanguage(token: Tokens.Code): string | undefined {
	const language = token.lang?.trim().split(/\s+/, 1)[0];
	return language ? canonicalShikiLanguage(language) : undefined;
}

function markdownLanguages(markdown: string): string[] {
	const languages = new Set<string>();
	marked.walkTokens(marked.lexer(sanitizeTerminalText(markdown)), (token) => {
		if (token.type !== 'code') return;
		const language = codeLanguage(token as Tokens.Code);
		if (language) languages.add(language);
	});
	return [...languages];
}

/** Returns a revision when fenced-code languages are ready for synchronous rendering. */
export function useMarkdownHighlighting(
	markdown: string,
	theme = 'monokai',
): number {
	const [revision, setRevision] = useState(0);
	const languages = useMemo(
		() => markdownLanguages(markdown).join('\0'),
		[markdown],
	);

	useEffect(() => {
		if (!languages) return;
		let cancelled = false;
		void Promise.all(
			languages.split('\0').map((language) => getHighlighter(theme, language)),
		).then(() => {
			if (!cancelled) setRevision((value) => value + 1);
		});
		return () => {
			cancelled = true;
		};
	}, [languages, theme]);

	return revision;
}

// --- Block rendering to ANSI strings ---

/**
 * Renders markdown to a plain ANSI string (no React) — for hosts that manage
 * their own line windowing/scrolling (e.g. transcript viewports) and only
 * want the styled text. Code blocks use a synchronously available Shiki
 * theme/language when supplied, then fall back to the fast gray path.
 */
export function markdownToAnsi(
	markdown: string,
	options?: { theme?: string; baseColor?: string },
): string {
	const tokens = marked.lexer(sanitizeTerminalText(markdown));
	const rendered = tokens
		.map((token) => blockToString(token, options?.theme))
		.filter((s): s is string => s !== null)
		.join('\n');
	const match = options?.baseColor?.match(
		/^#([\da-f]{2})([\da-f]{2})([\da-f]{2})$/i,
	);
	if (!match) return rendered;

	const foreground =
		`\x1b[38;2;${Number.parseInt(match[1]!, 16)};` +
		`${Number.parseInt(match[2]!, 16)};${Number.parseInt(match[3]!, 16)}m`;
	return rendered.replaceAll(RESET, `${RESET}${foreground}`);
}

function blockToString(token: Token, theme?: string): string | null {
	switch (token.type) {
		case 'heading': {
			const t = token as Tokens.Heading;
			const color = HEADING_COLORS[Math.min(t.depth - 1, 5)] || '';
			return `${color}${BOLD}${inlineToString(t.tokens)}${RESET}`;
		}
		case 'paragraph':
			return inlineToString((token as Tokens.Paragraph).tokens);
		case 'code': {
			const t = token as Tokens.Code;
			const language = codeLanguage(t);
			if (theme && language) {
				try {
					const highlighter = getHighlighterSync();
					if (
						highlighter &&
						loadedThemes.has(theme) &&
						loadedLangs.has(language)
					) {
						const result = highlighter.codeToTokens(t.text, {
							lang: language,
							theme,
						});
						return result.tokens
							.map((line: Array<{ content: string; color?: string }>) =>
								'  ' +
								line
									.map((token: { content: string; color?: string }) => {
										if (!token.color) return token.content;
										const [r, g, b] = [
											token.color.slice(1, 3),
											token.color.slice(3, 5),
											token.color.slice(5, 7),
										].map((hex) => Number.parseInt(hex, 16));
										return `\x1b[38;2;${r};${g};${b}m${token.content}${RESET}`;
									})
									.join(''),
							)
							.join('\n');
					}
				} catch {
					// Shiki is not ready or the language is unknown; use gray below.
				}
			}
			const code = t.text.split('\n').map(l => `${GRAY}  ${l}${RESET}`).join('\n');
			return code;
		}
		case 'list':
			return listToString(token as Tokens.List);
		case 'blockquote': {
			const t = token as Tokens.Blockquote;
			return t.tokens.map(child => {
				if (child.type === 'paragraph') {
					return `${GRAY}  │ ${RESET}${ITALIC}${inlineToString((child as Tokens.Paragraph).tokens)}${RESET}`;
				}
				return blockToString(child);
			}).filter(Boolean).join('\n');
		}
		case 'table':
			return tableToString(token as Tokens.Table);
		case 'hr':
			return `${DIM}${'─'.repeat(40)}${RESET}`;
		case 'space':
			return ' ';
		default:
			return null;
	}
}

function listToString(token: Tokens.List): string {
	// marked types start as number | "" — "" would string-concat with i.
	const start = typeof token.start === 'number' ? token.start : 1;
	return token.items.map((item, i) => {
		const bullet = token.ordered ? `${start + i}. ` : '• ';
		const content = item.tokens.map(child => {
			if (child.type === 'text' && (child as Tokens.Text).tokens) {
				return inlineToString((child as Tokens.Text).tokens!);
			}
			if (child.type === 'list') {
				return '\n' + listToString(child as Tokens.List).split('\n').map(l => '  ' + l).join('\n');
			}
			return inlineToString([child]);
		}).join('');
		return `${DIM}  ${bullet}${RESET}${content}`;
	}).join('\n');
}

function tableToString(token: Tokens.Table): string {
	const colWidths = token.header.map((h, ci) => {
		let max = h.text.length;
		for (const row of token.rows) {
			if (row[ci]) max = Math.max(max, row[ci]!.text.length);
		}
		return max + 2;
	});
	const pad = (text: string, width: number) => text + ' '.repeat(Math.max(0, width - text.length));
	const sep = colWidths.map(w => '─'.repeat(w)).join('┼');
	const header = `${BOLD}  ${token.header.map((h, ci) => pad(h.text, colWidths[ci]!)).join('│')}${RESET}`;
	const separator = `${DIM}  ${sep}${RESET}`;
	const rows = token.rows.map(row =>
		`  ${row.map((cell, ci) => pad(cell.text, colWidths[ci]!)).join('│')}`,
	);
	return [header, separator, ...rows].join('\n');
}

function inlineToString(tokens: Token[]): string {
	return tokens.map(token => {
		switch (token.type) {
			case 'text':
				return (token as Tokens.Text).tokens
					? inlineToString((token as Tokens.Text).tokens!)
					: (token as Tokens.Text).text;
			case 'strong':
				return `${BOLD}${inlineToString((token as Tokens.Strong).tokens)}${RESET}`;
			case 'em':
				return `${ITALIC}${inlineToString((token as Tokens.Em).tokens)}${RESET}`;
			case 'codespan':
				return `${YELLOW}\`${(token as Tokens.Codespan).text}\`${RESET}`;
			case 'del':
				return `${STRIKE}${inlineToString((token as Tokens.Del).tokens)}${RESET}`;
			case 'link':
				return `${CYAN}${UNDERLINE}${(token as Tokens.Link).text}${RESET}`;
			case 'image':
				return `${DIM}[${(token as Tokens.Image).text}]${RESET}`;
			case 'br':
				return '\n';
			default:
				return 'raw' in token ? (token as any).raw : '';
		}
	}).join('');
}

// --- Shiki code block (only used when highlight=true) ---

const HighlightedCodeBlock: React.FC<{ token: Tokens.Code; theme: string }> = ({ token, theme }) => {
	const [highlighted, setHighlighted] = useState<string | null>(null);
	const lineCount = token.text.split('\n').length;
	// Only re-highlight every 3 lines to avoid per-word shiki calls
	const highlightKey = Math.floor(lineCount / 3);

	useEffect(() => {
		if (!token.lang) return;
		let cancelled = false;
		const language = codeLanguage(token);
		if (!language) return;
		getHighlighter(theme, language).then(highlighter => {
			if (cancelled) return;
			try {
				const result = highlighter.codeToTokens(token.text, { lang: language, theme });
				const lines = result.tokens.map((line: any[]) =>
					'  ' + line.map((t: any) => {
						const color = t.color ? `\x1b[38;2;${parseInt(t.color.slice(1, 3), 16)};${parseInt(t.color.slice(3, 5), 16)};${parseInt(t.color.slice(5, 7), 16)}m` : '';
						return `${color}${t.content}${RESET}`;
					}).join(''),
				);
				setHighlighted(lines.join('\n'));
			} catch {
				// Language not loaded — keep gray fallback
			}
		});
		return () => { cancelled = true; };
	}, [highlightKey, token.lang, theme]);

	// Show highlighted lines + any unhighlighted trailing lines in gray
	const allLines = token.text.split('\n');
	if (highlighted) {
		const highlightedLines = highlighted.split('\n');
		const extra = allLines.slice(highlightedLines.length);
		if (extra.length > 0) {
			return React.createElement(Text, null,
				highlighted + '\n' + extra.map(l => `${GRAY}  ${l}${RESET}`).join('\n'),
			);
		}
		return React.createElement(Text, null, highlighted);
	}
	return React.createElement(Text, null, allLines.map(l => `${GRAY}  ${l}${RESET}`).join('\n'));
};
