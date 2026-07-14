/**
 * useTabs — tab collection state management for the Tabs component.
 *
 * Owns the ordered list of open tabs, the active tab id, and operations to
 * open/close/activate/reorder. Purely UI state — what each tab "contains" is
 * the consumer's concern (map activeId to content in a switch/lookup).
 */
import { useCallback, useState } from 'react';
import type { Tab } from '../components/Tabs.js';

export interface UseTabsOpts {
	/** Initial set of open tabs. */
	initial?: Tab[];
	/** Called when a tab is closed; return false to veto. */
	onBeforeClose?: (id: string) => boolean | void;
}

export interface TabsModel {
	/** Ordered open tabs (render order = strip order). */
	tabs: Tab[];
	/** Id of the active tab, or '' when no tabs are open. */
	activeId: string;
	/** Activate a tab by id (no-op if not found). */
	activate: (id: string) => void;
	/** Open a new tab (or activate it if already open). Focus it. */
	open: (tab: Tab) => void;
	/** Close a tab. If it's active, activate the adjacent one. */
	close: (id: string) => void;
	/** Cycle to the next/prev tab (wraps). */
	cycleNext: () => void;
	cyclePrev: () => void;
	/** Jump to tab by 0-based index. */
	jumpTo: (index: number) => void;
	/** Mark a tab dirty/clean. */
	setDirty: (id: string, dirty: boolean) => void;
	/** Update a tab's title (e.g. after a rename); no-op if the id is unknown. */
	setTitle: (id: string, title: string) => void;
}

/**
 * Tab-collection state for the {@link Tabs} component: ordered open tabs, the
 * active id, and open/close/activate/cycle/jump operations. What each tab
 * *contains* is the consumer's concern — map `activeId` to content.
 */
export function useTabs(opts: UseTabsOpts = {}): TabsModel {
	const [tabs, setTabs] = useState<Tab[]>(opts.initial ?? []);
	const [activeId, setActiveId] = useState<string>(() =>
		(opts.initial?.[0]?.id) ?? '',
	);

	const activate = useCallback((id: string) => {
		setTabs((ts) => {
			if (ts.some((t) => t.id === id)) {
				setActiveId(id);
			}
			return ts;
		});
	}, []);

	const open = useCallback((tab: Tab) => {
		setTabs((ts) => {
			const existing = ts.find((t) => t.id === tab.id);
			if (existing) {
				setActiveId(tab.id);
				return ts;
			}
			setActiveId(tab.id);
			return [...ts, tab];
		});
	}, []);

	const close = useCallback(
		(id: string) => {
			if (opts.onBeforeClose && opts.onBeforeClose(id) === false) return;
			setTabs((ts) => {
				const idx = ts.findIndex((t) => t.id === id);
				if (idx === -1) return ts;
				const next = ts.filter((t) => t.id !== id);
				setActiveId((prev) => {
					if (prev !== id) return prev;
					// Pick adjacent: prefer next, fallback to prev, or empty.
					const nextTab = next[Math.min(idx, next.length - 1)];
					return nextTab?.id ?? '';
				});
				return next;
			});
		},
		[opts.onBeforeClose],
	);

	const cycleNext = useCallback(() => {
		setTabs((ts) => {
			setActiveId((prev) => {
				const idx = ts.findIndex((t) => t.id === prev);
				return ts[(idx + 1) % ts.length]?.id ?? prev;
			});
			return ts;
		});
	}, []);

	const cyclePrev = useCallback(() => {
		setTabs((ts) => {
			setActiveId((prev) => {
				const idx = ts.findIndex((t) => t.id === prev);
				return ts[(idx - 1 + ts.length) % ts.length]?.id ?? prev;
			});
			return ts;
		});
	}, []);

	const jumpTo = useCallback((index: number) => {
		setTabs((ts) => {
			const tab = ts[index];
			if (tab) setActiveId(tab.id);
			return ts;
		});
	}, []);

	const setDirty = useCallback((id: string, dirty: boolean) => {
		setTabs((ts) =>
			ts.map((t) => (t.id === id ? { ...t, dirty } : t)),
		);
	}, []);

	const setTitle = useCallback((id: string, title: string) => {
		setTabs((ts) =>
			ts.map((t) => (t.id === id && t.title !== title ? { ...t, title } : t)),
		);
	}, []);

	return {
		tabs,
		activeId,
		activate,
		open,
		close,
		cycleNext,
		cyclePrev,
		jumpTo,
		setDirty,
		setTitle,
	};
}
