/**
 * <Typewriter> — progressively reveals text with natural pacing.
 *
 * During streaming: <Markdown> (fast ANSI strings, minimal React nodes).
 * On completion: <Markdown highlight> (shiki syntax highlighting).
 */
import React from 'react';
import { Markdown } from './Markdown.js';
import { Text } from './Text.js';
import { useTypewriter } from '../hooks/useTypewriter.js';
import type { TypewriterSpeed } from '../hooks/useTypewriter.js';
import type { MarkdownTheme } from './Markdown.js';

export interface TypewriterProps {
	children: string;
	/** Speed preset or words-per-second. Default: 'natural' */
	speed?: TypewriterSpeed | number;
	/** Render as Markdown (default: true) or plain Text */
	markdown?: boolean;
	/** Markdown theme (only when markdown=true) */
	theme?: MarkdownTheme;
	/** Called when all text has been revealed */
	onComplete?: () => void;
}

function closeOpenFences(text: string): string {
	const fenceCount = (text.match(/^```/gm) || []).length;
	return fenceCount % 2 === 1 ? text + '\n```' : text;
}

export const Typewriter: React.FC<TypewriterProps> = ({
	children,
	speed = 'natural',
	markdown = true,
	theme,
	onComplete,
}) => {
	const { visibleText, isComplete } = useTypewriter(children, { speed, onComplete });

	if (!visibleText) return null;

	if (!markdown) {
		return React.createElement(Text, null, visibleText);
	}

	return React.createElement(Markdown, {
		children: closeOpenFences(visibleText),
		theme,
	});
};
