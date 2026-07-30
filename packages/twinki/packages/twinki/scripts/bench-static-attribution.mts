/**
 * Attribution harness: which statements in _doRenderInner scale with the
 * static prefix? Replicates the exact hot-path operations in isolation.
 */
const WIDTH = 120;
const STATIC = 10_000;
const LIVE = 12;
const FRAMES = 500;

const RESET = '\x1b[0m';

function makeStatic(n: number): string[] {
  const a: string[] = [];
  for (let i = 0; i < n; i++) a.push(`\x1b[36m[${i}]\x1b[0m scrollback message line with some content`);
  return a;
}
function makeLive(n: number, tick: number): string[] {
  const a: string[] = [];
  for (let i = 0; i < n; i++) a.push(i === n - 1 ? `live tail frame ${tick}` : `live line ${i} steady content here`);
  return a;
}

const accumulated = makeStatic(STATIC);

function time(label: string, fn: () => void) {
  fn(); fn(); // warm
  const t0 = performance.now();
  for (let i = 0; i < FRAMES; i++) fn();
  const t1 = performance.now();
  console.log(`${label.padEnd(46)} ${((t1 - t0) / FRAMES * 1000).toFixed(1).padStart(8)} µs/frame`);
}

// Stage 1: newLines = accumulatedStaticOutput.concat(newLines)
time('concat(static, live)  [line ~1645]', () => {
  const live = makeLive(LIVE, 1);
  const merged = accumulated.concat(live);
  if (merged.length < 0) throw new Error();
});

// Stage 2: applyLineResets over the FULL merged array
const merged = accumulated.concat(makeLive(LIVE, 1));
time('applyLineResets(merged) [line 1282]', () => {
  const copy = merged.slice();
  for (let i = 0; i < copy.length; i++) copy[i] = copy[i]! + RESET;
});

// Stage 3: diff loop over the FULL merged array (strings equal but distinct objects)
const prevDistinct = merged.map(l => l + RESET);
const newDistinct = merged.map(l => l + RESET);
time('diff loop, distinct string objects [1830]', () => {
  let first = -1, last = -1;
  const maxLen = Math.max(newDistinct.length, prevDistinct.length);
  for (let i = 0; i < maxLen; i++) {
    if ((prevDistinct[i] ?? '') !== (newDistinct[i] ?? '')) { if (first === -1) first = i; last = i; }
  }
  if (first === -2) throw new Error();
});

// Stage 3b: same diff loop when the static prefix is REFERENCE-identical
const shared = prevDistinct;
time('diff loop, shared string refs (ideal)', () => {
  let first = -1, last = -1;
  const maxLen = shared.length;
  for (let i = 0; i < maxLen; i++) {
    if ((shared[i] ?? '') !== (shared[i] ?? '')) { if (first === -1) first = i; last = i; }
  }
  if (first === -2) throw new Error();
});

// Stage 4: what it SHOULD cost — diff only the live suffix
time('diff loop, live suffix only (target)', () => {
  let first = -1, last = -1;
  for (let i = STATIC; i < newDistinct.length; i++) {
    if ((prevDistinct[i] ?? '') !== (newDistinct[i] ?? '')) { if (first === -1) first = i; last = i; }
  }
  if (first === -2) throw new Error();
});
