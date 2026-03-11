import React from 'react';
import { render as twinkiRender } from 'twinki';
import type { Instance } from 'twinki';
import { FrameCapturingTerminal } from '@twinki/testing';
import type { Frame } from '@twinki/testing';

export interface RenderResult {
	lastFrame(): string;
	frames: Frame[];
	stdin: { write(data: string): void };
	unmount(): void;
	rerender(element: React.ReactElement): void;
}

export function render(element: React.ReactElement, options?: { columns?: number; rows?: number }): RenderResult {
	const terminal = new FrameCapturingTerminal(options?.columns ?? 80, options?.rows ?? 24);
	const instance: Instance = twinkiRender(element, { terminal, exitOnCtrlC: false });

	// Flush synchronously to capture first frame
	terminal.flush();

	const stripAnsi = (s: string) =>
		s.replace(/\x1b\[[0-9;]*[mGKHJ]/g, '')
		 .replace(/\x1b\]8;;[^\x07]*\x07/g, '')
		 .replace(/\x1b_[^\x07\x1b]*(?:\x07|\x1b\\)/g, '')
		 .replace(/\x1b\[\?[0-9;]*[hlsu]/g, '')
		 .replace(/\x1b\[[0-9;]*[ABCDEFGHJKSTfn]/g, '');

	return {
		lastFrame() {
			const f = terminal.getLastFrame();
			if (!f) return '';
			const lines = f.viewport.map(stripAnsi);
			let last = lines.length - 1;
			while (last >= 0 && lines[last].trim() === '') last--;
			return lines.slice(0, last + 1).join('\n');
		},
		get frames() {
			return terminal.getFrames();
		},
		stdin: {
			write(data: string) {
				terminal.sendInput(data);
			},
		},
		unmount() {
			instance.unmount();
		},
		rerender(newElement: React.ReactElement) {
			instance.rerender(newElement);
		},
	};
}
