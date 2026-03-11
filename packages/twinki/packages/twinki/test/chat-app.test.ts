/**
 * Chat App E2E Test
 *
 * Simulates a real AI chat interface with:
 * - Scrolling message history (Static-like: grows over time)
 * - Live "typing" indicator that animates
 * - Status bar at the bottom
 * - User input → AI response cycles
 *
 * Validates:
 * - No flickering across 50+ frames
 * - No component overlap (status bar stays at bottom)
 * - Scrollback works (old messages preserved)
 * - Differential updates (only changed lines rewritten)
 * - Status bar never corrupted by message content
 */
import { describe, it, expect } from 'vitest';
import { TUI } from '../src/renderer/tui.js';
import type { Component } from '../src/renderer/component.js';
import { TestTerminal, analyzeFlicker, wait, diffFrames } from './helpers.js';

// ============================================================
// Chat App Component
// ============================================================

interface Message {
	role: 'user' | 'assistant';
	content: string;
}

class ChatApp implements Component {
	messages: Message[] = [];
	typing = false;
	typingFrame = 0;
	statusText = 'Ready';
	private spinners = ['⠋', '⠙', '⠹', '⠸', '⠼', '⠴', '⠦', '⠧', '⠇', '⠏'];

	render(width: number): string[] {
		const lines: string[] = [];

		// --- Message history ---
		for (const msg of this.messages) {
			if (msg.role === 'user') {
				lines.push(`> ${msg.content}`);
			} else {
				// AI messages can be multi-line
				const msgLines = msg.content.split('\n');
				for (const ml of msgLines) {
					lines.push(`  ${ml}`);
				}
			}
			lines.push(''); // blank line between messages
		}

		// --- Typing indicator ---
		if (this.typing) {
			const spinner = this.spinners[this.typingFrame % this.spinners.length]!;
			lines.push(`${spinner} AI is thinking...`);
		}

		// --- Pad to push status bar to bottom ---
		// We want the status bar at a fixed position relative to content
		lines.push(''); // separator

		// --- Status bar ---
		const bar = '─'.repeat(width);
		lines.push(bar);
		lines.push(` ${this.statusText}`);

		return lines;
	}

	addUserMessage(content: string) {
		this.messages.push({ role: 'user', content });
		this.statusText = 'Sending...';
	}

	startTyping() {
		this.typing = true;
		this.typingFrame = 0;
		this.statusText = 'AI is responding...';
	}

	tickTyping() {
		this.typingFrame++;
	}

	addAssistantMessage(content: string) {
		this.typing = false;
		this.messages.push({ role: 'assistant', content });
		this.statusText = 'Ready';
	}

	handleInput(data: string) {
		// In a real app this would handle text input
	}

	invalidate() {}
}

// ============================================================
// Tests
// ============================================================

describe('Chat App: Full Conversation Flow', () => {
	it('renders initial empty state with status bar', async () => {
		const term = new TestTerminal(50, 15);
		const tui = new TUI(term);
		const app = new ChatApp();
		tui.addChild(app);
		tui.start();

		await wait(); await term.flush();

		const vp = term.getLastFrame()!.viewport;
		// Status bar should be present
		expect(vp.some(l => l.includes('Ready'))).toBe(true);
		expect(vp.some(l => l.includes('─'))).toBe(true);

		tui.stop();
	});

	it('user message appears in history', async () => {
		const term = new TestTerminal(50, 15);
		const tui = new TUI(term);
		const app = new ChatApp();
		tui.addChild(app);
		tui.start();

		await wait(); await term.flush();

		app.addUserMessage('Hello, AI!');
		tui.requestRender();
		await wait(); await term.flush();

		const vp = term.getLastFrame()!.viewport;
		expect(vp.some(l => l.includes('> Hello, AI!'))).toBe(true);
		expect(vp.some(l => l.includes('Sending...'))).toBe(true);

		tui.stop();
	});

	it('typing indicator animates without flicker', async () => {
		const term = new TestTerminal(50, 15);
		const tui = new TUI(term);
		const app = new ChatApp();
		tui.addChild(app);
		tui.start();

		await wait(); await term.flush();

		app.addUserMessage('What is 2+2?');
		app.startTyping();
		tui.requestRender();
		await wait(); await term.flush();

		// Animate typing indicator for 10 frames
		for (let i = 0; i < 10; i++) {
			app.tickTyping();
			tui.requestRender();
			await wait(); await term.flush();
		}

		const frames = term.getFrames();
		// Should have: initial + user msg + 10 typing ticks = 12
		expect(frames.length).toBe(12);

		// Verify typing indicator visible in each typing frame
		for (let i = 2; i < 12; i++) {
			expect(frames[i]!.viewport.some(l => l.includes('AI is thinking...'))).toBe(true);
		}

		// Zero flicker
		const report = analyzeFlicker(frames);
		expect(report.clean).toBe(true);

		tui.stop();
	});

	it('full conversation: user → typing → AI response', async () => {
		const term = new TestTerminal(50, 20);
		const tui = new TUI(term);
		const app = new ChatApp();
		tui.addChild(app);
		tui.start();

		await wait(); await term.flush();

		// User sends message
		app.addUserMessage('What is the meaning of life?');
		tui.requestRender();
		await wait(); await term.flush();

		// AI starts typing
		app.startTyping();
		tui.requestRender();
		await wait(); await term.flush();

		// Animate typing for a few frames
		for (let i = 0; i < 5; i++) {
			app.tickTyping();
			tui.requestRender();
			await wait(); await term.flush();
		}

		// AI responds
		app.addAssistantMessage('The meaning of life is 42.');
		tui.requestRender();
		await wait(); await term.flush();

		const vp = term.getLastFrame()!.viewport;
		expect(vp.some(l => l.includes('> What is the meaning of life?'))).toBe(true);
		expect(vp.some(l => l.includes('The meaning of life is 42.'))).toBe(true);
		expect(vp.some(l => l.includes('Ready'))).toBe(true);
		// Typing indicator should be gone
		expect(vp.some(l => l.includes('AI is thinking...'))).toBe(false);

		tui.stop();
	});

	it('multiple conversation turns preserve history', async () => {
		const term = new TestTerminal(60, 25);
		const tui = new TUI(term);
		const app = new ChatApp();
		tui.addChild(app);
		tui.start();

		await wait(); await term.flush();

		const exchanges = [
			{ user: 'Hi!', ai: 'Hello! How can I help?' },
			{ user: 'What is TypeScript?', ai: 'TypeScript is a typed superset of JavaScript.' },
			{ user: 'Thanks!', ai: 'You\'re welcome!' },
		];

		for (const ex of exchanges) {
			app.addUserMessage(ex.user);
			tui.requestRender();
			await wait(); await term.flush();

			app.startTyping();
			tui.requestRender();
			await wait(); await term.flush();

			// Brief typing animation
			for (let i = 0; i < 3; i++) {
				app.tickTyping();
				tui.requestRender();
				await wait(); await term.flush();
			}

			app.addAssistantMessage(ex.ai);
			tui.requestRender();
			await wait(); await term.flush();
		}

		const vp = term.getLastFrame()!.viewport;

		// All messages should be in history
		expect(vp.some(l => l.includes('> Hi!'))).toBe(true);
		expect(vp.some(l => l.includes('Hello! How can I help?'))).toBe(true);
		expect(vp.some(l => l.includes('> What is TypeScript?'))).toBe(true);
		expect(vp.some(l => l.includes('typed superset'))).toBe(true);
		expect(vp.some(l => l.includes('> Thanks!'))).toBe(true);
		expect(vp.some(l => l.includes('welcome'))).toBe(true);

		// Status bar shows Ready
		expect(vp.some(l => l.includes('Ready'))).toBe(true);

		tui.stop();
	});

	it('zero flicker across entire conversation', async () => {
		const term = new TestTerminal(50, 20);
		const tui = new TUI(term);
		const app = new ChatApp();
		tui.addChild(app);
		tui.start();

		await wait(); await term.flush();

		// Run 3 full conversation turns with typing animation
		for (let turn = 0; turn < 3; turn++) {
			app.addUserMessage(`Question ${turn + 1}`);
			tui.requestRender();
			await wait(); await term.flush();

			app.startTyping();
			tui.requestRender();
			await wait(); await term.flush();

			for (let i = 0; i < 5; i++) {
				app.tickTyping();
				tui.requestRender();
				await wait(); await term.flush();
			}

			app.addAssistantMessage(`Answer ${turn + 1}: This is the response.`);
			tui.requestRender();
			await wait(); await term.flush();
		}

		const frames = term.getFrames();
		// 1 initial + 3 turns * (user + typing_start + 5 ticks + response) = 1 + 3*8 = 25
		expect(frames.length).toBe(25);

		const report = analyzeFlicker(frames);
		expect(report.clean).toBe(true);

		tui.stop();
	});

	it('status bar never overlaps with message content', async () => {
		const term = new TestTerminal(50, 15);
		const tui = new TUI(term);
		const app = new ChatApp();
		tui.addChild(app);
		tui.start();

		await wait(); await term.flush();

		// Add enough messages to fill the viewport
		for (let i = 0; i < 8; i++) {
			app.addUserMessage(`Message ${i + 1}`);
			app.addAssistantMessage(`Response ${i + 1}`);
			tui.requestRender();
			await wait(); await term.flush();
		}

		// Check every frame: status bar line should contain '─' and
		// the line after it should contain the status text
		const frames = term.getFrames();
		for (const frame of frames) {
			const barIdx = frame.viewport.findIndex(l => l.includes('─'.repeat(10)));
			if (barIdx >= 0) {
				const statusLine = frame.viewport[barIdx + 1];
				if (statusLine) {
					// Status text should not contain message content
					expect(statusLine).not.toContain('> Message');
					expect(statusLine).not.toContain('Response');
				}
			}
		}

		tui.stop();
	});

	it('differential updates during typing only change spinner line', async () => {
		const term = new TestTerminal(50, 20);
		const tui = new TUI(term);
		const app = new ChatApp();
		tui.addChild(app);
		tui.start();

		await wait(); await term.flush();

		app.addUserMessage('Test question');
		app.startTyping();
		tui.requestRender();
		await wait(); await term.flush();

		// Capture frame before tick
		const beforeTick = term.getLastFrame()!;

		app.tickTyping();
		tui.requestRender();
		await wait(); await term.flush();

		const afterTick = term.getLastFrame()!;

		// Only the spinner line should change
		const diff = diffFrames(beforeTick, afterTick);
		expect(diff.length).toBe(1);
		expect(diff[0]).toContain('AI is thinking');

		tui.stop();
	});

	it('multi-line AI response renders correctly', async () => {
		const term = new TestTerminal(60, 25);
		const tui = new TUI(term);
		const app = new ChatApp();
		tui.addChild(app);
		tui.start();

		await wait(); await term.flush();

		app.addUserMessage('Write me a haiku');
		tui.requestRender();
		await wait(); await term.flush();

		app.addAssistantMessage(
			'An old silent pond\nA frog jumps into the pond\nSplash! Silence again'
		);
		tui.requestRender();
		await wait(); await term.flush();

		const vp = term.getLastFrame()!.viewport;
		expect(vp.some(l => l.includes('An old silent pond'))).toBe(true);
		expect(vp.some(l => l.includes('A frog jumps'))).toBe(true);
		expect(vp.some(l => l.includes('Splash!'))).toBe(true);

		tui.stop();
	});

	it('long conversation with scrollback beyond viewport', async () => {
		const term = new TestTerminal(50, 10); // Small viewport
		const tui = new TUI(term);
		const app = new ChatApp();
		tui.addChild(app);
		tui.start();

		await wait(); await term.flush();

		// Add many messages — more than viewport can show
		for (let i = 0; i < 15; i++) {
			app.addUserMessage(`Question ${i + 1}`);
			app.addAssistantMessage(`Answer ${i + 1}`);
			tui.requestRender();
			await wait(); await term.flush();
		}

		const vp = term.getLastFrame()!.viewport;

		// Latest messages should be visible
		expect(vp.some(l => l.includes('Question 15') || l.includes('Answer 15'))).toBe(true);

		// Status bar should still be present
		expect(vp.some(l => l.includes('Ready'))).toBe(true);

		// Zero flicker across all frames
		const report = analyzeFlicker(term.getFrames());
		expect(report.clean).toBe(true);

		tui.stop();
	});

	it('rapid message burst (simulating streaming)', async () => {
		const term = new TestTerminal(50, 15);
		const tui = new TUI(term);
		const app = new ChatApp();
		tui.addChild(app);
		tui.start();

		await wait(); await term.flush();

		app.addUserMessage('Stream test');
		tui.requestRender();
		await wait(); await term.flush();

		// Simulate streaming: AI response arrives word by word
		const words = 'The quick brown fox jumps over the lazy dog'.split(' ');
		let partial = '';
		app.startTyping();

		for (const word of words) {
			partial += (partial ? ' ' : '') + word;
			// Remove typing indicator, update last message
			app.typing = false;
			if (app.messages[app.messages.length - 1]?.role === 'assistant') {
				app.messages[app.messages.length - 1]!.content = partial;
			} else {
				app.messages.push({ role: 'assistant', content: partial });
			}
			tui.requestRender();
			await wait(); await term.flush();
		}

		app.statusText = 'Ready';
		tui.requestRender();
		await wait(); await term.flush();

		const vp = term.getLastFrame()!.viewport;
		expect(vp.some(l => l.includes('The quick brown fox jumps over the lazy dog'))).toBe(true);
		expect(vp.some(l => l.includes('Ready'))).toBe(true);

		// Zero flicker
		const report = analyzeFlicker(term.getFrames());
		expect(report.clean).toBe(true);

		tui.stop();
	});
});
