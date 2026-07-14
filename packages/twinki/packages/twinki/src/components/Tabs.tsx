/**
 * Tabs — a horizontal tab strip component for twinki.
 *
 * Renders a row of tab labels with an inverted active chip, dirty markers (●),
 * close affordances (✕), and active-anchored overflow windowing ("‹N" / "N›"
 * hidden-count markers on each side — the active tab is always visible).
 * Purely presentational: all state (active id, ordering, open/close) is owned
 * by the consumer or by the companion `useTabs` hook.
 *
 * Keyboard: Ctrl+Tab cycles right, Ctrl+Shift+Tab cycles left (consumer wires
 * these through useInput → the hook); Ctrl+1..9 jumps to tab N; Ctrl+w closes.
 * Mouse: click a tab to activate, click ✕ to close.
 */
import React from 'react';
import { Box } from './Box.js';
import { Text } from './Text.js';

// --- Public types ----------------------------------------------------------------

/** One tab in a {@link Tabs} strip. Identity is `id`; the rest is presentation. */
export interface Tab {
	/** Stable identity — used for activation, close, and React keys. */
	id: string;
	/** Display label shown in the chip. */
	title: string;
	/** Optional leading glyph (e.g. a filetype or status icon). */
	icon?: string;
	/** Color for the icon glyph (e.g. a live status color); defaults to the tab text color. */
	iconColor?: string;
	/** Renders a dirty marker (●) on the tab. */
	dirty?: boolean;
	/** Whether the tab shows a close affordance (✕) and accepts close. */
	closable?: boolean;
}

/** Props for the {@link Tabs} strip: the open tabs, which is active, and the
 * activate/close callbacks. State lives in the consumer (see `useTabs`). */
export interface TabsProps {
	/** Ordered open tabs; render order is the on-screen strip order. */
	tabs: Tab[];
	/** Id of the currently active tab (rendered as the inverted chip). */
	activeId: string;
	/** Called when the user clicks (or keyboard-activates) a tab. */
	onActivate: (id: string) => void;
	/** Called when the user closes a tab (✕ click or Ctrl+w). */
	onClose?: (id: string) => void;
	/**
	 * Render each tab's 1-based index before its title (e.g. "1:app.ts") so
	 * Ctrl+1..9 jump targets are discoverable. Indexes refer to positions in
	 * the FULL tab list, not the visible window.
	 */
	showIndexes?: boolean;
	/** Total width available for the strip (columns). Tabs overflow beyond this. */
	width?: number;
	/** Active tab accent color (default: yellow). */
	activeColor?: string;
	/** Inactive tab text color (default: dim gray). */
	inactiveColor?: string;
	/** Strip border color (default: dim gray). */
	borderColor?: string;
	/**
	 * Background painted across the WHOLE strip row (inactive tabs and empty
	 * trailing space sit on this band, like an editor buffer bar). Unset keeps
	 * the terminal's default background.
	 */
	stripColor?: string;
	/**
	 * Text color painted over the active chip (default: near-black). The active
	 * tab renders as an inverted chip — accent background, dark text — so it
	 * reads at a glance like an editor buffer bar.
	 */
	activeTextColor?: string;
}

// --- Helpers --------------------------------------------------------------------

const DEFAULT_ACTIVE = '#ffd866';
const DEFAULT_INACTIVE = '#727072';
const DEFAULT_BORDER = '#727072';

/** Rendered width of one tab chip (including padding, icon, dirty/close markers). */
function tabWidth(tab: Tab, indexDigits = 0, isActive = false): number {
	let w = 1;
	if (indexDigits > 0) w += indexDigits + 1;
	if (tab.icon) w += tab.icon.length + 1;
	w += tab.title.length;
	if (tab.dirty) w += 1;
	if (tab.closable && isActive) w += 2;
	w += 1;
	return w;
}

/**
 * Compute which tabs are visible given a width budget, keeping the active tab
 * centered. Returns the visible slice + how many are hidden on each side.
 *
 * Algorithm: start with the active tab, then alternately try to include the
 * next tab to the right, then the next to the left, until neither fits.
 * Each side reserves space for an overflow marker ("‹N" / "N›") when hidden
 * tabs exist beyond it.
 */
export function computeVisibleWindow(
	tabs: Tab[], activeId: string, width: number, showIndexes: boolean,
): { visibleTabs: Tab[]; hiddenBefore: number; hiddenAfter: number } {
	if (width <= 0 || tabs.length === 0) {
		return { visibleTabs: tabs, hiddenBefore: 0, hiddenAfter: 0 };
	}

	const OVERFLOW_MARKER_WIDTH = 4; // "‹NN " or " NN›"
	const indexDigits = showIndexes ? String(tabs.length).length : 0;
	const activeIndex = Math.max(0, tabs.findIndex((t) => t.id === activeId));

	// Window boundaries (inclusive indices into the tabs array)
	let firstVisible = activeIndex;
	let lastVisible = activeIndex;
	let usedWidth = tabWidth(tabs[activeIndex], indexDigits, true) + 1;

	while (true) {
		const hasHiddenLeft = firstVisible > 0;
		const hasHiddenRight = lastVisible < tabs.length - 1;

		// Available budget = total width minus space reserved for overflow markers
		const availableBudget = width
			- (hasHiddenLeft ? OVERFLOW_MARKER_WIDTH : 0)
			- (hasHiddenRight ? OVERFLOW_MARKER_WIDTH : 0);

		// Try to expand right first, then left
		const canExpandRight = hasHiddenRight;
		const canExpandLeft = hasHiddenLeft;
		const rightCost = canExpandRight ? tabWidth(tabs[lastVisible + 1], indexDigits) + 1 : Infinity;
		const leftCost = canExpandLeft ? tabWidth(tabs[firstVisible - 1], indexDigits) + 1 : Infinity;

		// Pick the cheaper side (right-biased on ties) if it fits the budget
		if (rightCost <= leftCost && usedWidth + rightCost <= availableBudget) {
			lastVisible++;
			usedWidth += rightCost;
		} else if (leftCost < Infinity && usedWidth + leftCost <= availableBudget) {
			firstVisible--;
			usedWidth += leftCost;
		} else {
			break; // neither side fits
		}
	}

	return {
		visibleTabs: tabs.slice(firstVisible, lastVisible + 1),
		hiddenBefore: firstVisible,
		hiddenAfter: tabs.length - 1 - lastVisible,
	};
}

// --- Component -------------------------------------------------------------------

/**
 * Horizontal tab strip. The active tab renders as an inverted chip (accent bg,
 * dark text); overflow windows around the active tab with ‹N / N› markers so
 * it can never scroll out of view. Click activates; ✕ closes. Pair with
 * {@link useTabs} for state.
 */
export const Tabs: React.FC<TabsProps> = ({
	tabs,
	activeId,
	onActivate,
	onClose,
	showIndexes = false,
	width,
	activeColor = DEFAULT_ACTIVE,
	inactiveColor = DEFAULT_INACTIVE,
	borderColor = DEFAULT_BORDER,
	activeTextColor = '#221f22',
	stripColor,
}) => {
	const { visibleTabs, hiddenBefore, hiddenAfter } = width !== undefined
		? computeVisibleWindow(tabs, activeId, width, showIndexes)
		: { visibleTabs: tabs, hiddenBefore: 0, hiddenAfter: 0 };

	return (
		<Box flexDirection="row" width={width} backgroundColor={stripColor}>
			{hiddenBefore > 0 && (
				<Text color={inactiveColor} backgroundColor={stripColor}>{`‹${hiddenBefore} `}</Text>
			)}
			{visibleTabs.map((tab, i) => {
				// The active tab renders as an inverted chip (accent bg, dark text);
				// inactive tabs stay dim text on the default background.
				const active = tab.id === activeId;
				const fg = active ? activeTextColor : inactiveColor;
				const bg = active ? activeColor : stripColor;
				// 1-based index into the FULL list (Ctrl+N jump target).
				const fullIndex = hiddenBefore + i + 1;
				return (
					<Box key={tab.id} onClick={() => onActivate(tab.id)}>
						{showIndexes && (
							<Text color={fg} backgroundColor={bg} bold={active}>{` ${fullIndex}:`}</Text>
						)}
						{tab.icon && (
							<Text color={active ? fg : tab.iconColor ?? fg} backgroundColor={bg} bold={active}>
								{`${showIndexes ? '' : ' '}${tab.icon}`}
							</Text>
						)}
						<Text color={fg} backgroundColor={bg} bold={active}>
							{' '}
							{tab.title}
							{tab.dirty ? '●' : ''}
							{tab.closable && active ? (
								<Text color={fg} backgroundColor={bg} onClick={() => onClose?.(tab.id)}>
									{' ✕'}
								</Text>
							) : ''}
							{' '}
						</Text>
						{i < visibleTabs.length - 1 && (
							<Text color={borderColor} backgroundColor={stripColor}>│</Text>
						)}
					</Box>
				);
			})}
			{hiddenAfter > 0 && (
				<Text color={inactiveColor} backgroundColor={stripColor}>{` ${hiddenAfter}›`}</Text>
			)}
		</Box>
	);
};
