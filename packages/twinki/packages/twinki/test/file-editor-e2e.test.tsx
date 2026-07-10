/**
 * 28-file-editor E2E — drives the example headlessly through the twinki test
 * harness (keyboard + mouse), capturing a "screenshot" (text frame) after each
 * interaction. Artifacts land in test/.artifacts/28-file-editor_E2E/<test>/:
 *   - screenshots.txt : labeled frame after every step (for human review)
 *   - all-frames.txt  : every frame + inter-frame diffs + flicker report
 *
 * render() is imported from 'twinki' (dist) so it shares the same module
 * instance/React context as the example (which also imports 'twinki').
 *
 * The sample workspace is copied to a temp dir, so saving never touches the
 * tracked example files.
 */
import { describe, it, expect, beforeAll, afterAll } from 'vitest';
import React from 'react';
import { cpSync, mkdtempSync, rmSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { dirname, join } from 'node:path';
import { fileURLToPath } from 'node:url';
import { render } from 'twinki';
import { TestTerminal, wait, testDir, dumpAllFrames, serializeFrame } from './helpers.js';
import { App } from '../../../examples/28-file-editor/App.js';

const sampleSrc = join(dirname(fileURLToPath(import.meta.url)), '../../../examples/28-file-editor/sample-workspace');

describe('28-file-editor E2E', () => {
	let workRoot: string;

	beforeAll(() => {
		workRoot = mkdtempSync(join(tmpdir(), 'fe-e2e-'));
		cpSync(sampleSrc, workRoot, { recursive: true });
	});

	afterAll(() => {
		rmSync(workRoot, { recursive: true, force: true });
	});

	it('drives the editor with keyboard + mouse and captures frames', async () => {
		const term = new TestTerminal(100, 32);
		const dir = testDir('28-file-editor E2E', 'drive');
		const shots: string[] = [];

		const shot = async (label: string, ms = 60) => {
			await wait(ms);
			await term.flush();
			const f = term.getLastFrame();
			if (f) shots.push(`### ${label}\n${serializeFrame(f, term.columns)}`);
		};

		const inst = render(<App workspaceRoot={workRoot} />, {
			terminal: term,
			exitOnCtrlC: false,
			mouse: true,
		});

		await shot('01 initial render (NORMAL, tree expanded)', 150);

		// Navigate down to app.tsx (visible index 4: scripts, build.sh, src, utils, app.tsx).
		term.sendInput('\x1b[B'); await shot('02 down -> build.sh');
		term.sendInput('\x1b[B'); await shot('03 down -> src');
		term.sendInput('\x1b[B'); await shot('04 down -> utils');
		term.sendInput('\x1b[B'); await shot('05 down -> app.tsx');

		// Open it (view mode) and let shiki highlight.
		term.sendInput('\r'); await shot('06 enter -> open app.tsx (view)', 800);

		// Enter edit mode, type, save, leave edit mode.
		term.sendInput('e'); await shot('07 e -> INSERT');
		term.sendInput('// hi from e2e\n'); await shot('08 typed a line (dirty)');
		term.sendInput('\x13'); await shot('09 Ctrl+S -> saved');
		term.sendInput('\x1b'); await shot('10 Esc -> NORMAL');

		// Rotate themes.
		term.sendInput('\t'); await shot('11 Tab -> next theme', 500);
		term.sendInput('\x1b[Z'); await shot('12 Shift+Tab -> prev theme', 500);

		// Mouse: click the first explorer row (viewport row 2 -> 1-based y=3) to toggle it.
		term.sendInput('\x1b[<0;5;3M');
		term.sendInput('\x1b[<0;5;3m');
		await shot('13 mouse click first tree row', 150);

		// Write artifacts for review.
		writeFileSync(join(dir, 'screenshots.txt'), shots.join('\n\n'));
		// dumpAllFrames also writes a flicker report. It compares every captured
		// snapshot, so transitions between *different* screens (placeholder -> code
		// -> editor rulers) can surface false-positive "flicker" at columns where a
		// code line has spaces the neighboring screens don't. Same-screen redraws
		// are atomic/diffed and do not flicker.
		dumpAllFrames(term, dir);

		// Assertions (lenient — the screenshots are the primary deliverable).
		const frames = term.getFrames();
		const all = frames.map((f) => f.viewport.join('\n')).join('\n===\n');

		expect(frames.length).toBeGreaterThan(5);
		expect(all).toContain('src'); // tree rendered
		expect(all).toContain('app.tsx'); // file visible in tree
		expect(all).toContain('NORMAL'); // statusline mode segment
		expect(all).toContain('INSERT'); // edit mode reached
		expect(all).toContain('Counter'); // app.tsx content shown in the editor
		expect(all).toContain('dracula'); // theme rotated (index 1)

		inst.unmount();
	});
});
