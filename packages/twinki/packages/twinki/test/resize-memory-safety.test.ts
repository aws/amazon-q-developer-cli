/**
 * Integration test: Resize handler must not cause memory spiral on tmux attach.
 *
 * Background: When a TUI session runs in a detached tmux pane, stdout.write()
 * calls back up in the kernel PTY buffer. On tmux attach, SIGWINCH fires.
 * If the resize handler uses setTimeout (async), the callback fires inside
 * the event loop while backed-up writes are flushing, creating an unbounded
 * re-render cascade (~500MB/s until OOM).
 *
 * This test verifies that:
 * 1. The resize handler fires synchronously (no setTimeout)
 * 2. Rapid resize events with unchanged dimensions are coalesced (no-op)
 * 3. Memory stays bounded after resize during active rendering
 *
 * See: PR #2137, commit 533b343c8
 */

import { describe, it, expect, beforeEach, afterEach } from "vitest";
import { ProcessTerminal } from "../src/terminal/process-terminal";

describe("resize handler memory safety", () => {
	let terminal: ProcessTerminal;
	let resizeCount: number;
	let onResizeFn: () => void;

	beforeEach(() => {
		terminal = new ProcessTerminal();
		resizeCount = 0;
		onResizeFn = () => { resizeCount++; };
	});

	afterEach(() => {
		try { terminal.stop(); } catch { /* may not be started */ }
	});

	it("should NOT use setTimeout in resize handler", () => {
		// Inspect the source to ensure no setTimeout in resize path
		const src = ProcessTerminal.prototype.start.toString();
		const hasSetTimeout = src.includes("setTimeout");
		expect(hasSetTimeout).toBe(false);
	});

	it("should skip resize when dimensions unchanged", () => {
		// Start terminal to register resize handler
		terminal.start(() => {}, onResizeFn);

		// Get the resize handler
		const handler = (terminal as any).resizeHandler;
		expect(handler).toBeDefined();

		// First call — dimensions will match what's already stored
		// (stdout.columns/rows matches _columns/_rows from constructor)
		const initialCols = (terminal as any)._columns;
		const initialRows = (terminal as any)._rows;

		// Simulate resize with same dimensions — should be no-op
		handler();
		// If dimensions didn't change, resizeCount should be 0
		if (process.stdout.columns === initialCols && process.stdout.rows === initialRows) {
			expect(resizeCount).toBe(0);
		}
	});

	it("should fire onResize synchronously (not deferred)", () => {
		terminal.start(() => {}, onResizeFn);

		// Temporarily change dimensions to force a resize
		const origCols = (terminal as any)._columns;
		(terminal as any)._columns = 1; // Force mismatch

		const handler = (terminal as any).resizeHandler;
		handler();

		// onResize must have fired synchronously — resizeCount should be
		// incremented BEFORE we reach this line (no microtask/setTimeout delay)
		expect(resizeCount).toBeGreaterThan(0);

		// Restore
		(terminal as any)._columns = origCols;
	});

	it("should not grow memory during rapid resize events", () => {
		terminal.start(() => {}, onResizeFn);
		const handler = (terminal as any).resizeHandler;

		// Simulate 1000 rapid resize events (like scrollbar oscillation)
		const memBefore = process.memoryUsage().rss;

		for (let i = 0; i < 1000; i++) {
			// Alternate dimensions to trigger actual resizes
			(terminal as any)._columns = i % 2 === 0 ? 80 : 81;
			handler();
		}

		const memAfter = process.memoryUsage().rss;
		const growthMB = (memAfter - memBefore) / 1024 / 1024;

		// Memory growth should be minimal (< 50MB for 1000 resizes)
		// The old setTimeout debounce would accumulate timer callbacks
		expect(growthMB).toBeLessThan(50);
	});

	it("regression: setTimeout debounce causes spiral (documentation test)", () => {
		// This test documents WHY setTimeout is dangerous in the resize handler.
		// It does NOT actually reproduce the full spiral (that requires a real
		// detached PTY), but it verifies the mechanism.
		//
		// With setTimeout: onResize fires asynchronously, allowing event loop
		// interleaving with stdout backpressure → unbounded cascade.
		//
		// Without setTimeout: onResize fires synchronously in the signal handler,
		// completing before the event loop processes backed-up writes.
		//
		// The key invariant: after calling the resize handler, onResize must
		// have already been called (synchronous) — not scheduled for later.

		let resizeFired = false;
		terminal.start(() => {}, () => { resizeFired = true; });

		(terminal as any)._columns = 1; // Force mismatch
		const handler = (terminal as any).resizeHandler;
		handler();

		// CRITICAL: resizeFired must be true HERE, not after await/setTimeout
		expect(resizeFired).toBe(true);
	});
});
