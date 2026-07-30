/**
 * Proof harness: cost of one differential render as a function of the
 * accumulated static-scrollback buffer size.
 *
 * The static prefix is immutable by construction, so render cost SHOULD be
 * O(live lines). If it scales with static-buffer size, the "no redraw of
 * static buffer" invariant is broken.
 */
import { TUI } from '../src/renderer/tui.js';
import type { Component } from '../src/renderer/component.js';
import type { Terminal } from '../src/terminal/terminal.js';

class NullTerminal implements Terminal {
  columns = 120;
  rows = 40;
  bytesWritten = 0;
  writeCalls = 0;
  start(): void {}
  stop(): void {}
  write(s: string): void {
    this.bytesWritten += s.length;
    this.writeCalls++;
  }
  moveBy(): void {}
  hideCursor(): void {}
  showCursor(): void {}
  clearLine(): void {}
  clearFromCursor(): void {}
  clearScreen(): void {}
  enableMouse(): void {}
  disableMouse(): void {}
  setTitle(): void {}
  drainInput(): string { return ''; }
  get kittyProtocolActive(): boolean { return false; }
  onData(): void {}
  onResize(): void {}
}

/** Live component: `liveCount` lines; one line mutates each frame (streaming). */
function makeLive(liveCount: number): Component & { tick: number } {
  const c = {
    tick: 0,
    render(_width: number): string[] {
      const out: string[] = [];
      for (let i = 0; i < liveCount; i++) {
        out.push(i === liveCount - 1
          ? `live tail frame ${c.tick} — streaming token`
          : `live line ${i} steady content here`);
      }
      return out;
    },
  } as unknown as Component & { tick: number };
  return c;
}

function bench(staticLines: number, liveLines: number, frames: number, wideLines = false, wide = false) {
  const term = new NullTerminal();
  const tui = new TUI(term as unknown as Terminal, { staticScrollbackCap: 1_000_000, wideLines });
  const live = makeLive(liveLines);
  tui.addChild(live as unknown as Component);

  const statics: string[] = [];
  for (let i = 0; i < staticLines; i++) {
    statics.push(wide
      ? `\x1b[36m[${i}]\x1b[0m ` + 'soft-wrapped scrollback content that exceeds terminal width '.repeat(3)
      : `\x1b[36m[${i}]\x1b[0m scrollback message line with some content`);
  }
  if (statics.length) tui.writeStaticLines(statics);

  // Warm up + establish previousLines
  for (let i = 0; i < 3; i++) { live.tick++; (tui as any)._doRenderInner(); }

  const before = term.bytesWritten;
  const t0 = performance.now();
  for (let i = 0; i < frames; i++) {
    live.tick++;
    (tui as any)._doRenderInner();
  }
  const t1 = performance.now();
  const bytes = term.bytesWritten - before;

  return {
    staticLines,
    msPerFrame: (t1 - t0) / frames,
    bytesPerFrame: Math.round(bytes / frames),
    fullRedraws: (tui as any).fullRedrawCount,
  };
}

const LIVE = 12;
const FRAMES = 200;
console.log(`live=${LIVE} lines, ${FRAMES} frames/measurement, terminal 120x40\n`);
console.log('A) wideLines=false, narrow static lines');
console.log('staticLines | ms/frame | bytes written/frame');
console.log('------------+----------+--------------------');
for (const s of [0, 100, 500, 1000, 2000, 5000, 10000, 20000]) {
  const r = bench(s, LIVE, FRAMES);
  console.log(
    `${String(r.staticLines).padStart(11)} | ${r.msPerFrame.toFixed(3).padStart(8)} | ${String(r.bytesPerFrame).padStart(19)}`
  );
}

console.log('\nB) wideLines=true (lite mode), soft-wrapping static lines');
console.log('staticLines | ms/frame | bytes written/frame');
console.log('------------+----------+--------------------');
for (const s of [0, 100, 500, 1000, 2000, 5000, 10000]) {
  const r = bench(s, LIVE, FRAMES, true, true);
  console.log(
    `${String(r.staticLines).padStart(11)} | ${r.msPerFrame.toFixed(3).padStart(8)} | ${String(r.bytesPerFrame).padStart(19)}`
  );
}
