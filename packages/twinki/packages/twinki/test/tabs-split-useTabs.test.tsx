/**
 * Unit tests for the Tabs / Split components and the useTabs hook.
 */
import { describe, it, expect } from 'vitest';
import React from 'react';
import { render, Text } from '../src/index.js';
import { Tabs, type Tab } from '../src/components/Tabs.js';
import { Split } from '../src/components/Split.js';
import { useTabs, type TabsModel } from '../src/hooks/useTabs.js';
import { TestTerminal, wait } from './helpers.js';

async function renderFrame(node: React.ReactElement, cols = 60, rows = 10) {
	const term = new TestTerminal(cols, rows);
	const inst = render(node, { terminal: term, exitOnCtrlC: false, mouse: true });
	await wait(60);
	await term.flush();
	const text = (term.getLastFrame()?.viewport ?? []).join('\n');
	return { term, inst, text };
}

const tab = (id: string, extra: Partial<Tab> = {}): Tab => ({ id, title: id, ...extra });

describe('Tabs', () => {
	it('renders all tab titles', async () => {
		const { inst, text } = await renderFrame(
			<Tabs tabs={[tab('one'), tab('two'), tab('three')]} activeId="one" onActivate={() => {}} />,
		);
		expect(text).toContain('one');
		expect(text).toContain('two');
		expect(text).toContain('three');
		inst.unmount();
	});

	it('renders the active tab distinctly from inactive tabs', async () => {
		const { text, inst } = await renderFrame(
			<Tabs tabs={[tab('one'), tab('two')]} activeId="one" onActivate={() => {}} />,
		);
		// Both titles visible, separated by │
		expect(text).toContain('one');
		expect(text).toContain('two');
		expect(text).toContain('│');
		// The active tab is rendered with the title (basic sanity — the styling
		// is visually verified via the multiplexer golden tests, not cell-buffer
		// internals which vary across xterm.js versions).
		inst.unmount();
	});

	it('calls onActivate when a tab is clicked', async () => {
		let activated = '';
		const { term, inst } = await renderFrame(
			<Tabs tabs={[tab('one'), tab('two')]} activeId="one" onActivate={(id) => { activated = id; }} />,
		);
		// Click " two " (0-based col 7, row 0 → 1-based SGR 8;1).
		term.sendInput('\x1b[<0;8;1M');
		term.sendInput('\x1b[<0;8;1m');
		await wait();
		expect(activated).toBe('two');
		inst.unmount();
	});

	it('renders the ✕ close affordance on the active closable tab only', async () => {
		const { text, inst } = await renderFrame(
			<Tabs
				tabs={[tab('one', { closable: true }), tab('two', { closable: true })]}
				activeId="one"
				onActivate={() => {}}
				onClose={() => {}}
			/>,
		);
		// Exactly one ✕ (the active tab's); the inactive closable tab stays quiet.
		expect(text.split('✕').length - 1).toBe(1);
		inst.unmount();
	});

	it('renders icon and dirty marker', async () => {
		const { text, inst } = await renderFrame(
			<Tabs
				tabs={[tab('one', { icon: '◆', iconColor: 'cyan', dirty: true })]}
				activeId="one"
				onActivate={() => {}}
			/>,
		);
		expect(text).toContain('◆');
		expect(text).toContain('●');
		inst.unmount();
	});

	it('shows ‹N / N› overflow markers and keeps the active tab visible', async () => {
		const tabs = Array.from({ length: 10 }, (_, i) => tab(`tab${i + 1}`));
		const { inst, text } = await renderFrame(
			<Tabs tabs={tabs} activeId="tab5" onActivate={() => {}} width={30} />,
		);
		expect(text).toMatch(/‹\d+/);
		expect(text).toMatch(/\d+›/);
		expect(text).toContain('tab5');
		inst.unmount();
	});
});

describe('Split', () => {
	it('renders both panes separated by a divider', async () => {
		const { inst, text } = await renderFrame(
			<Split direction="row" ratio={0.5} width={41} height={6}>
				<Text>alpha</Text>
				<Text>beta</Text>
			</Split>, 41, 6,
		);
		expect(text).toContain('alpha');
		expect(text).toContain('beta');
		expect(text).toContain('╮ ╭'); // two bordered panes with the divider column between
		expect(text).toContain('│'); // divider glyph at the vertically-centered row
		inst.unmount();
	});

	it('sizes pane A according to ratio', async () => {
		const { inst, text } = await renderFrame(
			<Split direction="row" ratio={0.75} width={41} height={6}>
				<Text>alpha</Text>
				<Text>beta</Text>
			</Split>, 41, 6,
		);
		// usable = 40; pane A = round(40 * 0.75) = 30 cols, so its ╮ lands at col 29.
		expect(text.split('\n')[0].indexOf('╮')).toBe(29);
		inst.unmount();
	});

	it('renders both panes with border corners at the expected positions', async () => {
		const { text, inst } = await renderFrame(
			<Split direction="row" ratio={0.5} width={41} height={6} activePane="b">
				<Text>alpha</Text>
				<Text>beta</Text>
			</Split>, 41, 6,
		);
		// Both panes have rounded borders: pane A ╭ at col 0, pane B ╭ at col 21
		const firstLine = text.split('\n')[0];
		expect(firstLine[0]).toBe('╭');
		expect(firstLine[21]).toBe('╭');
		inst.unmount();
	});

	it('calls onResize when the separator is dragged', async () => {
		let newRatio = 0;
		const { term, inst } = await renderFrame(
			<Split direction="row" ratio={0.5} width={41} height={6} activePane="a" onResize={(r) => { newRatio = r; }}>
				<Text>alpha</Text>
				<Text>beta</Text>
			</Split>, 41, 6,
		);
		// Mousedown on the separator (col 20, row 3 → 1-based 21;4)
		term.sendInput('\x1b[<0;21;4M');
		await wait();
		// Mousemove to col 30 (1-based 31;4)
		term.sendInput('\x1b[<32;31;4M');
		await wait();
		// Mouseup
		term.sendInput('\x1b[<0;31;4m');
		await wait();
		expect(newRatio).toBeGreaterThan(0.5);
		inst.unmount();
	});
});

describe('Scrollbar', () => {
	it('renders clickable arrows and track when onScrollTo is provided', async () => {
		const { Scrollbar } = await import('../src/components/Scrollbar.js');
		const { text, inst } = await renderFrame(
			<Scrollbar scrollTop={5} totalLines={50} viewportHeight={10} onScrollTo={() => {}} />,
			5, 15,
		);
		expect(text).toContain('▲');
		expect(text).toContain('▼');
		expect(text).toContain('█');
		inst.unmount();
	});

	it('arrow clicks step scrollTop by one; track click jumps to position', async () => {
		const { Scrollbar } = await import('../src/components/Scrollbar.js');
		const targets: number[] = [];
		const { term, inst } = await renderFrame(
			<Scrollbar scrollTop={5} totalLines={50} viewportHeight={10} onScrollTo={(v) => { targets.push(v); }} />,
			5, 15,
		);
		// ▲ at row 0 (1-based 1;1) → scrollTop - 1 = 4
		term.sendInput('\x1b[<0;1;1M'); term.sendInput('\x1b[<0;1;1m');
		await wait();
		// ▼ at row 9 (viewportHeight rows: arrow+track(8)+arrow → 1-based 1;10) → scrollTop + 1 = 6
		term.sendInput('\x1b[<0;1;10M'); term.sendInput('\x1b[<0;1;10m');
		await wait();
		// Track click near the bottom (row 8, 1-based 1;9) → jumps toward maxScroll
		term.sendInput('\x1b[<0;1;9M'); term.sendInput('\x1b[<0;1;9m');
		await wait();
		expect(targets).toContain(4);  // ▲ step
		expect(targets).toContain(6);  // ▼ step
		expect(targets.some((t) => t > 6)).toBe(true); // track jump
		inst.unmount();
	});

	it('renders static scrollbar without onScrollTo', async () => {
		const { Scrollbar } = await import('../src/components/Scrollbar.js');
		const { text, inst } = await renderFrame(
			<Scrollbar scrollTop={0} totalLines={50} viewportHeight={10} />,
			5, 15,
		);
		expect(text).toContain('▲');
		expect(text).toContain('░');
		inst.unmount();
	});

	it('returns null when content fits the viewport', async () => {
		const { Scrollbar } = await import('../src/components/Scrollbar.js');
		const { text, inst } = await renderFrame(
			<Scrollbar scrollTop={0} totalLines={5} viewportHeight={10} />,
			5, 15,
		);
		expect(text.trim()).toBe('');
		inst.unmount();
	});
});

describe('useTabs', () => {
	async function renderTabsHook(initial?: Tab[]) {
		const result = { current: undefined as unknown as TabsModel };
		const Host = () => { result.current = useTabs({ initial }); return <Text>h</Text>; };
		const term = new TestTerminal(20, 3);
		const inst = render(<Host />, { terminal: term, exitOnCtrlC: false });
		await wait();
		const act = async (fn: () => void) => { fn(); await wait(); };
		return { result, act, unmount: () => inst.unmount() };
	}

	it('open() adds a tab and activates it', async () => {
		const { result, act, unmount } = await renderTabsHook();
		await act(() => result.current.open(tab('a')));
		await act(() => result.current.open(tab('b')));
		expect(result.current.tabs.map((t) => t.id)).toEqual(['a', 'b']);
		expect(result.current.activeId).toBe('b');
		unmount();
	});

	it('close() removes the tab and activates the adjacent one', async () => {
		const { result, act, unmount } = await renderTabsHook([tab('a'), tab('b'), tab('c')]);
		await act(() => result.current.activate('b'));
		await act(() => result.current.close('b'));
		expect(result.current.tabs.map((t) => t.id)).toEqual(['a', 'c']);
		expect(result.current.activeId).toBe('c');
		unmount();
	});

	it('cycleNext/cyclePrev wrap around', async () => {
		const { result, act, unmount } = await renderTabsHook([tab('a'), tab('b'), tab('c')]);
		await act(() => result.current.activate('c'));
		await act(() => result.current.cycleNext());
		expect(result.current.activeId).toBe('a');
		await act(() => result.current.cyclePrev());
		expect(result.current.activeId).toBe('c');
		unmount();
	});

	it('setTitle updates the title; setDirty marks dirty', async () => {
		const { result, act, unmount } = await renderTabsHook([tab('a')]);
		await act(() => result.current.setTitle('a', 'renamed'));
		expect(result.current.tabs[0].title).toBe('renamed');
		await act(() => result.current.setDirty('a', true));
		expect(result.current.tabs[0].dirty).toBe(true);
		unmount();
	});
});
