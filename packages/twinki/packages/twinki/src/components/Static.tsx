import React from 'react';

/**
 * Props for the Static component.
 */
export interface StaticProps<T> {
	/** Array of items to render */
	items: T[];
	/** Function to render each item */
	children: (item: T, index: number) => React.ReactNode;
	/** Additional styling properties */
	style?: Record<string, any>;
}

/**
 * Static component for rendering content to the terminal's scrollback buffer.
 * 
 * The Static component is used for content that should be written to the
 * terminal's scrollback history rather than the live interactive area.
 * This is useful for logs, messages, or any content that should persist
 * even when the interactive UI updates.
 * 
 * Key features:
 * - Content goes to scrollback, not live area
 * - Preserves terminal history
 * - ReactBridge tracks which items have been written and skips duplicates
 * 
 * @param props - The component props
 * @param props.items - Array of items to render
 * @param props.children - Function to render each item
 * @param props.style - Additional styling properties
 * @returns A React element representing static content or null if empty
 */
export function Static<T>({ items, children }: StaticProps<T>): React.ReactElement | null {
	if (items.length === 0) return null;
	return React.createElement(
		'twinki-static',
		null,
		...items.map((item, i) => children(item, i)),
	);
}

(Static as any).displayName = 'Static';

(Static as any).displayName = 'Static';
