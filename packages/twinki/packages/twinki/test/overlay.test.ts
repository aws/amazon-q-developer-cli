/**
 * useOverlay hook tests
 *
 * Tests overlay positioning, show/hide, and that overlay content
 * appears in the correct viewport position.
 */
import { describe, it, expect } from 'vitest';
import React from 'react';
import { TestTerminal, wait } from './helpers.js';
import { render, Text, Box } from '../src/index.js';
import { useOverlay } from '../src/hooks/useOverlay.js';

// ── Helper ────────────────────────────────────────────────────────────────────

async function renderAndCapture(element: React.ReactElement, cols = 40, rows = 10) {
	const term = new TestTerminal(cols, rows);
	const instance = render(element, { terminal: term });
	await wait();
	await term.flush();
	return { term, instance };
}

// ── Tests ─────────────────────────────────────────────────────────────────────

describe('useOverlay', () => {
	it('shows overlay content in viewport', async () => {
		function Test() {
			const show = useOverlay(
				() => React.createElement(Box, { borderStyle: 'round' },
					React.createElement(Text, null, 'hello overlay')),
				{ anchor: 'center' },
			);
			React.useEffect(() => { show(); }, []);
			return React.createElement(Text, null, 'base');
		}
		const { term, instance } = await renderAndCapture(React.createElement(Test));
		const viewport = term.getViewport().join('\n');
		expect(viewport).toContain('hello overlay');
		instance.unmount();
	});

	it('hides overlay when handle.hide() called', async () => {
		let hideRef: (() => void) | null = null;
		function Test() {
			const show = useOverlay(
				() => React.createElement(Text, null, 'hidden-content'),
				{ anchor: 'center' },
			);
			React.useEffect(() => {
				const h = show();
				hideRef = () => h.hide();
			}, []);
			return React.createElement(Text, null, 'base');
		}
		const { term, instance } = await renderAndCapture(React.createElement(Test));
		// Before hide — content visible
		expect(term.getViewport().join('\n')).toContain('hidden-content');
		// After hide
		hideRef?.();
		await wait();
		await term.flush();
		expect(term.getViewport().join('\n')).not.toContain('hidden-content');
		instance.unmount();
	});

	it('overlay at top-left anchor appears near row 0', async () => {
		function Test() {
			const show = useOverlay(
				() => React.createElement(Text, null, 'topleft'),
				{ anchor: 'top-left' },
			);
			React.useEffect(() => { show(); }, []);
			return React.createElement(Text, null, 'base');
		}
		const { term, instance } = await renderAndCapture(React.createElement(Test));
		const viewport = term.getViewport();
		const found = viewport.slice(0, 3).some(l => l.includes('topleft'));
		expect(found).toBe(true);
		instance.unmount();
	});

	it('overlay at bottom-right anchor appears near last rows', async () => {
		function Test() {
			const show = useOverlay(
				() => React.createElement(Text, null, 'botright'),
				{ anchor: 'bottom-right' },
			);
			React.useEffect(() => { show(); }, []);
			return React.createElement(Text, null, 'base');
		}
		const { term, instance } = await renderAndCapture(React.createElement(Test), 40, 10);
		const viewport = term.getViewport();
		const found = viewport.slice(-4).some(l => l.includes('botright'));
		expect(found).toBe(true);
		instance.unmount();
	});
});
