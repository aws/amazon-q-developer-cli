// Terminal and input
export type { Terminal } from './terminal/index.js';
export { ProcessTerminal } from './terminal/index.js';
export type { KeyId, KeyEventType } from './input/index.js';
export { Key, matchesKey, parseKey } from './input/index.js';
export type { MouseEvent, MouseButton, MouseEventType } from './input/index.js';

// Utilities
export { visibleWidth } from './utils/index.js';
export { getHighlighter } from './utils/shiki.js';
import { visibleWidth as _visibleWidth } from './utils/index.js';

// Renderer
export { TUI } from './renderer/index.js';
export type { Component, InputListener, OverlayHandle, OverlayOptions, OverlayAnchor } from './renderer/index.js';

// React integration
export { render } from './reconciler/index.js';
export type { TwinkiRenderOptions, Instance, RenderMetrics } from './reconciler/index.js';

// Components
export { Text, Box, Newline, Spacer, Static, Transform, Markdown, Typewriter, DiffView, TextInput, Input, Select, SelectList, Editor, EditorInput } from './components/index.js';
export type { TextProps, BoxProps, NewlineProps, StaticProps, TransformProps, MarkdownProps, TypewriterProps, DiffViewProps, DiffSide, TextInputProps, SelectProps, SelectItem, AutocompleteProvider, EditorTheme, EditorInputProps } from './components/index.js';

// Hooks
export { useInput, useApp, useStdin, useStdout, useStderr, useFocus, useFocusManager, useTwinkiContext } from './hooks/index.js';
export type { Key as InkKey, UseInputOptions, UseFocusOptions, TwinkiContext } from './hooks/index.js';
export { useTypewriter } from './hooks/index.js';
export type { UseTypewriterOptions, TypewriterSpeed } from './hooks/index.js';
export { useFrames } from './hooks/index.js';
export { useMouse } from './hooks/index.js';
export type { UseMouseOptions } from './hooks/index.js';
export { usePaste } from './hooks/index.js';
export type { UsePasteOptions } from './hooks/index.js';
export { useFullscreen } from './hooks/index.js';
export { useKeyRelease } from './hooks/index.js';
export type { UseKeyReleaseOptions } from './hooks/index.js';
export { useKeyRepeat } from './hooks/index.js';
export type { UseKeyRepeatOptions } from './hooks/index.js';
export { useOverlay } from './hooks/index.js';

// Animation utilities
export { halfBlock, stamp, renderGrid, createGrid, solidBg, radialGlow, COLOR_RGB } from './animation/index.js';
export type { RGB, SpriteGrid } from './animation/index.js';

// Compatibility
/**
 * Measures the visual dimensions of text, accounting for ANSI escape sequences.
 * 
 * This function provides Ink compatibility by measuring text width and height
 * while properly handling ANSI escape sequences that don't contribute to visual width.
 * 
 * @param text - The text to measure, may contain ANSI escape sequences
 * @returns Object containing the visual width and height of the text
 * 
 * @example
 * ```typescript
 * const dimensions = measureText('Hello\nWorld');
 * console.log(dimensions); // { width: 5, height: 2 }
 * 
 * const coloredText = measureText('\x1b[31mRed text\x1b[0m');
 * console.log(coloredText); // { width: 8, height: 1 }
 * ```
 */
export function measureText(text: string): { width: number; height: number } {
	const lines = text.split('\n');
	return {
		width: Math.max(...lines.map((l) => _visibleWidth(l))),
		height: lines.length,
	};
}

/**
 * Measures the rendered dimensions of a component node (Ink compatibility).
 * The ref must point to a TwinkiNode with a computed Yoga layout.
 * Forces a layout pass if the node is dirty so measurements are accurate
 * even when called from useLayoutEffect (before twinki's render tick).
 */
export function measureElement(node: any): { width: number; height: number } {
	if (node?.yogaNode) {
		// Force layout via rootContainer if available and dirty
		const rc = node.rootContainer;
		if (rc?.yogaNode) {
			const width = process.stdout.columns || 80;
			rc.yogaNode.setWidth(width);
			rc.yogaNode.calculateLayout(width, undefined, 1 /* LTR */);
		}
		return {
			width: node.yogaNode.getComputedWidth(),
			height: node.yogaNode.getComputedHeight(),
		};
	}
	return { width: 0, height: 0 };
}
