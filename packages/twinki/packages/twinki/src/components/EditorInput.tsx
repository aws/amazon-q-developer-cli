import React, { useEffect, useRef, useState } from 'react';
import { useTwinkiContext } from '../hooks/context.js';
import { Editor, type AutocompleteProvider } from './Editor.js';
import { getHighlighter } from '../utils/shiki.js';
import type { ComponentMouseEvent } from '../input/mouse.js';

const RESET = '\x1b[0m';

async function highlightText(text: string, lang: string, theme: string): Promise<Map<number, string>> {
	const highlighter = await getHighlighter(theme, lang);
	const result = highlighter.codeToTokens(text, { lang, theme });
	const map = new Map<number, string>();
	(result.tokens as any[][]).forEach((lineTokens: any[], i: number) => {
		map.set(i, lineTokens.map((t: any) => {
			const color = t.color ? `\x1b[38;2;${parseInt(t.color.slice(1, 3), 16)};${parseInt(t.color.slice(3, 5), 16)};${parseInt(t.color.slice(5, 7), 16)}m` : '';
			return `${color}${t.content}${RESET}`;
		}).join(''));
	});
	return map;
}

/** Scroll state the editor reports each render — drives an external scrollbar. */
export interface EditorScrollInfo {
	scrollTop: number;
	totalLines: number;
	viewportHeight: number;
}

export interface EditorInputProps {
	/** Current value (controlled) */
	value?: string;
	/** Called when user submits (Enter) */
	onSubmit?: (value: string) => void;
	/** Called when value changes */
	onChange?: (value: string) => void;
	/** Disable submit on Enter */
	disableSubmit?: boolean;
	/** Autocomplete provider */
	autocompleteProvider?: AutocompleteProvider;
	/** Max visible autocomplete items */
	autocompleteMaxVisible?: number;
	/** Horizontal padding */
	paddingX?: number;
	/** Whether input is active (default: true) */
	isActive?: boolean;
	/** Explicit text color instead of relying on the terminal default. */
	color?: string;
	/** Explicit line background color. */
	backgroundColor?: string;
	/** Dimmed guidance rendered when the editor is empty. */
	placeholder?: string;
	/** Language for syntax highlighting (e.g. 'tsx', 'python'). Requires shiki. */
	syntaxHighlight?: string;
	/** Viewport height in lines — fills a fixed-height pane instead of the 30%-of-terminal default. */
	visibleLines?: number;
	/** Render/wrap width in columns. Defaults to the full terminal width;
	 *  pass the pane's inner width so content wraps to the PANE, not the screen. */
	width?: number;
	/** Reports scroll state after each render — drives an external scrollbar. */
	onScrollInfo?: (info: EditorScrollInfo) => void;
	/** Imperative scroll target (e.g. from a clicked scrollbar); jumps the viewport. */
	scrollTo?: number;
	/** Theme for syntax highlighting (default: 'monokai') */
	syntaxTheme?: string;
	/** Show line numbers */
	lineNumbers?: boolean;
	/** Place the editor cursor from left mouse clicks. */
	mouseCursor?: boolean;
}

/**
 * React wrapper for the Editor component.
 * Multi-line text editor with word-wrap, scrolling, undo/redo, kill ring.
 */
export const EditorInput: React.FC<EditorInputProps> = ({
	value,
	onSubmit,
	onChange,
	disableSubmit,
	autocompleteProvider,
	autocompleteMaxVisible,
	paddingX,
	isActive = true,
	color,
	backgroundColor,
	placeholder,
	syntaxHighlight,
	syntaxTheme = 'monokai',
	lineNumbers = false,
	mouseCursor = false,
	visibleLines,
	width: widthProp,
	onScrollInfo,
	scrollTo,
}) => {
	const { tui } = useTwinkiContext();
	const editorRef = useRef<Editor>(null!);
	const created = !editorRef.current;

	if (created) {
		editorRef.current = new Editor({
			paddingX,
			autocompleteMaxVisible,
			terminalRows: tui.terminal.rows,
		});
	}

	const editor = editorRef.current;
	if (created) {
		editor.disableSubmit = disableSubmit ?? false;
		editor.lineNumbers = lineNumbers;
		editor.placeholder = placeholder ?? '';
		editor.focused = isActive;
		editor.setVisibleLines(visibleLines ?? null);
		if (autocompleteProvider) editor.setAutocompleteProvider(autocompleteProvider);
		if (value !== undefined) editor.setText(value);
	}
	const [renderedLines, setRenderedLines] = useState<string[]>(
		() => editor.render(widthProp ?? tui.terminal.columns),
	);
	const widthRef = useRef(widthProp);
	widthRef.current = widthProp;
	const visibleLinesRef = useRef(visibleLines);
	visibleLinesRef.current = visibleLines;
	const onScrollInfoRef = useRef(onScrollInfo);
	onScrollInfoRef.current = onScrollInfo;

	useEffect(() => {
		editor.onSubmit = onSubmit;
		editor.onChange = (val: string) => {
			onChange?.(val);
			rerender();
		};
		editor.disableSubmit = disableSubmit ?? false;
		if (autocompleteProvider) editor.setAutocompleteProvider(autocompleteProvider);
	});

	useEffect(() => {
		editor.lineNumbers = lineNumbers;
		editor.placeholder = placeholder ?? '';
		editor.setVisibleLines(visibleLines ?? null);
		rerender();
	}, [lineNumbers, placeholder, visibleLines, widthProp]);

	useEffect(() => {
		if (value !== undefined && value !== editor.getExpandedText()) {
			editor.setText(value);
			scheduleHighlight();
			rerender();
		}
	}, [value]);

	// Syntax highlighting: re-highlight whenever text changes
	const highlightLangRef = useRef(syntaxHighlight);
	highlightLangRef.current = syntaxHighlight;
	const highlightThemeRef = useRef(syntaxTheme);
	highlightThemeRef.current = syntaxTheme;

	function scheduleHighlight() {
		const lang = highlightLangRef.current;
		const theme = highlightThemeRef.current ?? 'monokai';
		if (!lang) return;
		const text = editor.getText();
		highlightText(text, lang, theme).then((map) => {
			editor.setHighlightedLines(map);
			rerender();
		}).catch(() => {});
	}

	useEffect(() => {
		if (!isActive) {
			editor.focused = false;
			rerender();
			return;
		}

		editor.focused = true;
		editor.setTerminalRows(tui.terminal.rows);
		rerender();

		const unsub = tui.addInputListener((data) => {
			editor.handleInput(data);
			scheduleHighlight();
			rerender();
		});

		return () => {
			unsub();
			editor.focused = false;
		};
	}, [tui, isActive]);

	// Initial highlight when syntaxHighlight prop is set
	useEffect(() => {
		if (syntaxHighlight) scheduleHighlight();
	}, [syntaxHighlight, syntaxTheme]);

	// External scrollbar click → jump the viewport.
	useEffect(() => {
		if (scrollTo === undefined) return;
		editor.setScrollOffset(scrollTo);
		rerender();
	}, [scrollTo]);

	function rerender() {
		const width = widthRef.current ?? tui.terminal.columns;
		setRenderedLines(editor.render(width));
		onScrollInfoRef.current?.({
			scrollTop: editor.getScrollOffset(),
			totalLines: editor.getTotalLines(),
			viewportHeight: visibleLinesRef.current ?? Math.max(5, Math.floor(tui.terminal.rows * 0.3)),
		});
	}

	return React.createElement(
		'twinki-box',
		{
			flexDirection: 'column',
			onMouseDown: mouseCursor
				? (event: ComponentMouseEvent) => {
					if (event.button !== 'left') return;
					editor.setCursorFromViewport(event.localY, event.localX);
					rerender();
				}
				: undefined,
		},
		...renderedLines.map((line, i) =>
			React.createElement(
				'twinki-text',
				{ key: i, wrap: 'truncate', color, backgroundColor },
				line,
			),
		),
	);
};

EditorInput.displayName = 'EditorInput';
