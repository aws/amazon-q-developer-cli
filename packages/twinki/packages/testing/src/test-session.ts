import { FrameCapturingTerminal } from './frame-capturing-terminal.js';
import type { Frame } from './frame-capturing-terminal.js';
import { TUI } from 'twinki';
import type { Component } from 'twinki';

export class TestSession {
	private terminal: FrameCapturingTerminal;
	private tui: TUI;

	constructor(component: Component, options: { cols?: number; rows?: number } = {}) {
		this.terminal = new FrameCapturingTerminal(options.cols ?? 80, options.rows ?? 24);
		this.tui = new TUI(this.terminal);
		this.tui.addChild(component);
	}

	async start(): Promise<void> {
		this.tui.start();
		await this.terminal.flush();
	}

	async stop(): Promise<void> {
		this.tui.stop();
		await this.terminal.flush();
	}

	sendInput(data: string): void {
		this.terminal.sendInput(data);
	}

	async waitForFrame(
		predicate: (frame: Frame) => boolean,
		timeoutMs = 1000,
	): Promise<Frame> {
		const start = Date.now();
		while (Date.now() - start < timeoutMs) {
			await this.terminal.flush();
			const frame = this.terminal.getLastFrame();
			if (frame && predicate(frame)) return frame;
			await new Promise((resolve) => setImmediate(resolve));
		}
		throw new Error(`waitForFrame timed out after ${timeoutMs}ms`);
	}

	async waitForText(text: string, timeoutMs = 1000): Promise<Frame> {
		return this.waitForFrame(
			(f) => f.viewport.some((l) => l.includes(text)),
			timeoutMs,
		);
	}

	getFrames(): Frame[] {
		return this.terminal.getFrames();
	}

	getLastFrame(): Frame | undefined {
		return this.terminal.getLastFrame();
	}

	getViewport(): string[] {
		return this.terminal.getViewport();
	}

	getTerminal(): FrameCapturingTerminal {
		return this.terminal;
	}

	getTUI(): TUI {
		return this.tui;
	}
}
