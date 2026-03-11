import { describe, it, expect } from 'vitest';
import { TUI } from '../src/renderer/tui.js';
import type { Component } from '../src/renderer/component.js';
import {
	TestTerminal, MutableComponent, analyzeFlicker,
	wait, diffFrames, serializeFrame,
} from './helpers.js';

// ============================================================
// C1: Counter App
// ============================================================

describe('Sample App: Counter', () => {
	class CounterApp implements Component {
		count = 0;
		render() { return [`Count: ${this.count}`, '', 'Press any key to increment']; }
		handleInput() { this.count++; }
		invalidate() {}
	}

	it('increments on keypress', async () => {
		const term = new TestTerminal(40, 5);
		const tui = new TUI(term);
		const app = new CounterApp();
		tui.addChild(app);
		tui.setFocus(app);
		tui.start();

		await wait();
		await term.flush();
		expect(term.getLastFrame()!.viewport[0]).toContain('Count: 0');

		for (let i = 1; i <= 5; i++) {
			term.sendInput('x');
			await wait();
			await term.flush();
			expect(term.getLastFrame()!.viewport[0]).toContain(`Count: ${i}`);
		}

		tui.stop();
	});

	it('only the counter line changes (differential)', async () => {
		const term = new TestTerminal(40, 5);
		const tui = new TUI(term);
		const app = new CounterApp();
		tui.addChild(app);
		tui.setFocus(app);
		tui.start();

		await wait();
		await term.flush();

		term.sendInput('x');
		await wait();
		await term.flush();

		const frames = term.getFrames();
		const diff = diffFrames(frames[0]!, frames[1]!);

		// Only row 0 should change (Count: 0 → Count: 1)
		expect(diff.length).toBe(1);
		expect(diff[0]).toContain('row 0');

		tui.stop();
	});

	it('zero flicker across 20 increments', async () => {
		const term = new TestTerminal(40, 5);
		const tui = new TUI(term);
		const app = new CounterApp();
		tui.addChild(app);
		tui.setFocus(app);
		tui.start();

		await wait();
		await term.flush();

		for (let i = 0; i < 20; i++) {
			term.sendInput('x');
			await wait();
			await term.flush();
		}

		const report = analyzeFlicker(term.getFrames());
		expect(report.clean).toBe(true);

		tui.stop();
	});
});

// ============================================================
// C2: Spinner App
// ============================================================

describe('Sample App: Spinner', () => {
	const SPINNER = ['⠋', '⠙', '⠹', '⠸', '⠼', '⠴', '⠦', '⠧', '⠇', '⠏'];

	class SpinnerApp implements Component {
		frame = 0;
		message = 'Installing dependencies...';
		render() {
			return [
				`${SPINNER[this.frame % SPINNER.length]} ${this.message}`,
			];
		}
		tick() { this.frame++; }
		invalidate() {}
	}

	it('cycles through spinner characters', async () => {
		const term = new TestTerminal(40, 3);
		const tui = new TUI(term);
		const app = new SpinnerApp();
		tui.addChild(app);
		tui.start();

		await wait();
		await term.flush();

		for (let i = 0; i < 10; i++) {
			app.tick();
			tui.requestRender();
			await wait();
			await term.flush();
		}

		const frames = term.getFrames();
		expect(frames.length).toBe(11); // initial + 10 ticks

		// Verify each frame shows the correct spinner char
		for (let i = 0; i < 11; i++) {
			const expected = SPINNER[i % SPINNER.length]!;
			expect(frames[i]!.viewport[0]).toContain(expected);
		}

		tui.stop();
	});

	it('zero flicker across 30 frames', async () => {
		const term = new TestTerminal(40, 3);
		const tui = new TUI(term);
		const app = new SpinnerApp();
		tui.addChild(app);
		tui.start();

		await wait();
		await term.flush();

		for (let i = 0; i < 30; i++) {
			app.tick();
			tui.requestRender();
			await wait();
			await term.flush();
		}

		const report = analyzeFlicker(term.getFrames());
		expect(report.clean).toBe(true);

		tui.stop();
	});

	it('all updates are differential (not full)', async () => {
		const term = new TestTerminal(40, 3);
		const tui = new TUI(term);
		const app = new SpinnerApp();
		tui.addChild(app);
		tui.start();

		await wait();
		await term.flush();

		for (let i = 0; i < 5; i++) {
			app.tick();
			tui.requestRender();
			await wait();
			await term.flush();
		}

		const frames = term.getFrames();
		for (let i = 1; i < frames.length; i++) {
			expect(frames[i]!.isFull).toBe(false);
		}

		tui.stop();
	});
});

// ============================================================
// C3: Multi-line App (header/body/footer)
// ============================================================

describe('Sample App: Multi-line Layout', () => {
	class DashboardApp implements Component {
		header = '=== Dashboard ===';
		body = 'Status: OK';
		footer = '[q] quit  [r] refresh';
		render() {
			return [this.header, '', this.body, '', this.footer];
		}
		invalidate() {}
	}

	it('changing body does not rewrite header/footer', async () => {
		const term = new TestTerminal(40, 8);
		const tui = new TUI(term);
		const app = new DashboardApp();
		tui.addChild(app);
		tui.start();

		await wait();
		await term.flush();

		app.body = 'Status: DEGRADED';
		tui.requestRender();
		await wait();
		await term.flush();

		const frames = term.getFrames();
		const diff = diffFrames(frames[0]!, frames[1]!);

		// Only the body row (row 2) should change
		expect(diff.length).toBe(1);
		expect(diff[0]).toContain('row 2');
		expect(diff[0]).toContain('DEGRADED');

		tui.stop();
	});

	it('differential update bytes are small', async () => {
		const term = new TestTerminal(40, 8);
		const tui = new TUI(term);
		const app = new DashboardApp();
		tui.addChild(app);
		tui.start();

		await wait();
		await term.flush();

		app.body = 'Status: ERROR';
		tui.requestRender();
		await wait();
		await term.flush();

		const frames = term.getFrames();
		const firstBytes = frames[0]!.writeBytes;
		const diffBytes = frames[1]!.writeBytes;

		// Differential should be significantly smaller than first render
		expect(diffBytes).toBeLessThan(firstBytes);

		tui.stop();
	});

	it('multiple body changes, header/footer never touched', async () => {
		const term = new TestTerminal(40, 8);
		const tui = new TUI(term);
		const app = new DashboardApp();
		tui.addChild(app);
		tui.start();

		await wait();
		await term.flush();

		const statuses = ['OK', 'DEGRADED', 'ERROR', 'RECOVERING', 'OK'];
		for (const status of statuses) {
			app.body = `Status: ${status}`;
			tui.requestRender();
			await wait();
			await term.flush();
		}

		const frames = term.getFrames();
		// Check every transition only changes row 2
		for (let i = 1; i < frames.length; i++) {
			const diff = diffFrames(frames[i - 1]!, frames[i]!);
			expect(diff.every(d => d.includes('row 2'))).toBe(true);
		}

		tui.stop();
	});
});

// ============================================================
// C4: Input Echo App
// ============================================================

describe('Sample App: Input Echo', () => {
	class InputEchoApp implements Component {
		lastKey = '(none)';
		history: string[] = [];
		render() {
			return [
				`Last key: ${this.lastKey}`,
				`History: ${this.history.join(', ')}`,
			];
		}
		handleInput(data: string) {
			// Simple display of what was received
			if (data.length === 1 && data.charCodeAt(0) >= 0x20) {
				this.lastKey = data;
			} else if (data === '\x1b[A') {
				this.lastKey = 'up';
			} else if (data === '\x1b[B') {
				this.lastKey = 'down';
			} else if (data === '\r') {
				this.lastKey = 'enter';
			} else if (data === '\x1b') {
				this.lastKey = 'escape';
			} else {
				this.lastKey = `raw:${JSON.stringify(data)}`;
			}
			this.history.push(this.lastKey);
		}
		invalidate() {}
	}

	it('shows typed character', async () => {
		const term = new TestTerminal(60, 5);
		const tui = new TUI(term);
		const app = new InputEchoApp();
		tui.addChild(app);
		tui.setFocus(app);
		tui.start();

		await wait();
		await term.flush();
		expect(term.getLastFrame()!.viewport[0]).toContain('(none)');

		term.sendInput('a');
		await wait();
		await term.flush();
		expect(term.getLastFrame()!.viewport[0]).toContain('Last key: a');

		tui.stop();
	});

	it('shows arrow key name', async () => {
		const term = new TestTerminal(60, 5);
		const tui = new TUI(term);
		const app = new InputEchoApp();
		tui.addChild(app);
		tui.setFocus(app);
		tui.start();

		await wait();
		await term.flush();

		term.sendInput('\x1b[A'); // Up arrow
		await wait();
		await term.flush();
		expect(term.getLastFrame()!.viewport[0]).toContain('Last key: up');

		term.sendInput('\x1b[B'); // Down arrow
		await wait();
		await term.flush();
		expect(term.getLastFrame()!.viewport[0]).toContain('Last key: down');

		tui.stop();
	});

	it('accumulates history', async () => {
		const term = new TestTerminal(60, 5);
		const tui = new TUI(term);
		const app = new InputEchoApp();
		tui.addChild(app);
		tui.setFocus(app);
		tui.start();

		await wait();
		await term.flush();

		term.sendInput('h');
		await wait(); await term.flush();
		term.sendInput('i');
		await wait(); await term.flush();

		expect(term.getLastFrame()!.viewport[1]).toContain('h, i');

		tui.stop();
	});

	it('key release events are filtered by default', async () => {
		const term = new TestTerminal(60, 5);
		const tui = new TUI(term);
		const app = new InputEchoApp();
		tui.addChild(app);
		tui.setFocus(app);
		tui.start();

		await wait();
		await term.flush();

		// Send a key release event (Kitty protocol format)
		term.sendInput('\x1b[97;1:3u'); // 'a' release
		await wait();
		await term.flush();

		// Should still show (none) — release was filtered
		expect(term.getLastFrame()!.viewport[0]).toContain('(none)');

		tui.stop();
	});

	it('wantsKeyRelease receives release events', async () => {
		const term = new TestTerminal(60, 5);
		const tui = new TUI(term);
		const releases: string[] = [];
		const app: Component = {
			render: () => ['waiting...'],
			handleInput: (data) => { releases.push(data); },
			invalidate: () => {},
			wantsKeyRelease: true,
		};
		tui.addChild(app);
		tui.setFocus(app);
		tui.start();

		await wait();
		await term.flush();

		term.sendInput('\x1b[97;1:3u'); // 'a' release
		await wait();

		expect(releases).toContain('\x1b[97;1:3u');

		tui.stop();
	});
});
