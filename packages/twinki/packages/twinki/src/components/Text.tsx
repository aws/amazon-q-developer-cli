import React from 'react';

/**
 * Props for the Text component.
 */
export interface TextProps {
	/** Text content to display */
	children?: React.ReactNode;
	/** Text color (named color or hex) */
	color?: string;
	/** Background color (named color or hex) */
	backgroundColor?: string;
	/** Whether to dim the text color */
	dimColor?: boolean;
	/** Whether to make text bold */
	bold?: boolean;
	/** Whether to make text italic */
	italic?: boolean;
	/** Whether to underline text */
	underline?: boolean;
	/** Whether to strike through text */
	strikethrough?: boolean;
	/** Whether to invert foreground and background colors */
	inverse?: boolean;
	/** How to handle text that exceeds available width */
	wrap?: 'wrap' | 'truncate' | 'truncate-end' | 'truncate-start' | 'truncate-middle';

	// Mouse events
	/** Called on mouse click */
	onClick?: () => void;
	/** Called when mouse enters the element */
	onMouseEnter?: () => void;
	/** Called when mouse leaves the element */
	onMouseLeave?: () => void;
}

/**
 * Text component for displaying styled text content.
 * 
 * The Text component is the primary way to display text in Twinki applications.
 * It supports various styling options including colors, text formatting, and
 * different wrapping behaviors for handling long text.
 * 
 * Text wrapping modes:
 * - `wrap`: Wrap text to multiple lines (default)
 * - `truncate`/`truncate-end`: Cut off text at the end with no indicator
 * - `truncate-start`: Cut off text at the beginning
 * - `truncate-middle`: Cut off text in the middle with ellipsis
 * 
 * @param props - The component props
 * @param props.children - Text content to display
 * @param props.color - Text color (named color or hex)
 * @param props.backgroundColor - Background color (named color or hex)
 * @param props.dimColor - Whether to dim the text color
 * @param props.bold - Whether to make text bold
 * @param props.italic - Whether to make text italic
 * @param props.underline - Whether to underline text
 * @param props.strikethrough - Whether to strike through text
 * @param props.inverse - Whether to invert foreground and background colors
 * @param props.wrap - How to handle text that exceeds available width
 * @returns A React element representing styled text
 * 
 * @example
 * ```tsx
 * <Text color="red" bold>Error: Something went wrong</Text>
 * <Text wrap="truncate-middle">Very long text that will be truncated</Text>
 * <Text backgroundColor="blue" color="white">Highlighted text</Text>
 * ```
 */
export const Text: React.FC<TextProps> = (props) => {
	return React.createElement('twinki-text', { flexDirection: 'row', flexGrow: 0, flexShrink: 1, ...props }, props.children);
};

Text.displayName = 'Text';
