// One tip is shown per launch below the KIRO banner. Ephemeral in the TUI,
// baked into the banner in Lite. Pure module: callers build a TipContext and
// call pickTip; selection is stateless and random per launch (seed rng in tests).
import { chalk } from '../utils/color.js';

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
  {
    id: 'compact',
    text: 'Running low on context? Type /compact to summarize the conversation and free up space.',
  },
  {
    id: 'model-switch',
    text: 'Type /model to switch between AI models mid-conversation without starting over.',
  },
  {
    id: 'effort',
    text: 'Use /effort to adjust reasoning depth. Lower effort for quick answers, higher for complex tasks.',
  },
  {
    id: 'context-add',
    text: 'Use /context add <path> to attach files or folders so Kiro can reference them.',
  },
  {
    id: 'agent-switch',
    text: 'Type /agent to list available agents or switch to a specialized one.',
  },
  {
    id: 'chat-save',
    text: 'Save a conversation with /chat save <path> and reload it later with /chat load.',
  },
  {
    id: 'chat-new',
    text: 'Start fresh without quitting: /chat new begins a new session in the same window.',
  },
  {
    id: 'knowledge',
    text: 'Use /knowledge to manage knowledge bases that give Kiro persistent project context.',
  },
  {
    id: 'mcp-status',
    text: 'Type /mcp to check which MCP servers are connected and their status.',
  },
  {
    id: 'hooks',
    text: 'Use /hooks to see your configured automation hooks.',
  },
  {
    id: 'usage',
    text: 'Curious about your usage? Type /usage to see plan limits and billing info.',
  },
  {
    id: 'code-overview',
    text: 'Type /code overview to get a high-level map of your codebase structure.',
  },
  {
    id: 'tools-list',
    text: 'Type /tools to see all available tools Kiro can use in the current session.',
  },
  {
    id: 'changelog',
    text: 'Type /changelog to see what shipped in the latest releases.',
  },
  {
    id: 'settings',
    text: 'Type /settings to customize keybindings, display, terminal behavior, and more.',
  },
  {
    id: 'verbosity',
    chance: 0.05,
    requiresFlag: 'KIRO_LITE_ROLLOUT_ENABLED',
    text: 'Tune how much each tool call shows — args, reasoning, output caps, density — via /verbosity or /settings → verbosity.',
  },
  {
    id: 'goal',
    text: 'Use /goal <description> to set a persistent goal that keeps Kiro aligned across long, multi-turn tasks.',
  },
  {
    id: 'at-file',
    text: 'Type @ in your message to attach a file to context without copy-pasting.',
  },
  {
    id: 'editor',
    text: 'Use /editor to compose long prompts in your $EDITOR.',
  },
  {
    id: 'copy',
    text: 'Type /copy to copy the last response to your clipboard.',
  },
  {
    id: 'transcript',
    text: 'Use /transcript to open the full conversation in your $PAGER.',
  },
  {
    id: 'rewind',
    engines: ['kas'], // /rewind is v3-only (v2 uses /checkpoint)
    text: 'Made a wrong turn? Use /rewind to fork the conversation from any earlier point.',
  },
  {
    id: 'paste-image',
    text: 'Press Ctrl+V or run /paste to attach an image from your clipboard to the conversation.',
  },
  {
    id: 'spec',
    engines: ['kas'], // /spec is v3-only
    text: 'Type /spec new <name> to create a structured feature spec with requirements and tasks.',
  },
  {
    id: 'plan',
    text: 'Use /plan for structured spec generation mode when you want Kiro to plan before coding.',
  },
  {
    id: 'newline',
    text: 'Press Shift+Enter or Alt+Enter to insert a newline instead of sending your prompt.',
  },
  {
    id: 'shell-escape',
    text: 'Start your message with ! to run a shell command without leaving the chat; Ctrl+C cancels it.',
  },
  {
    id: 'reverse-search',
    text: 'Press Ctrl+R to reverse-search prompt history; press again for older matches, Esc accepts.',
  },
  {
    id: 'crew-monitor',
    text: 'Press Ctrl+G to open the full-screen monitor of subagent activity; Ctrl+G or q exits.',
  },
  {
    id: 'plan-mode',
    engines: ['v2'], // Shift+Tab agent-switch is a no-op on kas (v3)
    text: 'Press Shift+Tab to toggle plan mode; press it again to return to your previous agent.',
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
    id: 'tui-steer',
    text: 'Type while Kiro is working to steer it mid-turn (Ctrl+S switches to queue mode); Ctrl+X opens the tray to edit a queued message.',
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
    id: 'lite-queue-edit',
    text: 'Type while Kiro is working to steer it mid-turn (Ctrl+S switches to queue mode); ↑ pulls a queued message back into the input to edit.',
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
