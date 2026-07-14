import { describe, test, expect, beforeEach, afterEach } from 'bun:test';
import {
  pickTip,
  formatTipLine,
  __TIPS_FOR_TESTS,
  type TipContext,
  type TipSurface,
} from './tips';

// try-lite is gated on KIRO_LITE_ROLLOUT_ENABLED. Default it ON so the existing
// "shows in cohort" assertions hold; the gating block below flips it OFF.
beforeEach(() => {
  process.env.KIRO_LITE_ROLLOUT_ENABLED = '1';
});
afterEach(() => {
  delete process.env.KIRO_LITE_ROLLOUT_ENABLED;
});

/** Deterministic rng that always returns `v` (must be in [0,1)). */
const constRng = (v: number) => () => v;

/** All string tip texts in a group (every current tip uses a string body). */
function textsOf(group: readonly { text: unknown }[]): string[] {
  return group
    .map((t) => t.text)
    .filter((t): t is string => typeof t === 'string');
}

const SHARED_TEXT = textsOf(__TIPS_FOR_TESTS.SHARED);
const TUI_TEXT = textsOf(__TIPS_FOR_TESTS.TUI_ONLY);
const LITE_TEXT = textsOf(__TIPS_FOR_TESTS.LITE_ONLY);

const TRY_LITE_TEXT = TUI_TEXT.find((t) => t.includes('/lite'))!;

function ctx(over: Partial<TipContext> = {}): TipContext {
  return {
    surface: 'tui',
    engine: 'v2',
    recommendLiteUi: false,
    ...over,
  };
}

/** Sweep rng across [0,1) and collect every distinct tip the picker can yield. */
function allOutcomes(c: TipContext): Set<string> {
  const out = new Set<string>();
  for (let i = 0; i < 1000; i++) {
    const tip = pickTip(c, constRng(i / 1000));
    if (tip) out.add(tip);
  }
  return out;
}

describe('pickTip — surface filtering', () => {
  test('TUI surface never yields a LITE_ONLY tip', () => {
    const outcomes = allOutcomes(
      ctx({ surface: 'tui', recommendLiteUi: true })
    );
    for (const liteOnly of LITE_TEXT) {
      expect(outcomes.has(liteOnly)).toBe(false);
    }
  });

  test('Lite surface never yields a TUI_ONLY tip (incl. the Ctrl+O conflict)', () => {
    const outcomes = allOutcomes(
      ctx({ surface: 'lite', recommendLiteUi: true })
    );
    for (const tuiOnly of TUI_TEXT) {
      expect(outcomes.has(tuiOnly)).toBe(false);
    }
    // The lite-inspect tip (Ctrl+O = inspect panel) is fine in Lite...
    expect(
      [...outcomes].some((t) => t.includes('watch a running subagent'))
    ).toBe(true);
  });
});

describe('pickTip — gating', () => {
  test('try-lite tip shows only when recommendLiteUi is set', () => {
    expect(allOutcomes(ctx({ recommendLiteUi: true })).has(TRY_LITE_TEXT)).toBe(
      true
    );
    expect(
      allOutcomes(ctx({ recommendLiteUi: false })).has(TRY_LITE_TEXT)
    ).toBe(false);
  });

  test('try-lite tip is gated off when KIRO_LITE_ROLLOUT_ENABLED is not "1"', () => {
    // In-cohort context, but the rollout flag is off → /lite is a no-op, so the
    // nudge must not appear regardless of recommendLiteUi.
    delete process.env.KIRO_LITE_ROLLOUT_ENABLED;
    expect(allOutcomes(ctx({ recommendLiteUi: true })).has(TRY_LITE_TEXT)).toBe(
      false
    );
    process.env.KIRO_LITE_ROLLOUT_ENABLED = '0';
    expect(allOutcomes(ctx({ recommendLiteUi: true })).has(TRY_LITE_TEXT)).toBe(
      false
    );
  });

  test('engine-gated tip (Ctrl+X kill-subagent) shows on v2, not on v3/kas', () => {
    const killTip = LITE_TEXT.find((t) => t.includes('Ctrl+X'))!;
    expect(killTip).toBeDefined();
    expect(
      allOutcomes(ctx({ surface: 'lite', engine: 'v2' })).has(killTip)
    ).toBe(true);
    expect(
      allOutcomes(ctx({ surface: 'lite', engine: 'kas' })).has(killTip)
    ).toBe(false);
  });
});

describe('pickTip — chance math (deterministic rng)', () => {
  // try-lite is the only featured tip: it occupies [0,0.35), the plain pool
  // occupies [0.35,1.0).
  const withLite = ctx({ recommendLiteUi: true });

  test('roll in [0,0.35) → try-lite tip', () => {
    expect(pickTip(withLite, constRng(0))).toBe(TRY_LITE_TEXT);
    expect(pickTip(withLite, constRng(0.34))).toBe(TRY_LITE_TEXT);
  });

  test('roll in [0.35,1.0) → a plain (non-featured) tip', () => {
    const tip = pickTip(withLite, constRng(0.8))!;
    expect(tip).not.toBe(TRY_LITE_TEXT);
    // It must be one of the eligible plain tips (all of SHARED, plus TUI_ONLY
    // minus try-lite).
    const plain = [
      ...SHARED_TEXT,
      ...TUI_TEXT.filter((t) => t !== TRY_LITE_TEXT),
    ];
    expect(plain).toContain(tip);
  });

  test('an ineligible featured tip donates its share to the pool', () => {
    // No lite recommendation → try-lite ineligible, so plain tips split the
    // full [0,1) range uniformly. Every roll lands on a plain tip.
    const noLite = ctx({ recommendLiteUi: false });
    expect(pickTip(noLite, constRng(0))).not.toBe(TRY_LITE_TEXT);
    expect(pickTip(noLite, constRng(0.99))).not.toBe(TRY_LITE_TEXT);
  });

  test('with no featured tips eligible, selection is uniform over plain tips', () => {
    const outcomes = allOutcomes(ctx({ recommendLiteUi: false }));
    expect(outcomes.has(TRY_LITE_TEXT)).toBe(false);
    // Every SHARED and TUI-minus-trylite plain tip is reachable.
    expect(outcomes.size).toBeGreaterThanOrEqual(4);
  });
});

describe('pickTip — determinism', () => {
  test('same context + same rng value → same tip', () => {
    const c = ctx({ recommendLiteUi: true });
    for (const v of [0, 0.2, 0.5, 0.85, 0.999]) {
      expect(pickTip(c, constRng(v))).toBe(pickTip(c, constRng(v)));
    }
  });

  test('always returns a defined tip for the welcome (SHARED is never empty)', () => {
    const surfaces: TipSurface[] = ['tui', 'lite'];
    for (const surface of surfaces) {
      expect(pickTip(ctx({ surface }), constRng(0.999))).toBeDefined();
    }
  });
});

describe('formatTipLine', () => {
  test('wraps the tip with a "Tip:" label and the body', () => {
    const line = formatTipLine('hello world');
    expect(line).toContain('Tip:');
    expect(line).toContain('hello world');
  });
});

describe('tip catalog', () => {
  test('a /feedback tip is part of the shared rotation', () => {
    const feedback = SHARED_TEXT.find((t) => t.includes('/feedback'));
    expect(feedback).toBeDefined();
  });
});
