import React, { useEffect, useRef, useState } from 'react';
import { useTwinkiContext } from '../hooks/context.js';
import { Editor, type AutocompleteProvider } from './Editor.js';
import { getHighlighter } from '../utils/shiki.js';

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
	/** Language for syntax highlighting (e.g. 'tsx', 'python'). Requires shiki. */
	syntaxHighlight?: string;
	/** Theme for syntax highlighting (default: 'monokai') */
	syntaxTheme?: string;
	/** Show line numbers */
	lineNumbers?: boolean;
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
	syntaxHighlight,
	syntaxTheme = 'monokai',
	lineNumbers = false,
}) => {
	const { tui } = useTwinkiContext();
	const editorRef = useRef<Editor>(null!);
	const [renderedLines, setRenderedLines] = useState<string[]>([]);

	if (!editorRef.current) {
		editorRef.current = new Editor({
			paddingX,
			autocompleteMaxVisible,
			terminalRows: tui.terminal.rows,
		});
	}

	const editor = editorRef.current;

	useEffect(() => {
		editor.onSubmit = onSubmit;
		editor.onChange = (val: string) => {
			onChange?.(val);
			rerender();
		};
		editor.disableSubmit = disableSubmit ?? false;
		editor.lineNumbers = lineNumbers;
		if (autocompleteProvider) editor.setAutocompleteProvider(autocompleteProvider);
	});

	useEffect(() => {
		if (value !== undefined && value !== editor.getText()) {
			editor.setText(value);
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

	function rerender() {
		const width = tui.terminal.columns;
		setRenderedLines(editor.render(width));
	}

	return React.createElement(
		'twinki-box',
		{ flexDirection: 'column' },
		...renderedLines.map((line, i) =>
			React.createElement('twinki-text', { key: i, wrap: 'truncate' }, line),
		),
	);
};

EditorInput.displayName = 'EditorInput';
