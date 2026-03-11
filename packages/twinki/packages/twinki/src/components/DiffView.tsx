/**
 * DiffView — VS Code-inspired side-by-side or inline code diff viewer.
 *
 * Features:
 *   - Myers line-level diff with word-level intra-line highlights
 *   - Optional shiki syntax highlighting (same engine as VS Code)
 *   - Line numbers (auto-tracked or from startLine metadata)
 *   - layout="auto" switches horizontal↔vertical based on terminal width
 *
 * @example
 * ```tsx
 * <DiffView values={[oldCode, newCode]} highlight lang="typescript" />
 * <DiffView values={[{ content: old, startLine: 1 }, { content: newCode, startLine: 1 }]} layout="horizontal" />
 * ```
 */
import React, { useState, useEffect } from 'react';
import * as Diff from 'diff';
import { Text } from './Text.js';
import { Box } from './Box.js';
import type { MarkdownTheme } from './Markdown.js';
import { useTwinkiContext } from '../hooks/context.js';

// ── Types ────────────────────────────────────────────────────────────────────

export interface DiffSide {
	content: string;
	/** First line number to display. Defaults to 1. */
	startLine?: number;
}

export interface DiffViewProps {
	/** [oldContent, newContent] — strings or objects with optional startLine */
	values: [string | DiffSide, string | DiffSide];
	/** 'auto' picks horizontal when terminal is wide enough (≥120 cols) */
	layout?: 'auto' | 'horizontal' | 'vertical';
	/** Enable shiki syntax highlighting */
	highlight?: boolean;
	/** Language hint for syntax highlighting */
	lang?: string;
	/** Shiki theme */
	theme?: MarkdownTheme;
}

// ── ANSI helpers ─────────────────────────────────────────────────────────────

const RESET      = '\x1b[0m';
const DIM        = '\x1b[2m';
const FG_LINENUM = '\x1b[38;2;100;100;100m';
const FG_SEP     = '\x1b[38;2;80;80;80m';

function fg(hex: string): string {
	const h = hex.replace('#', '').slice(0, 6);
	return `\x1b[38;2;${parseInt(h.slice(0,2),16)};${parseInt(h.slice(2,4),16)};${parseInt(h.slice(4,6),16)}m`;
}
function bg(hex: string): string {
	const h = hex.replace('#', '').slice(0, 6);
	return `\x1b[48;2;${parseInt(h.slice(0,2),16)};${parseInt(h.slice(2,4),16)};${parseInt(h.slice(4,6),16)}m`;
}
/** Composite an rgba hex (8 chars) over an opaque bg hex, return opaque hex. */
function composite(rgba: string, over: string): string {
	const s = rgba.replace('#', '');
	const o = over.replace('#', '').slice(0, 6);
	const a = parseInt(s.slice(6, 8), 16) / 255;
	const blend = (f: number, b: number) => Math.round(f * a + b * (1 - a));
	const r = blend(parseInt(s.slice(0,2),16), parseInt(o.slice(0,2),16));
	const g = blend(parseInt(s.slice(2,4),16), parseInt(o.slice(2,4),16));
	const b2= blend(parseInt(s.slice(4,6),16), parseInt(o.slice(4,6),16));
	return `#${r.toString(16).padStart(2,'0')}${g.toString(16).padStart(2,'0')}${b2.toString(16).padStart(2,'0')}`;
}

interface DiffColors {
	bgRem: string; bgAdd: string;   // line background
	hlRem: string; hlAdd: string;   // word highlight background
	fgRem: string; fgAdd: string;   // gutter +/- color
}

function colorsFromTheme(t: any): DiffColors {
	const c = t.colors ?? {};
	const dark = t.type === 'dark';
	const themeBg = t.bg ?? (dark ? '#1e1e1e' : '#ffffff');

	const resolve = (key: string, fallback: string) => {
		const v: string | undefined = c[key];
		if (!v) return fallback;
		return v.replace('#','').length === 8 ? composite(v, themeBg) : v;
	};

	// Line bg: use removedLineBackground first, fall back to removedTextBackground
	const remLine = resolve('diffEditor.removedLineBackground',
		resolve('diffEditor.removedTextBackground', dark ? '#3d1212' : '#ffeef0'));
	const addLine = resolve('diffEditor.insertedLineBackground',
		resolve('diffEditor.insertedTextBackground', dark ? '#0d2e0d' : '#e6ffec'));
	// Word bg: text key is typically slightly brighter than line key
	const remWord = resolve('diffEditor.removedTextBackground',
		resolve('diffEditor.removedLineBackground', dark ? '#6b1c1c' : '#ffc0c0'));
	const addWord = resolve('diffEditor.insertedTextBackground',
		resolve('diffEditor.insertedLineBackground', dark ? '#1e4d1e' : '#a0f0a0'));

	return {
		bgRem: bg(remLine), bgAdd: bg(addLine),
		hlRem: bg(remWord), hlAdd: bg(addWord),
		fgRem: dark ? fg('#f97583') : fg('#d73a49'),
		fgAdd: dark ? fg('#85e89d') : fg('#28a745'),
	};
}

const DEFAULT_COLORS: DiffColors = {
	bgRem: bg('#3d1212'), bgAdd: bg('#0d2e0d'),
	hlRem: bg('#6b1c1c'), hlAdd: bg('#1e4d1e'),
	fgRem: fg('#f97583'), fgAdd: fg('#85e89d'),
};

import { getHighlighter, loadedThemes, loadedLangs, cachedHighlighter } from '../utils/shiki.js';

/** Convert shiki token line to ANSI string with optional bg overlay */
function tokensToAnsi(tokens: any[], bgOverride?: string): string {
	return tokens.map((t: any) => {
		const fgc = t.color ? fg(t.color) : '';
		const bgc = bgOverride ?? '';
		return `${bgc}${fgc}${t.content}${RESET}`;
	}).join('');
}

/** Word-level diff with highlight colors */
function applyWordDiff(oldLine: string, newLine: string, colors: DiffColors): [string, string] {
	const changes = Diff.diffWords(oldLine, newLine);
	let oldOut = '', newOut = '';
	for (const c of changes) {
		if (c.removed)     oldOut += `${colors.hlRem}${c.value}${RESET}`;
		else if (c.added)  newOut += `${colors.hlAdd}${c.value}${RESET}`;
		else { oldOut += c.value; newOut += c.value; }
	}
	return [oldOut, newOut];
}

// ── Diff computation ──────────────────────────────────────────────────────────

type LineKind = 'unchanged' | 'removed' | 'added' | 'empty';

interface DiffLine {
	kind: LineKind;
	content: string;       // plain text (for word diff / display)
	ansi?: string;         // syntax-highlighted ANSI (set after highlight pass)
	lineNum?: number;
}

interface DiffPair {
	left: DiffLine;
	right: DiffLine;
}

function computePairs(oldText: string, newText: string, colors: DiffColors): DiffPair[] {
	const changes = Diff.diffLines(oldText, newText, { newlineIsToken: false });
	const pairs: DiffPair[] = [];
	let i = 0;
	while (i < changes.length) {
		const c = changes[i]!;
		if (!c.removed && !c.added) {
			for (const line of c.value.replace(/\n$/, '').split('\n'))
				pairs.push({ left: { kind: 'unchanged', content: line }, right: { kind: 'unchanged', content: line } });
			i++;
		} else {
			const removedLines = c.removed ? c.value.replace(/\n$/, '').split('\n') : [];
			const nextC = changes[i + 1];
			const addedLines = (c.removed && nextC?.added)
				? nextC.value.replace(/\n$/, '').split('\n')
				: (!c.removed && c.added) ? c.value.replace(/\n$/, '').split('\n') : [];
			const maxLen = Math.max(removedLines.length, addedLines.length);
			for (let j = 0; j < maxLen; j++) {
				const oldLine = removedLines[j];
				const newLine = addedLines[j];
				if (oldLine !== undefined && newLine !== undefined) {
					const [oldAnsi, newAnsi] = applyWordDiff(oldLine, newLine, colors);
					pairs.push({ left: { kind: 'removed', content: oldLine, ansi: oldAnsi }, right: { kind: 'added', content: newLine, ansi: newAnsi } });
				} else if (oldLine !== undefined) {
					pairs.push({ left: { kind: 'removed', content: oldLine }, right: { kind: 'empty', content: '' } });
				} else {
					pairs.push({ left: { kind: 'empty', content: '' }, right: { kind: 'added', content: newLine! } });
				}
			}
			i += (c.removed && nextC?.added) ? 2 : 1;
		}
	}
	return pairs;
}

/** Assign line numbers to pairs */
function assignLineNumbers(pairs: DiffPair[], leftStart: number, rightStart: number): void {
	let l = leftStart;
	let r = rightStart;
	for (const p of pairs) {
		if (p.left.kind !== 'empty') p.left.lineNum = l++;
		if (p.right.kind !== 'empty') p.right.lineNum = r++;
	}
}

// ── Syntax highlight overlay ──────────────────────────────────────────────────

/** Apply shiki highlighting to all lines in pairs — synchronous, highlighter must already be loaded */
function applyHighlight(pairs: DiffPair[], lang: string, theme: string, colors: DiffColors, highlighter: any): void {
	const hl = (text: string, bgc?: string): string => {
		try {
			const result = highlighter.codeToTokens(text, { lang, theme });
			return tokensToAnsi(result.tokens[0] ?? [], bgc);
		} catch { return text; }
	};

	for (const p of pairs) {
		if (p.left.kind === 'unchanged') {
			p.left.ansi = hl(p.left.content);
			p.right.ansi = p.left.ansi;
		} else if (p.left.kind === 'removed') {
			p.left.ansi = p.left.ansi
				? `${colors.bgRem}${p.left.ansi}${RESET}`
				: hl(p.left.content, colors.bgRem);
		} else if (p.right.kind === 'added') {
			p.right.ansi = p.right.ansi
				? `${colors.bgAdd}${p.right.ansi}${RESET}`
				: hl(p.right.content, colors.bgAdd);
		}
	}
}

// ── Rendering helpers ─────────────────────────────────────────────────────────

const GUTTER: Record<LineKind, string> = { unchanged: ' ', removed: '-', added: '+', empty: ' ' };

function renderLine(line: DiffLine, lineNumWidth: number, colors: DiffColors): string {
	const num = line.lineNum !== undefined
		? `${FG_LINENUM}${String(line.lineNum).padStart(lineNumWidth)}${RESET} `
		: ' '.repeat(lineNumWidth + 1);
	const gutterFg = line.kind === 'removed' ? colors.fgRem : line.kind === 'added' ? colors.fgAdd : DIM;
	const gutter = `${gutterFg}${GUTTER[line.kind]}${RESET} `;
	const lineBg = line.kind === 'removed' ? colors.bgRem : line.kind === 'added' ? colors.bgAdd : '';
	const content = line.ansi ?? `${lineBg}${line.content}${RESET}`;
	return `${num}${gutter}${content}`;
}

// ── Main component ────────────────────────────────────────────────────────────

export const DiffView: React.FC<DiffViewProps> = ({
	values,
	layout = 'auto',
	highlight = false,
	lang = 'typescript',
	theme = 'github-dark',
}) => {
	const [left, right] = values;
	const leftSide:  DiffSide = typeof left  === 'string' ? { content: left }  : left;
	const rightSide: DiffSide = typeof right === 'string' ? { content: right } : right;

	const { tui } = useTwinkiContext();
	const [state, setState] = useState<{ pairs: DiffPair[]; colors: DiffColors }>({ pairs: [], colors: DEFAULT_COLORS });
	const [cols, setCols] = useState(process.stdout.columns ?? 80);

	// After each state update commits to DOM, force a full clear redraw
	// so stale ANSI from previous theme never lingers on screen
	useEffect(() => {
		if (state.pairs.length > 0) tui.requestRender(true);
	}, [state]);

	useEffect(() => {
		const onResize = () => setCols(process.stdout.columns ?? 80);
		process.stdout.on('resize', onResize);
		return () => { process.stdout.off('resize', onResize); };
	}, []);

	useEffect(() => {
		let cancelled = false;
		async function run() {
			const isCached = !highlight || (cachedHighlighter && loadedThemes.has(theme) && loadedLangs.has(lang));

			let h = isCached
				? (highlight ? cachedHighlighter : null)
				: await getHighlighter(theme, lang);

			if (cancelled) return;

			const colors = h ? colorsFromTheme(h.getTheme(theme)) : DEFAULT_COLORS;
			const computed = computePairs(leftSide.content, rightSide.content, colors);
			assignLineNumbers(computed, leftSide.startLine ?? 1, rightSide.startLine ?? 1);

			if (h) {
				applyHighlight(computed, lang, theme, colors, h);
			} else {
				for (const p of computed) {
					if (!p.left.ansi && p.left.kind === 'removed')
						p.left.ansi = `${colors.bgRem}${p.left.content}${RESET}`;
					if (!p.right.ansi && p.right.kind === 'added')
						p.right.ansi = `${colors.bgAdd}${p.right.content}${RESET}`;
				}
			}
			setState({ pairs: computed, colors });
		}
		run();
		return () => { cancelled = true; };
	}, [leftSide.content, rightSide.content, highlight, lang, theme]);

	const { pairs, colors } = state;

	return React.useMemo(() => {
		const resolvedLayout = layout === 'auto'
			? (cols >= 120 ? 'horizontal' : 'vertical')
			: layout;

		const maxLineNum = Math.max(
			...pairs.map(p => p.left.lineNum ?? 0),
			...pairs.map(p => p.right.lineNum ?? 0),
		);
		const lineNumWidth = String(maxLineNum).length;

		if (pairs.length === 0) return null;

		if (resolvedLayout === 'horizontal') {
			const leftLines  = pairs.map(p => renderLine(p.left,  lineNumWidth, colors));
			const rightLines = pairs.map(p => renderLine(p.right, lineNumWidth, colors));
			return (
				<Box flexDirection="row">
					<Box flexDirection="column" flexGrow={1}>
						<Text wrap="truncate">{`${FG_LINENUM}${DIM}── old ${'─'.repeat(20)}${RESET}`}</Text>
						{leftLines.map((l, i) => <Text key={i} wrap="truncate">{l}</Text>)}
					</Box>
					<Box flexDirection="column">
						<Text wrap="truncate">{`${FG_SEP}│${RESET}`}</Text>
						{pairs.map((_, i) => <Text key={i} wrap="truncate">{`${FG_SEP}│${RESET}`}</Text>)}
					</Box>
					<Box flexDirection="column" flexGrow={1}>
						<Text wrap="truncate">{`${FG_LINENUM}${DIM}── new ${'─'.repeat(20)}${RESET}`}</Text>
						{rightLines.map((l, i) => <Text key={i} wrap="truncate">{l}</Text>)}
					</Box>
				</Box>
			);
		}

		return (
			<Box flexDirection="column">
				{pairs.map((p, i) => {
					if (p.left.kind === 'unchanged') {
						return <Text key={i} wrap="truncate">{renderLine(p.left, lineNumWidth, colors)}</Text>;
					}
					return (
						<React.Fragment key={i}>
							{p.left.kind !== 'empty' && (
								<Text wrap="truncate">{renderLine(p.left, lineNumWidth, colors)}</Text>
							)}
							{p.right.kind !== 'empty' && (
								<Text wrap="truncate">{renderLine(p.right, lineNumWidth, colors)}</Text>
							)}
						</React.Fragment>
					);
				})}
			</Box>
		);
	// state reference changes only when pairs+colors are fully computed
	}, [state, cols, layout]); // eslint-disable-line react-hooks/exhaustive-deps
};
