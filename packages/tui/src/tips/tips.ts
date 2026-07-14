// One tip is shown per launch below the KIRO banner. Ephemeral in the TUI,
// baked into the banner in Lite. Pure module: callers build a TipContext and
// call pickTip; selection is stateless and random per launch (seed rng in tests).
import chalk from 'chalk';

import type { AgentEngine } from '../agent-engine.js';
import { getActiveGlyphs } from '../hooks/useGlyphs.js';

export type TipSurface = 'tui' | 'lite';

// Honor chat.allowAsciiArt: in ASCII mode arrows become ^/->/<; no-op otherwise.
function applyGlyphs(text: string): string {
  const g = getActiveGlyphs();
  return text
    .replace(/↑/g, g.arrowUp)
    .replace(/←/g, g.arrowLeft)
    .replace(/→/g, g.arrow);
}

export interface TipContext {
  surface: TipSurface;
  /** `'v2'` | `'kas'` (kas is v3). Gates tips via a tip's `engines` allowlist. */
  engine: AgentEngine;
  /** In the lite cohort, on a TTY, with no default UI chosen. Gates "try Lite". */
  recommendLiteUi: boolean;
}

interface TipDef {
  id: string;
  /** Fixed probability (0–1) when eligible. Omit for the leftover pool. */
  chance?: number;
  /** Env flag that must be `'1'` for this tip to show (rollout gate). */
  requiresFlag?: string;
  /** Engines this tip's feature exists on. Omitted = all. */
  engines?: readonly AgentEngine[];
  when?: (ctx: TipContext) => boolean;
  text: string | ((ctx: TipContext) => string);
}

// Shown in both the TUI and Lite.
const SHARED: readonly TipDef[] = [
  {
    id: 'default-ui',
    text: 'Choose which layout opens by default in /settings → display → Default UI.',
  },
  {
    id: 'theme',
    text: 'Switch between Auto, Dark, Light, and Custom themes via /settings → theme (Auto follows your terminal).',
  },
  {
    id: 'interrupt',
    text: 'Press Esc or Ctrl+C while Kiro is working to interrupt the current turn cleanly.',
  },
  {
    id: 'feedback',
    text: 'Share your thoughts anytime — type /feedback to send feedback to the Kiro team.',
  },
];

// TUI only (Ctrl+O here expands output; in Lite it opens the inspect panel).
const TUI_ONLY: readonly TipDef[] = [
  {
    id: 'try-lite',
    chance: 0.35,
    requiresFlag: 'KIRO_LITE_ROLLOUT_ENABLED',
    when: (c) => c.recommendLiteUi,
    text: 'Want a simpler, lighter-weight terminal with all the same features? Try /lite!',
  },
  {
    id: 'tui-expand-output',
    text: "Press Ctrl+O to expand a tool's full output, then again to collapse it.",
  },
  {
    id: 'tui-verbosity',
    text: 'Open /verbosity to tune truncation, output filters, and density anytime.',
  },
];

// Lite only.
const LITE_ONLY: readonly TipDef[] = [
  {
    id: 'lite-swap-tui',
    text: 'Type /tui to swap to the classic interface and /lite to return — your conversation re-renders in the new style.',
  },
  {
    id: 'lite-inspect-subagent',
    text: 'Press Ctrl+O to watch a running subagent live — Shift+←/→ cycles subagents, Ctrl+A/Ctrl+Z jump to top/bottom.',
  },
  {
    id: 'lite-kill-subagent',
    engines: ['v2'], // kill-subagent UI dropped on v3 (kas)
    text: 'With the Ctrl+O inspect panel open, press Ctrl+X twice to kill a running subagent.',
  },
  {
    id: 'lite-verbosity-preview',
    text: 'Inside /verbosity, Ctrl+P toggles a live preview and p expands it so you can see a change before committing.',
  },
  {
    id: 'lite-density-preset',
    text: 'Pick a density preset (minimal, lean, default, full) in /verbosity to reset every output knob in one step.',
  },
  {
    id: 'lite-show-output',
    text: "If a tool's output isn't showing, /settings → verbosity → Show output controls which categories surface.",
  },
  {
    id: 'lite-queue-edit',
    text: 'After queueing a message, ↑ pulls it back into the input to edit; empty + Enter deletes the slot.',
  },
];

const flagEnabled = (flag: string): boolean => process.env[flag] === '1';

/** Eligible tips for a surface, after applying each tip's flag/engine/when gate. */
function eligibleTips(ctx: TipContext): TipDef[] {
  const pool =
    ctx.surface === 'tui'
      ? [...SHARED, ...TUI_ONLY]
      : [...SHARED, ...LITE_ONLY];
  return pool.filter(
    (t) =>
      (!t.requiresFlag || flagEnabled(t.requiresFlag)) &&
      (!t.engines || t.engines.includes(ctx.engine)) &&
      (!t.when || t.when(ctx))
  );
}

const hasChance = (t: TipDef): boolean =>
  typeof t.chance === 'number' && t.chance > 0;

function resolveText(t: TipDef, ctx: TipContext): string {
  return applyGlyphs(typeof t.text === 'function' ? t.text(ctx) : t.text);
}

/**
 * Pick one tip via weighted random: featured tips (`chance`) take their declared
 * probability, plain tips split the remainder uniformly. Returns the tip text,
 * or undefined when nothing is eligible.
 */
export function pickTip(
  ctx: TipContext,
  rng: () => number = Math.random
): string | undefined {
  const tips = eligibleTips(ctx);
  if (tips.length === 0) return undefined;

  const featured = tips.filter(hasChance);
  const plain = tips.filter((t) => !hasChance(t));
  const featuredSum = featured.reduce((s, t) => s + (t.chance ?? 0), 0);

  const weighted: Array<{ tip: TipDef; weight: number }> = [];
  if (featuredSum >= 1) {
    // Featured saturate the probability mass: normalize among them; plain = 0.
    for (const t of featured)
      weighted.push({ tip: t, weight: (t.chance ?? 0) / featuredSum });
  } else {
    for (const t of featured) weighted.push({ tip: t, weight: t.chance ?? 0 });
    const remainder = 1 - featuredSum;
    if (plain.length > 0) {
      const each = remainder / plain.length;
      for (const t of plain) weighted.push({ tip: t, weight: each });
    } else {
      // No plain tips to absorb the remainder — hand it back to featured.
      for (const w of weighted)
        w.weight += ((w.tip.chance ?? 0) / featuredSum) * remainder;
    }
  }

  const total = weighted.reduce((s, w) => s + w.weight, 0) || 1;
  let roll = rng() * total;
  for (const w of weighted) {
    roll -= w.weight;
    if (roll < 0) return resolveText(w.tip, ctx);
  }
  // Floating-point fallthrough: return the last eligible tip.
  return resolveText(weighted[weighted.length - 1]!.tip, ctx);
}

/** Format a tip for the Lite welcome banner: dim+bold `Tip:` prefix, dim body. */
export function formatTipLine(tip: string): string {
  return `${chalk.dim.bold('  Tip:')} ${chalk.dim(tip)}`;
}

/** @internal Test-only access to the grouped tip lists. */
export const __TIPS_FOR_TESTS = { SHARED, TUI_ONLY, LITE_ONLY };
