import React, { useEffect, useRef, useState } from 'react';
import { useTwinkiContext } from '../hooks/context.js';
import { SelectList, type SelectItem } from './SelectList.js';

export type { SelectItem } from './SelectList.js';

export interface SelectProps {
	/** Items to display */
	items: SelectItem[];
	/** Max visible items before scrolling (default: 5) */
	maxVisible?: number;
	/** Called when user selects an item */
	onSelect?: (item: SelectItem) => void;
	/** Called when user cancels (Escape) */
	onCancel?: () => void;
	/** Called when selection changes */
	onChange?: (item: SelectItem) => void;
	/** Filter string for items */
	filter?: string;
	/** Whether input is active (default: true) */
	isActive?: boolean;
}

/**
 * React wrapper for the SelectList component.
 * Scrollable selection list with keyboard navigation and filtering.
 */
export const Select: React.FC<SelectProps> = ({
	items,
	maxVisible = 5,
	onSelect,
	onCancel,
	onChange,
	filter,
	isActive = true,
}) => {
	const { tui } = useTwinkiContext();
	const listRef = useRef<SelectList>(null!);
	const [renderedLines, setRenderedLines] = useState<string[]>([]);

	if (!listRef.current) {
		listRef.current = new SelectList(items, maxVisible);
	}

	const list = listRef.current;

	// Sync props
	useEffect(() => {
		list.onSelect = onSelect;
		list.onCancel = onCancel;
		list.onSelectionChange = (item) => {
			onChange?.(item);
			rerender();
		};
	});

	// Sync items
	useEffect(() => {
		list.setItems(items);
		rerender();
	}, [items]);

	// Sync filter
	useEffect(() => {
		if (filter !== undefined) {
			list.setFilter(filter);
			rerender();
		}
	}, [filter]);

	// Input handling
	useEffect(() => {
		if (!isActive) return;
		rerender();

		const unsub = tui.addInputListener((data) => {
			list.handleInput(data);
			rerender();
		});

		return unsub;
	}, [tui, isActive]);

	function rerender() {
		const width = tui.terminal.columns;
		setRenderedLines(list.render(width));
	}

	return React.createElement(
		'twinki-box',
		{ flexDirection: 'column' },
		...renderedLines.map((line, i) =>
			React.createElement('twinki-text', { key: i, wrap: 'truncate' }, line),
		),
	);
};

Select.displayName = 'Select';
