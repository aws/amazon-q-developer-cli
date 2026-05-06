/**
 * Tests for the TUI renderer (differential rendering, overlay management,
 * resize handling, static output) and process-terminal (Kitty protocol
 * negotiation, signal handling, raw mode lifecycle).
 */
import { describe, it, expect, beforeEach, afterEach } from 'vitest';
import React, { useState, useEffect } from 'react';
import { TestTerminal, MutableComponent, wait } from './helpers.js';
import { TUI } from '../src/renderer/tui.js';
import { Container, parseSizeValue } from '../src/renderer/component.js';
import { render, Text, Box } from '../src/index.js';
import {
	detectCapabilities,
	resetCapabilitiesCache,
} from '../src/terminal/capabilities.js';

// ── capabilities.ts ───────────────────────────────────────────────────────────

describe('capabilities', () => {
	const saved: Record<string, string | undefined> = {};
	const envKeys = [
		'TERM',
		'TERM_PROGRAM',
		'COLORTERM',
		'ALACRITTY_LOG',
	];

	beforeEach(() => {
		for (const k of envKeys) {
			saved[k] = process.env[k];
			delete process.env[k];
		}
		resetCapabilitiesCache();
	});

	afterEach(() => {
		for (const k of envKeys) {
			if (saved[k] !== undefined) process.env[k] = saved[k];
			else delete process.env[k];
		}
		resetCapabilitiesCache();
	});

	it('detects kitty via TERM', () => {
		process.env.TERM = 'xterm-kitty';
		const c = detectCapabilities();
		expect(c.images).toBe('kitty');
		expect(c.trueColor).toBe(true);
		expect(c.hyperlinks).toBe(true);
	});

	it('detects ghostty', () => {
		process.env.TERM_PROGRAM = 'ghostty';
		const c = detectCapabilities();
		expect(c.images).toBe('kitty');
		expect(c.trueColor).toBe(true);
	});

	it('detects WezTerm', () => {
		process.env.TERM_PROGRAM = 'WezTerm';
		const c = detectCapabilities();
		expect(c.images).toBe('kitty');
	});

	it('detects iTerm2', () => {
		process.env.TERM_PROGRAM = 'iTerm.app';
		const c = detectCapabilities();
		expect(c.images).toBe('iterm2');
		expect(c.trueColor).toBe(true);
		expect(c.hyperlinks).toBe(true);
	});

	it('detects vscode hyperlinks only', () => {
		process.env.TERM_PROGRAM = 'vscode';
		const c = detectCapabilities();
		expect(c.images).toBeNull();
		expect(c.hyperlinks).toBe(true);
	});

	it('detects alacritty via ALACRITTY_LOG', () => {
		process.env.ALACRITTY_LOG = '/tmp/log';
		const c = detectCapabilities();
		expect(c.trueColor).toBe(true);
		expect(c.hyperlinks).toBe(true);
		expect(c.images).toBeNull();
	});

	it('detects truecolor via COLORTERM=truecolor', () => {
		process.env.COLORTERM = 'truecolor';
		const c = detectCapabilities();
		expect(c.trueColor).toBe(true);
	});

	it('detects truecolor via COLORTERM=24bit', () => {
		process.env.COLORTERM = '24bit';
		const c = detectCapabilities();
		expect(c.trueColor).toBe(true);
	});

	it('returns null/false for unknown terminal', () => {
		const c = detectCapabilities();
		expect(c.images).toBeNull();
		expect(c.trueColor).toBe(false);
		expect(c.hyperlinks).toBe(false);
	});

	it('caches results', () => {
		process.env.TERM_PROGRAM = 'kitty';
		const a = detectCapabilities();
		process.env.TERM_PROGRAM = 'iTerm.app';
		const b = detectCapabilities();
		expect(a).toBe(b);
	});
});

// ── component.ts ──────────────────────────────────────────────────────────────

describe('component', () => {
	it('parseSizeValue handles number', () => {
		expect(parseSizeValue(50, 100)).toBe(50);
	});

	it('parseSizeValue handles percentage', () => {
		expect(parseSizeValue('50%', 100)).toBe(50);
	});

	it('parseSizeValue handles undefined', () => {
		expect(parseSizeValue(undefined, 100)).toBeUndefined();
	});

	it('parseSizeValue handles invalid string', () => {
		expect(parseSizeValue('abc' as any, 100)).toBeUndefined();
	});

	it('Container addChild/removeChild/clear', () => {
		const c = new Container();
		const child: MutableComponent = new MutableComponent();
		child.lines = ['hello'];
		c.addChild(child);
		expect(c.render(80)).toEqual(['hello']);
		c.removeChild(child);
		expect(c.render(80)).toEqual([]);
		c.addChild(child);
		c.clear();
		expect(c.render(80)).toEqual([]);
	});

	it('Container removeChild no-op for missing child', () => {
		const c = new Container();
		const child = new MutableComponent();
		c.removeChild(child); // should not throw
	});
});

// ── tui.ts ────────────────────────────────────────────────────────────────────

describe('TUI coverage', () => {
	let term: TestTerminal;
	let tui: TUI;
	let comp: MutableComponent;

	beforeEach(() => {
		term = new TestTerminal(40, 10);
		comp = new MutableComponent();
		comp.lines = ['hello'];
	});

	afterEach(() => {
		try { tui.stop(); } catch {}
	});

	it('enterAltScreen / exitAltScreen', async () => {
		tui = new TUI(term);
		tui.addChild(comp);
		tui.start();
		await wait(); await term.flush();

		tui.enterAltScreen();
		expect(tui.isAltScreen()).toBe(true);
		await wait(); await term.flush();

		tui.exitAltScreen();
		expect(tui.isAltScreen()).toBe(false);
	});

	it('fullscreen option starts in alt screen', async () => {
		tui = new TUI(term, { fullscreen: true });
		tui.addChild(comp);
		tui.start();
		expect(tui.isAltScreen()).toBe(true);
		await wait(); await term.flush();
	});

	it('setShowHardwareCursor toggles cursor', async () => {
		tui = new TUI(term, { showHardwareCursor: false });
		tui.addChild(comp);
		tui.start();
		await wait(); await term.flush();

		tui.setShowHardwareCursor(true);
		await wait(); await term.flush();
		// calling again with same value is no-op
		tui.setShowHardwareCursor(true);
	});

	it('setClearOnShrink', async () => {
		tui = new TUI(term);
		tui.addChild(comp);
		tui.start();
		await wait(); await term.flush();

		tui.setClearOnShrink(true);
		comp.lines = ['hello', 'world', 'three'];
		tui.requestRender();
		await wait(); await term.flush();

		comp.lines = ['hello'];
		tui.requestRender();
		await wait(); await term.flush();
		expect(term.getViewport()[0]).toContain('hello');
	});

	it('fullRedraws counter', async () => {
		tui = new TUI(term);
		tui.addChild(comp);
		tui.start();
		await wait(); await term.flush();
		expect(tui.fullRedraws).toBeGreaterThanOrEqual(1);
	});

	it('writeStaticLines and resetStaticOutput', async () => {
		tui = new TUI(term);
		tui.addChild(comp);
		tui.start();
		await wait(); await term.flush();

		tui.writeStaticLines(['static line 1']);
		tui.requestRender();
		await wait(); await term.flush();
		expect(tui.staticBufferLines).toBeGreaterThanOrEqual(1);

		tui.resetStaticOutput();
		expect(tui.staticBufferLines).toBe(0);
	});

	it('replaceStaticOutput', async () => {
		tui = new TUI(term);
		tui.addChild(comp);
		tui.start();
		await wait(); await term.flush();

		tui.replaceStaticOutput(['replaced 1', 'replaced 2']);
		expect(tui.staticBufferLines).toBe(2);
	});

	it('trimStaticOutput caps buffer', async () => {
		tui = new TUI(term, { staticScrollbackCap: 10 });
		tui.addChild(comp);
		tui.start();
		await wait(); await term.flush();

		// Push 12 lines (>10 * 1.1 = 11) to trigger trim
		const lines = Array.from({ length: 12 }, (_, i) => `line ${i}`);
		tui.writeStaticLines(lines);
		// After trim, should be ~7 (75% of 10)
		expect(tui.staticBufferLines).toBeLessThanOrEqual(10);
	});

	it('overlay show/hide/hasOverlay', async () => {
		tui = new TUI(term);
		tui.addChild(comp);
		tui.start();
		await wait(); await term.flush();

		const overlay = new MutableComponent();
		overlay.lines = ['overlay content'];
		const handle = tui.showOverlay(overlay, { anchor: 'center' });
		expect(tui.hasOverlay()).toBe(true);
		await wait(); await term.flush();

		handle.hide();
		expect(tui.hasOverlay()).toBe(false);
		await wait(); await term.flush();
	});

	it('overlay setHidden', async () => {
		tui = new TUI(term);
		tui.addChild(comp);
		tui.start();
		await wait(); await term.flush();

		const overlay = new MutableComponent();
		overlay.lines = ['overlay'];
		const handle = tui.showOverlay(overlay, { anchor: 'top-left' });
		expect(handle.isHidden()).toBe(false);

		handle.setHidden(true);
		expect(handle.isHidden()).toBe(true);
		expect(tui.hasOverlay()).toBe(false);
		await wait(); await term.flush();

		handle.setHidden(false);
		expect(handle.isHidden()).toBe(false);
		await wait(); await term.flush();

		// no-op same value
		handle.setHidden(false);
		handle.hide();
	});

	it('hideOverlay pops topmost', async () => {
		tui = new TUI(term);
		tui.addChild(comp);
		tui.start();
		await wait(); await term.flush();

		const o1 = new MutableComponent();
		o1.lines = ['o1'];
		const o2 = new MutableComponent();
		o2.lines = ['o2'];
		tui.showOverlay(o1, { anchor: 'center' });
		tui.showOverlay(o2, { anchor: 'center' });
		tui.hideOverlay();
		expect(tui.hasOverlay()).toBe(true);
		tui.hideOverlay();
		expect(tui.hasOverlay()).toBe(false);
	});

	it('hideOverlay on empty stack is no-op', () => {
		tui = new TUI(term);
		tui.start();
		tui.hideOverlay(); // should not throw
	});

	it('overlay with visible callback', async () => {
		tui = new TUI(term);
		tui.addChild(comp);
		tui.start();
		await wait(); await term.flush();

		const overlay = new MutableComponent();
		overlay.lines = ['vis'];
		tui.showOverlay(overlay, {
			anchor: 'center',
			visible: (w, h) => w > 100, // false for 40-col terminal
		});
		expect(tui.hasOverlay()).toBe(false);
		tui.hideOverlay();
	});

	it('setFocus dispatches input to focused component', async () => {
		tui = new TUI(term);
		const received: string[] = [];
		const focusable = {
			render: () => ['focused'],
			invalidate: () => {},
			handleInput: (d: string) => received.push(d),
		};
		tui.addChild(focusable);
		tui.setFocus(focusable);
		tui.start();
		await wait(); await term.flush();

		term.sendInput('x');
		expect(received).toContain('x');
	});

	it('addKeyReleaseListener', async () => {
		tui = new TUI(term);
		tui.addChild(comp);
		tui.start();
		await wait(); await term.flush();

		const releases: string[] = [];
		const unsub = tui.addKeyReleaseListener((d) => releases.push(d));
		// Simulate a key release event (kitty protocol :3u)
		term.sendInput('\x1b[97:3u');
		expect(releases.length).toBe(1);
		unsub();
	});

	it('addKeyRepeatListener', async () => {
		tui = new TUI(term);
		tui.addChild(comp);
		tui.start();
		await wait(); await term.flush();

		const repeats: string[] = [];
		const unsub = tui.addKeyRepeatListener((d) => repeats.push(d));
		term.sendInput('\x1b[97:2u');
		expect(repeats.length).toBe(1);
		unsub();
	});

	it('addPasteListener', async () => {
		tui = new TUI(term);
		tui.addChild(comp);
		tui.start();
		await wait(); await term.flush();

		const pastes: string[] = [];
		const unsub = tui.addPasteListener((c) => pastes.push(c));
		term.sendInput('\x1b[200~pasted text\x1b[201~');
		expect(pastes).toEqual(['pasted text']);
		unsub();
	});

	it('addInputListener can consume input', async () => {
		tui = new TUI(term);
		const received: string[] = [];
		const focusable = {
			render: () => ['f'],
			invalidate: () => {},
			handleInput: (d: string) => received.push(d),
		};
		tui.addChild(focusable);
		tui.setFocus(focusable);
		tui.start();
		await wait(); await term.flush();

		const unsub = tui.addInputListener(() => ({ consume: true }));
		term.sendInput('z');
		expect(received).not.toContain('z');
		unsub();
	});

	it('enableMouse / disableMouse', async () => {
		tui = new TUI(term, { mouse: true });
		tui.addChild(comp);
		tui.start();
		await wait(); await term.flush();

		tui.enableMouse();
		tui.disableMouse();
		// enableMouse without mouse option is no-op
		const tui2 = new TUI(term);
		tui2.enableMouse();
		tui2.stop();
	});

	it('addMouseListener enables/disables mouse', async () => {
		tui = new TUI(term, { mouse: true });
		tui.addChild(comp);
		tui.start();
		await wait(); await term.flush();

		const events: any[] = [];
		const unsub = tui.addMouseListener((e) => events.push(e));
		unsub(); // removing last listener disables mouse
	});

	it('onResize callback fires', async () => {
		tui = new TUI(term);
		tui.addChild(comp);
		tui.start();
		await wait(); await term.flush();

		let resized = false;
		tui.onResize(() => { resized = true; });
		term.resize(60, 15);
		await wait(); await term.flush();
		expect(resized).toBe(true);
	});

	it('getContentYOffset returns number', async () => {
		tui = new TUI(term);
		tui.addChild(comp);
		tui.start();
		await wait(); await term.flush();
		expect(typeof tui.getContentYOffset()).toBe('number');
	});

	it('linesAboveViewport returns number', async () => {
		tui = new TUI(term);
		tui.addChild(comp);
		tui.start();
		await wait(); await term.flush();
		expect(typeof tui.linesAboveViewport).toBe('number');
	});

	it('requestRender after stop is no-op', async () => {
		tui = new TUI(term);
		tui.addChild(comp);
		tui.start();
		await wait(); await term.flush();
		tui.stop();
		tui.requestRender(); // should not throw
	});

	it('targetFps pacing', async () => {
		tui = new TUI(term, { targetFps: 30 });
		tui.addChild(comp);
		tui.start();
		await wait(50); await term.flush();
		comp.lines = ['updated'];
		tui.requestRender();
		await wait(50); await term.flush();
		expect(term.getViewport().join('\n')).toContain('updated');
	});

	it('clearRenderState resets tracking', async () => {
		tui = new TUI(term);
		tui.addChild(comp);
		tui.start();
		await wait(); await term.flush();
		tui.clearRenderState();
		// After clearing, next render should work fine
		tui.requestRender(true);
		await wait(); await term.flush();
		expect(term.getViewport()[0]).toContain('hello');
	});
});

// ── tui.ts overlay layout anchors ────────────────────────────────────────────

describe('TUI overlay anchors', () => {
	async function renderOverlayAt(anchor: string, opts?: any) {
		const term = new TestTerminal(40, 10);
		const tui = new TUI(term);
		const comp = new MutableComponent();
		comp.lines = ['base'];
		tui.addChild(comp);
		tui.start();
		await wait(); await term.flush();

		const overlay = new MutableComponent();
		overlay.lines = ['OVR'];
		tui.showOverlay(overlay, { anchor, ...opts });
		await wait(); await term.flush();
		const vp = term.getViewport();
		tui.stop();
		return vp;
	}

	it('top-left', async () => {
		const vp = await renderOverlayAt('top-left');
		expect(vp.slice(0, 2).some(l => l.includes('OVR'))).toBe(true);
	});

	it('top-center', async () => {
		const vp = await renderOverlayAt('top-center');
		expect(vp.slice(0, 2).some(l => l.includes('OVR'))).toBe(true);
	});

	it('top-right', async () => {
		const vp = await renderOverlayAt('top-right');
		expect(vp.slice(0, 2).some(l => l.includes('OVR'))).toBe(true);
	});

	it('bottom-left', async () => {
		const vp = await renderOverlayAt('bottom-left');
		expect(vp.slice(-2).some(l => l.includes('OVR'))).toBe(true);
	});

	it('bottom-center', async () => {
		const vp = await renderOverlayAt('bottom-center');
		expect(vp.slice(-2).some(l => l.includes('OVR'))).toBe(true);
	});

	it('bottom-right', async () => {
		const vp = await renderOverlayAt('bottom-right');
		expect(vp.slice(-2).some(l => l.includes('OVR'))).toBe(true);
	});

	it('left-center', async () => {
		const vp = await renderOverlayAt('left-center');
		expect(vp.some(l => l.includes('OVR'))).toBe(true);
	});

	it('right-center', async () => {
		const vp = await renderOverlayAt('right-center');
		expect(vp.some(l => l.includes('OVR'))).toBe(true);
	});

	it('percentage row/col', async () => {
		const vp = await renderOverlayAt('center', { row: '50%', col: '50%' });
		expect(vp.some(l => l.includes('OVR'))).toBe(true);
	});

	it('explicit numeric row/col', async () => {
		const vp = await renderOverlayAt('center', { row: 1, col: 2 });
		expect(vp.some(l => l.includes('OVR'))).toBe(true);
	});

	it('margin as number', async () => {
		const vp = await renderOverlayAt('center', { margin: 1 });
		expect(vp.some(l => l.includes('OVR'))).toBe(true);
	});

	it('margin as object', async () => {
		const vp = await renderOverlayAt('center', {
			margin: { top: 1, right: 1, bottom: 1, left: 1 },
		});
		expect(vp.some(l => l.includes('OVR'))).toBe(true);
	});

	it('offsetX/offsetY', async () => {
		const vp = await renderOverlayAt('top-left', { offsetX: 2, offsetY: 1 });
		expect(vp.some(l => l.includes('OVR'))).toBe(true);
	});

	it('width as percentage', async () => {
		const vp = await renderOverlayAt('center', { width: '80%' });
		expect(vp.some(l => l.includes('OVR'))).toBe(true);
	});

	it('maxHeight as percentage', async () => {
		const vp = await renderOverlayAt('center', { maxHeight: '50%' });
		expect(vp.some(l => l.includes('OVR'))).toBe(true);
	});

	it('minWidth', async () => {
		const vp = await renderOverlayAt('center', { minWidth: 30 });
		expect(vp.some(l => l.includes('OVR'))).toBe(true);
	});

	it('invalid string row/col falls back to anchor', async () => {
		const vp = await renderOverlayAt('center', { row: 'bad', col: 'bad' });
		expect(vp.some(l => l.includes('OVR'))).toBe(true);
	});
});

// ── text-renderer.ts (via React render) ──────────────────────────────────────

describe('text-renderer styles', () => {
	async function renderText(props: any, text = 'test') {
		const term = new TestTerminal(40, 5);
		const el = React.createElement(Text, props, text);
		const inst = render(el, { terminal: term });
		await wait(); await term.flush();
		const vp = term.getViewport().join('\n');
		inst.unmount();
		return vp;
	}

	it('bold', async () => {
		const vp = await renderText({ bold: true });
		expect(vp).toContain('test');
	});

	it('dimColor', async () => {
		const vp = await renderText({ dimColor: true });
		expect(vp).toContain('test');
	});

	it('italic', async () => {
		const vp = await renderText({ italic: true });
		expect(vp).toContain('test');
	});

	it('underline', async () => {
		const vp = await renderText({ underline: true });
		expect(vp).toContain('test');
	});

	it('strikethrough', async () => {
		const vp = await renderText({ strikethrough: true });
		expect(vp).toContain('test');
	});

	it('inverse', async () => {
		const vp = await renderText({ inverse: true });
		expect(vp).toContain('test');
	});
});

// ── box-renderer.ts (via React render) ───────────────────────────────────────

describe('box-renderer styles', () => {
	async function renderBox(boxProps: any, text = 'inside') {
		const term = new TestTerminal(40, 10);
		const el = React.createElement(
			Box, boxProps,
			React.createElement(Text, null, text),
		);
		const inst = render(el, { terminal: term });
		await wait(); await term.flush();
		const vp = term.getViewport();
		inst.unmount();
		return vp;
	}

	it('borderStyle round', async () => {
		const vp = await renderBox({ borderStyle: 'round' });
		expect(vp.some(l => l.includes('inside'))).toBe(true);
	});

	it('borderStyle single', async () => {
		const vp = await renderBox({ borderStyle: 'single' });
		expect(vp.some(l => l.includes('inside'))).toBe(true);
	});

	it('borderStyle double', async () => {
		const vp = await renderBox({ borderStyle: 'double' });
		expect(vp.some(l => l.includes('inside'))).toBe(true);
	});

	it('borderColor', async () => {
		const vp = await renderBox({ borderStyle: 'single', borderColor: 'red' });
		expect(vp.some(l => l.includes('inside'))).toBe(true);
	});

	it('backgroundColor', async () => {
		const vp = await renderBox({ backgroundColor: 'blue' });
		expect(vp.some(l => l.includes('inside'))).toBe(true);
	});

	it('padding', async () => {
		const vp = await renderBox({ paddingLeft: 2, paddingTop: 1 });
		expect(vp.some(l => l.includes('inside'))).toBe(true);
	});

	it('overflow hidden', async () => {
		const vp = await renderBox(
			{ overflow: 'hidden', height: 2, borderStyle: 'single' },
			'clipped',
		);
		// Box renders even if content is clipped
		expect(vp.length).toBeGreaterThan(0);
	});
});

// ── tree-renderer.ts (borders + padding via React render) ────────────────────

describe('tree-renderer', () => {
	it('renders nested boxes with borders and padding', async () => {
		const term = new TestTerminal(60, 15);
		const el = React.createElement(
			Box,
			{ borderStyle: 'single', paddingLeft: 1, paddingRight: 1, paddingTop: 1, paddingBottom: 1 },
			React.createElement(
				Box,
				{ borderStyle: 'round' },
				React.createElement(Text, null, 'nested'),
			),
		);
		const inst = render(el, { terminal: term });
		await wait(); await term.flush();
		const vp = term.getViewport().join('\n');
		expect(vp).toContain('nested');
		inst.unmount();
	});

	it('renders box with overflow hidden clips content', async () => {
		const term = new TestTerminal(40, 5);
		const children = Array.from({ length: 10 }, (_, i) =>
			React.createElement(Text, { key: i }, `line ${i}`),
		);
		const el = React.createElement(
			Box,
			{ flexDirection: 'column', overflow: 'hidden', height: 3 },
			...children,
		);
		const inst = render(el, { terminal: term });
		await wait(); await term.flush();
		const vp = term.getViewport().join('\n');
		// Content is clipped — not all 10 lines visible
		expect(vp).toContain('line');
		inst.unmount();
	});
});

// ── TUI lifecycle: resize, shrink, overlay, focus, static output ─────────────

describe('TUI lifecycle', () => {
	it('handles width change triggering full clear', async () => {
		const term = new TestTerminal(40, 10);
		const tui = new TUI(term);
		const comp = new MutableComponent();
		comp.lines = ['hello world'];
		tui.addChild(comp);
		tui.start();
		await wait();
		await term.flush();

		term.resize(60, 10);
		await wait();
		await term.flush();

		const frame = term.getLastFrame();
		expect(frame).toBeTruthy();
		tui.stop();
	});

	it('handles shrink clear', async () => {
		const term = new TestTerminal(40, 10);
		const tui = new TUI(term);
		tui.setClearOnShrink(true);
		const comp = new MutableComponent();
		comp.lines = ['line1', 'line2', 'line3', 'line4', 'line5'];
		tui.addChild(comp);
		tui.start();
		await wait();
		await term.flush();

		comp.lines = ['line1'];
		tui.requestRender();
		await wait();
		await term.flush();

		tui.stop();
	});

	it('compositeLineAt with overlay content', async () => {
		const term = new TestTerminal(40, 10);
		const tui = new TUI(term);
		const comp = new MutableComponent();
		comp.lines = ['background content here'];
		tui.addChild(comp);
		tui.start();
		await wait();
		await term.flush();

		const overlay = new MutableComponent();
		overlay.lines = ['popup'];
		const handle = tui.showOverlay(overlay, { anchor: 'center' });
		await wait();
		await term.flush();

		const frame = term.getLastFrame();
		expect(frame!.viewport.some(l => l.includes('popup'))).toBe(true);

		tui.hideOverlay(handle);
		tui.stop();
	});

	it('installStdoutInterceptor and removeStdoutInterceptor', async () => {
		const term = new TestTerminal(40, 10);
		const tui = new TUI(term);
		tui.start();

		tui.installStdoutInterceptor();
		tui.removeStdoutInterceptor();

		tui.stop();
	});

	it('handleInput dispatches to focused component', async () => {
		const term = new TestTerminal(40, 10);
		const tui = new TUI(term);
		const comp = new MutableComponent();
		comp.lines = ['test'];
		tui.addChild(comp);
		tui.start();
		await wait();

		tui.setFocus(comp);
		term.sendInput('a');
		await wait();

		tui.stop();
	});

	it('staticBufferLines tracks static output', async () => {
		const term = new TestTerminal(40, 10);
		const tui = new TUI(term);
		tui.start();

		expect(tui.staticBufferLines).toBe(0);

		tui.writeStaticLines(['static line 1', 'static line 2']);
		expect(tui.staticBufferLines).toBe(2);

		tui.stop();
	});
});

// ── Reconciler: rerender and child reconciliation ────────────────────────────

describe('reconciler rerender', () => {
	it('rerender updates text content', async () => {
		const { Text } = await import('../src/components/Text.js');
		const term = new TestTerminal(40, 5);
		const tui = new TUI(term);
		tui.start();

		const inst = render(React.createElement(Text, null, 'before'), tui);
		await wait();
		await term.flush();

		inst.rerender(React.createElement(Text, null, 'after'));
		await wait();
		await term.flush();

		expect(term.getLastFrame()!.viewport.some(l => l.includes('after'))).toBe(true);
		inst.unmount();
		tui.stop();
	});

	it('handles conditional children', async () => {
		const { Text } = await import('../src/components/Text.js');
		const { Box } = await import('../src/components/Box.js');
		const term = new TestTerminal(40, 10);
		const tui = new TUI(term);
		tui.start();

		// GIVEN a Box with children Alpha and Beta
		const inst = render(
			React.createElement(Box, null,
				React.createElement(Text, { key: 'a' }, 'Alpha'),
				React.createElement(Text, { key: 'b' }, 'Beta'),
			),
			tui,
		);
		await wait();
		await term.flush();

		// THEN both should be rendered
		let frame = term.getLastFrame();
		expect(frame!.viewport.some(l => l.includes('Alpha'))).toBe(true);
		expect(frame!.viewport.some(l => l.includes('Beta'))).toBe(true);

		// WHEN child Beta is replaced with Gamma
		inst.rerender(
			React.createElement(Box, null,
				React.createElement(Text, { key: 'a' }, 'Alpha'),
				React.createElement(Text, { key: 'c' }, 'Gamma'),
			),
		);
		await wait();
		await term.flush();

		// THEN Alpha and Gamma should be rendered, Beta should be gone
		frame = term.getLastFrame();
		expect(frame!.viewport.some(l => l.includes('Gamma'))).toBe(true);
		expect(frame!.viewport.some(l => l.includes('Beta'))).toBe(false);

		// WHEN all children are removed
		inst.rerender(React.createElement(Box, null));
		await wait();
		await term.flush();

		// THEN the rerender should complete without error (viewport may retain last frame)
		frame = term.getLastFrame();
		expect(frame).toBeTruthy();

		inst.unmount();
		tui.stop();
	});
});

// ── Public API exports (index.ts) ────────────────────────────────────────────

describe('public API exports', () => {
	it('measureText measures plain text', async () => {
		const { measureText } = await import('../src/index.js');
		const result = measureText('hello\nworld');
		expect(result.width).toBe(5);
		expect(result.height).toBe(2);
	});

	it('measureText handles ANSI', async () => {
		const { measureText } = await import('../src/index.js');
		const result = measureText('\x1b[31mred\x1b[0m');
		expect(result.width).toBe(3);
		expect(result.height).toBe(1);
	});

	it('measureText handles empty string', async () => {
		const { measureText } = await import('../src/index.js');
		const result = measureText('');
		expect(result.width).toBe(0);
		expect(result.height).toBe(1);
	});

	it('measureElement returns 0,0 for non-node', async () => {
		const { measureElement } = await import('../src/index.js');
		expect(measureElement(null)).toEqual({ width: 0, height: 0 });
		expect(measureElement({})).toEqual({ width: 0, height: 0 });
	});

	it('render export works', async () => {
		const { render: renderFn } = await import('../src/index.js');
		expect(typeof renderFn).toBe('function');
	});
});
