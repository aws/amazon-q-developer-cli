import { Chalk, type ColorSupportLevel } from 'chalk';

export const KIRO_TUI_FORCE_COLOR = 'KIRO_TUI_FORCE_COLOR';

/**
 * Resolve the color level the launcher delivered on KIRO_TUI_FORCE_COLOR.
 * Returns `undefined` when the variable is absent or malformed, leaving chalk
 * to auto-detect from stdout. FORCE_COLOR is intentionally ignored: the TUI
 * forwards its environment to the tools it spawns, which must not inherit a
 * forced color level.
 */
export function resolveForcedLevel(
  env: NodeJS.ProcessEnv = process.env
): ColorSupportLevel | undefined {
  const forced = env[KIRO_TUI_FORCE_COLOR];
  return forced !== undefined && /^[0-3]$/.test(forced)
    ? (Number(forced) as ColorSupportLevel)
    : undefined;
}

const forcedLevel = resolveForcedLevel();

/**
 * The TUI's shared chalk instance - import this instead of the `chalk`
 * package. The level is fixed here, before any importer's body runs, so every
 * call site shares one color level and stylers built at module scope bake the
 * correct escapes regardless of import order.
 */
export const chalk =
  forcedLevel === undefined ? new Chalk() : new Chalk({ level: forcedLevel });
