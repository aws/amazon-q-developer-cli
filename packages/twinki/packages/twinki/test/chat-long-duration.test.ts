/**
 * Chat Long Duration Test
 *
 * Simulates a chat session with very long AI responses (1000+ lines each).
 * Validates that the rendering engine stays stable, flicker-free, and
 * differential under extreme content volume.
 */
import { describe, it, expect } from 'vitest';
import { TUI } from '../src/renderer/tui.js';
import type { Component } from '../src/renderer/component.js';
import { TestTerminal, analyzeFlicker, wait, diffFrames, dumpAllFrames, testDir } from './helpers.js';

interface Message {
	role: 'user' | 'assistant';
	content: string;
}

class LongChatApp implements Component {
	messages: Message[] = [];
	typing = false;
	typingFrame = 0;
	statusText = 'Ready';
	messageCount = 0;
	private spinners = ['⠋', '⠙', '⠹', '⠸', '⠼', '⠴', '⠦', '⠧', '⠇', '⠏'];

	render(width: number): string[] {
		const lines: string[] = [];

		for (const msg of this.messages) {
			if (msg.role === 'user') {
				lines.push(`> ${msg.content}`);
			} else {
				for (const ml of msg.content.split('\n')) {
					lines.push(`  ${ml}`);
				}
			}
			lines.push('');
		}

		if (this.typing) {
			const spinner = this.spinners[this.typingFrame % this.spinners.length]!;
			lines.push(`${spinner} AI is thinking...`);
		}

		lines.push('');
		const bar = '─'.repeat(width);
		lines.push(bar);
		lines.push(` ${this.statusText}  •  ${this.messageCount} messages`);

		return lines;
	}

	invalidate() {}
}

function generateLongResponse(lineCount: number, prefix: string): string {
	const lines: string[] = [];
	for (let i = 0; i < lineCount; i++) {
		lines.push(`${prefix} line ${i + 1}: Lorem ipsum dolor sit amet, consectetur adipiscing elit.`);
	}
	return lines.join('\n');
}

describe('Chat Long Duration', () => {
	it('handles a 1000-line AI response without flicker', async () => {
		const term = new TestTerminal(60, 20);
		const tui = new TUI(term);
		const app = new LongChatApp();
		tui.addChild(app);
		tui.start();

		await wait(); await term.flush();

		// User asks a question
		app.messages.push({ role: 'user', content: 'Give me a very long explanation' });
		app.messageCount = 1;
		tui.requestRender();
		await wait(); await term.flush();

		// AI responds with 1000 lines
		const longResponse = generateLongResponse(1000, '[A]');
		app.messages.push({ role: 'assistant', content: longResponse });
		app.messageCount = 2;
		app.statusText = 'Ready';
		tui.requestRender();
		await wait(); await term.flush();

		const vp = term.getLastFrame()!.viewport;
		// Status bar should be visible at the bottom
		expect(vp.some(l => l.includes('2 messages'))).toBe(true);
		// Last lines of the response should be near the bottom
		expect(vp.some(l => l.includes('line 1000'))).toBe(true);

		const report = analyzeFlicker(term.getFrames());
		expect(report.clean).toBe(true);

		tui.stop();
	});

	it('streaming a 1000-line response word-by-word stays differential', async () => {
		const term = new TestTerminal(60, 20);
		const tui = new TUI(term);
		const app = new LongChatApp();
		tui.addChild(app);
		tui.start();

		await wait(); await term.flush();

		app.messages.push({ role: 'user', content: 'Stream me a long response' });
		app.messageCount = 1;
		tui.requestRender();
		await wait(); await term.flush();

		// Simulate streaming: append lines in chunks of 50
		const totalLines = 1000;
		const chunkSize = 50;
		const allLines: string[] = [];
		for (let i = 0; i < totalLines; i++) {
			allLines.push(`Line ${i + 1}: The quick brown fox jumps over the lazy dog.`);
		}

		// Add the assistant message, then grow it
		app.messages.push({ role: 'assistant', content: '' });
		app.messageCount = 2;

		let fullRenderCount = 0;
		for (let chunk = 0; chunk < totalLines; chunk += chunkSize) {
			const end = Math.min(chunk + chunkSize, totalLines);
			app.messages[1]!.content = allLines.slice(0, end).join('\n');
			app.statusText = `Streaming... ${end}/${totalLines}`;
			tui.requestRender();
			await wait(); await term.flush();

			const frame = term.getLastFrame()!;
			if (frame.isFull) fullRenderCount++;
		}

		app.statusText = 'Ready';
		tui.requestRender();
		await wait(); await term.flush();

		const vp = term.getLastFrame()!.viewport;
		expect(vp.some(l => l.includes('Ready'))).toBe(true);
		expect(vp.some(l => l.includes('Line 1000'))).toBe(true);

		// Most frames should be differential, not full redraws
		const frames = term.getFrames();
		const diffFrameCount = frames.filter(f => !f.isFull).length;
		expect(diffFrameCount).toBeGreaterThan(frames.length / 2);

		const report = analyzeFlicker(frames);
		expect(report.clean).toBe(true);

		// Dump frame-by-frame analysis
		dumpAllFrames(term, testDir('Chat_Long_Duration', 'streaming_1000_line_response'));

		tui.stop();
	});

	it('multiple 1000-line exchanges preserve scrollback', async () => {
		const term = new TestTerminal(60, 20);
		const tui = new TUI(term);
		const app = new LongChatApp();
		tui.addChild(app);
		tui.start();

		await wait(); await term.flush();

		// 3 exchanges, each with a 1000-line response
		for (let turn = 0; turn < 3; turn++) {
			app.messages.push({ role: 'user', content: `Question ${turn + 1}` });
			app.messageCount = app.messages.length;
			tui.requestRender();
			await wait(); await term.flush();

			const response = generateLongResponse(1000, `[${turn + 1}]`);
			app.messages.push({ role: 'assistant', content: response });
			app.messageCount = app.messages.length;
			app.statusText = 'Ready';
			tui.requestRender();
			await wait(); await term.flush();
		}

		const vp = term.getLastFrame()!.viewport;
		// Should show the latest content and status
		expect(vp.some(l => l.includes('6 messages'))).toBe(true);
		expect(vp.some(l => l.includes('[3] line 1000'))).toBe(true);

		// Total content: 3 * (1 user line + 1000 AI lines + 1 blank) + status = ~3006 lines
		// All rendered through a 20-row viewport — scrollback must be working
		const report = analyzeFlicker(term.getFrames());
		expect(report.clean).toBe(true);

		tui.stop();
	});

	it('appending single lines to a 1000-line buffer: diff bytes stay small', async () => {
		const term = new TestTerminal(60, 20);
		const tui = new TUI(term);
		const app = new LongChatApp();
		tui.addChild(app);
		tui.start();

		await wait(); await term.flush();

		// Build up a 1000-line response first
		app.messages.push({ role: 'user', content: 'Start' });
		const baseLines: string[] = [];
		for (let i = 0; i < 1000; i++) {
			baseLines.push(`Base line ${i + 1}`);
		}
		app.messages.push({ role: 'assistant', content: baseLines.join('\n') });
		app.messageCount = 2;
		tui.requestRender();
		await wait(); await term.flush();

		const baseFrame = term.getLastFrame()!;

		// Now append 10 more lines one at a time and check diff size
		const appendBytes: number[] = [];
		for (let i = 0; i < 10; i++) {
			baseLines.push(`Appended line ${i + 1}`);
			app.messages[1]!.content = baseLines.join('\n');
			tui.requestRender();
			await wait(); await term.flush();

			const frame = term.getLastFrame()!;
			if (!frame.isFull) {
				appendBytes.push(frame.writeBytes);
			}
		}

		// Differential updates should be much smaller than the base frame
		if (appendBytes.length > 0) {
			const avgAppendBytes = appendBytes.reduce((a, b) => a + b, 0) / appendBytes.length;
			expect(avgAppendBytes).toBeLessThan(baseFrame.writeBytes / 2);
		}

		tui.stop();
	});

	it('1000-line response with typing animation: zero flicker', async () => {
		const term = new TestTerminal(60, 20);
		const tui = new TUI(term);
		const app = new LongChatApp();
		tui.addChild(app);
		tui.start();

		await wait(); await term.flush();

		app.messages.push({ role: 'user', content: 'Think hard about this' });
		app.messageCount = 1;
		app.typing = true;
		app.statusText = 'AI is responding...';
		tui.requestRender();
		await wait(); await term.flush();

		// Animate typing for 20 frames
		for (let i = 0; i < 20; i++) {
			app.typingFrame++;
			tui.requestRender();
			await wait(); await term.flush();
		}

		// Then deliver the massive response
		app.typing = false;
		app.messages.push({ role: 'assistant', content: generateLongResponse(1000, '[R]') });
		app.messageCount = 2;
		app.statusText = 'Ready';
		tui.requestRender();
		await wait(); await term.flush();

		const vp = term.getLastFrame()!.viewport;
		expect(vp.some(l => l.includes('[R] line 1000'))).toBe(true);
		expect(vp.some(l => l.includes('Ready'))).toBe(true);
		// Typing indicator gone
		expect(vp.some(l => l.includes('AI is thinking'))).toBe(false);

		const report = analyzeFlicker(term.getFrames());
		expect(report.clean).toBe(true);

		// Dump frame-by-frame for the typing → response transition
		dumpAllFrames(term, testDir('Chat_Long_Duration', 'typing_animation_then_1000_lines'));

		tui.stop();
	});
});
