import { getEditorKeybindings } from '../input/keybindings.js';
import type { Component } from '../renderer/component.js';
import { truncateToWidth } from '../utils/slice.js';

const normalizeToSingleLine = (text: string): string => text.replace(/[\r\n]+/g, ' ').trim();

export interface SelectItem {
	value: string;
	label: string;
	description?: string;
}

export interface SelectListTheme {
	selectedPrefix: (text: string) => string;
	selectedText: (text: string) => string;
	description: (text: string) => string;
	scrollInfo: (text: string) => string;
	noMatch: (text: string) => string;
}

const DEFAULT_THEME: SelectListTheme = {
	selectedPrefix: (t) => `\x1b[1m${t}\x1b[22m`,
	selectedText: (t) => `\x1b[1m${t}\x1b[22m`,
	description: (t) => `\x1b[2m${t}\x1b[22m`,
	scrollInfo: (t) => `\x1b[2m${t}\x1b[22m`,
	noMatch: (t) => `\x1b[2m${t}\x1b[22m`,
};

export class SelectList implements Component {
	private items: SelectItem[] = [];
	private filteredItems: SelectItem[] = [];
	private selectedIndex: number = 0;
	private maxVisible: number = 5;
	private theme: SelectListTheme;

	public onSelect?: (item: SelectItem) => void;
	public onCancel?: () => void;
	public onSelectionChange?: (item: SelectItem) => void;

	constructor(items: SelectItem[], maxVisible = 5, theme?: SelectListTheme) {
		this.items = items;
		this.filteredItems = items;
		this.maxVisible = maxVisible;
		this.theme = theme ?? DEFAULT_THEME;
	}

	setFilter(filter: string): void {
		this.filteredItems = this.items.filter((item) =>
			item.value.toLowerCase().startsWith(filter.toLowerCase()),
		);
		this.selectedIndex = 0;
	}

	setItems(items: SelectItem[]): void {
		this.items = items;
		this.filteredItems = items;
		this.selectedIndex = 0;
	}

	setSelectedIndex(index: number): void {
		this.selectedIndex = Math.max(0, Math.min(index, this.filteredItems.length - 1));
	}

	getSelectedItem(): SelectItem | null {
		return this.filteredItems[this.selectedIndex] ?? null;
	}

	invalidate(): void {}

	handleInput(keyData: string): void {
		const kb = getEditorKeybindings();

		if (kb.matches(keyData, 'selectUp')) {
			this.selectedIndex = this.selectedIndex === 0 ? this.filteredItems.length - 1 : this.selectedIndex - 1;
			this.notifySelectionChange();
		} else if (kb.matches(keyData, 'selectDown')) {
			this.selectedIndex = this.selectedIndex === this.filteredItems.length - 1 ? 0 : this.selectedIndex + 1;
			this.notifySelectionChange();
		} else if (kb.matches(keyData, 'selectConfirm')) {
			const item = this.filteredItems[this.selectedIndex];
			if (item) this.onSelect?.(item);
		} else if (kb.matches(keyData, 'selectCancel')) {
			this.onCancel?.();
		}
	}

	render(width: number): string[] {
		if (this.filteredItems.length === 0) {
			return [this.theme.noMatch('  No matching items')];
		}

		const lines: string[] = [];
		const startIndex = Math.max(
			0,
			Math.min(this.selectedIndex - Math.floor(this.maxVisible / 2), this.filteredItems.length - this.maxVisible),
		);
		const endIndex = Math.min(startIndex + this.maxVisible, this.filteredItems.length);

		for (let i = startIndex; i < endIndex; i++) {
			const item = this.filteredItems[i]!;
			const isSelected = i === this.selectedIndex;
			const desc = item.description ? normalizeToSingleLine(item.description) : undefined;
			const displayValue = item.label || item.value;
			const prefix = isSelected ? '→ ' : '  ';
			const styleFn = isSelected ? this.theme.selectedText : (t: string) => t;

			if (desc && width > 40) {
				const maxValueWidth = Math.min(30, width - prefix.length - 4);
				const truncatedValue = truncateToWidth(displayValue, maxValueWidth, '');
				const spacing = ' '.repeat(Math.max(1, 32 - truncatedValue.length));
				const descStart = prefix.length + truncatedValue.length + spacing.length;
				const remainingWidth = width - descStart - 2;

				if (remainingWidth > 10) {
					const truncatedDesc = truncateToWidth(desc, remainingWidth, '');
					if (isSelected) {
						lines.push(styleFn(`${prefix}${truncatedValue}${spacing}${truncatedDesc}`));
					} else {
						lines.push(`${prefix}${truncatedValue}${this.theme.description(spacing + truncatedDesc)}`);
					}
				} else {
					lines.push(styleFn(`${prefix}${truncateToWidth(displayValue, width - prefix.length - 2, '')}`));
				}
			} else {
				lines.push(styleFn(`${prefix}${truncateToWidth(displayValue, width - prefix.length - 2, '')}`));
			}
		}

		if (startIndex > 0 || endIndex < this.filteredItems.length) {
			lines.push(this.theme.scrollInfo(truncateToWidth(`  (${this.selectedIndex + 1}/${this.filteredItems.length})`, width - 2, '')));
		}

		return lines;
	}

	private notifySelectionChange(): void {
		const item = this.filteredItems[this.selectedIndex];
		if (item) this.onSelectionChange?.(item);
	}
}
