/**
 * CPU-profile driver for the static-buffer render path.
 *
 * Simulates a long chat session: 10,000 lines of committed scrollback plus a
 * live region that mutates every frame (streaming / typing). Run under
 * `bun --cpu-prof` and analyze with packages/tui/scripts/analyze-profile.ts.
 *
 *   bun --cpu-prof --cpu-prof-dir=/tmp/prof-after scripts/profile-static-render.mts
 */
import { TUI } from '../src/renderer/tui.js';
import type { Component } from '../src/renderer/component.js';
import type { Terminal } from '../src/terminal/terminal.js';

const STATIC_LINES = Number(process.env.PROF_STATIC ?? 10_000);
const LIVE_LINES = 12;
const FRAMES = Number(process.env.PROF_FRAMES ?? 20_000);

class NullTerminal implements Terminal {
  columns = 120;
  rows = 40;
  start(): void {}
  stop(): void {}
  write(_s: string): void {}
  moveBy(): void {}
  hideCursor(): void {}
  showCursor(): void {}
  clearLine(): void {}
  clearFromCursor(): void {}
  clearScreen(): void {}
  enableMouse(): void {}
  disableMouse(): void {}
  setTitle(): void {}
  drainInput(): string {
    return '';
  }
  get kittyProtocolActive(): boolean {
    return false;
  }
  onData(): void {}
  onResize(): void {}
}

let tick = 0;
const live: Component = {
  render(): string[] {
    const out: string[] = [];
    for (let i = 0; i < LIVE_LINES; i++) {
      out.push(
        i === LIVE_LINES - 1
          ? `\x1b[32m>\x1b[0m streaming token ${tick} of the assistant reply`
          : `live line ${i} steady content here`
      );
    }
    return out;
  },
  invalidate() {},
} as unknown as Component;

const term = new NullTerminal();
const tui = new TUI(term as unknown as Terminal, {
  staticScrollbackCap: 1_000_000,
});
tui.addChild(live);
tui.start();

const statics: string[] = [];
for (let i = 0; i < STATIC_LINES; i++) {
  statics.push(
    `\x1b[36m[${i}]\x1b[0m committed scrollback message line with some content`
  );
}
tui.writeStaticLines(statics);

const renderNow = () => (tui as unknown as { doRender(): void }).doRender();
for (let i = 0; i < 3; i++) {
  tick++;
  renderNow();
}

const t0 = performance.now();
for (let i = 0; i < FRAMES; i++) {
  tick++;
  renderNow();
}
const elapsed = performance.now() - t0;
// Capture metrics BEFORE stop() — stop() tears down renderer state.
const summary = {
  staticLines: tui.staticBufferLines,
  frames: FRAMES,
  totalMs: Number(elapsed.toFixed(1)),
  msPerFrame: Number((elapsed / FRAMES).toFixed(4)),
  fullRedraws: tui.fullRedraws,
  maxRenderMs: Number(tui.perfMaxRenderMs.toFixed(3)),
};
tui.stop();

console.log(JSON.stringify(summary, null, 2));
