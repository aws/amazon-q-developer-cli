import { extractRpcErrorMessage } from '../../utils/error-handling';
import type {
  KasContextEntry,
  KasContextShowResponse,
} from '../../types/session-client';
import type { CommandContext } from '../types';
import type { KasCommand } from '../../kas-commands';
import type { DispatchOptions } from '../dispatcher';
import { getActiveGlyphs } from '../../hooks/useGlyphs';
import { unquote } from '../../utils/string';

/**
 * KAS-mode dispatch handler for `/context`.
 *
 * Owns all wire-format parsing for the slash command (subcommand
 * normalisation, `rm` → `remove` alias, `--force`/`-f` flag, quoted-path
 * stripping). Dispatches to the typed `ctx.kiro.context*` methods, then
 * presents the result either as the breakdown panel or as a transient
 * alert. The client (`KasAcpClient`) stays a thin type-safe wrapper —
 * see `_kiro/session/context` in `@kiro/acp-type-covenant`.
 */
export async function handleContext(
  _cmd: KasCommand,
  args: string,
  ctx: CommandContext,
  _options?: DispatchOptions
): Promise<void> {
  const trimmed = args.trim();
  const tokens = trimmed.length === 0 ? [] : trimmed.split(/\s+/);
  const rawSub = (tokens[0] ?? '').toLowerCase();
  const explicitShow = rawSub === 'show';

  // Aliases: bare `/context` → show; `rm` → remove. Matches V2 Rust
  // behaviour at crates/chat-cli-v2/.../commands/context.rs.
  const subcommand: 'show' | 'add' | 'remove' | 'clear' | 'unknown' =
    rawSub === '' || rawSub === 'show'
      ? 'show'
      : rawSub === 'rm'
        ? 'remove'
        : rawSub === 'add' || rawSub === 'remove' || rawSub === 'clear'
          ? rawSub
          : 'unknown';

  if (subcommand === 'unknown') {
    ctx.showAlert(
      `Unknown /context subcommand '${tokens[0]}'. Use: show, add, remove, clear`,
      'error',
      5000
    );
    return;
  }

  if (subcommand === 'show') {
    return runShow(ctx, explicitShow);
  }

  if (subcommand === 'clear') {
    return runMutation(ctx, () => ctx.kiro.contextClear(), 'clear');
  }

  // add / remove — both take a path; --force/-f is honoured for add.
  let force = false;
  const positional: string[] = [];
  for (const tok of tokens.slice(1)) {
    if (tok === '--force' || tok === '-f') {
      force = true;
    } else {
      positional.push(tok);
    }
  }
  const pathArg = unquote(positional.join(' ').trim());
  if (pathArg.length === 0) {
    ctx.showAlert(`Usage: /context ${subcommand} <path>`, 'error', 5000);
    return;
  }

  if (subcommand === 'add') {
    return runMutation(
      ctx,
      () => ctx.kiro.contextAdd(pathArg, { force }),
      'add'
    );
  }
  return runMutation(ctx, () => ctx.kiro.contextRemove(pathArg), 'remove');
}

/**
 * `explicitShow=true` (user typed `/context show`) opens the panel
 * expanded; bare `/context` keeps it collapsed — matches V2 Rust.
 */
async function runShow(
  ctx: CommandContext,
  explicitShow: boolean
): Promise<void> {
  let response: KasContextShowResponse;
  try {
    response = await ctx.kiro.contextShow();
  } catch (err) {
    ctx.showAlert(
      extractRpcErrorMessage(err, 'Failed to load context'),
      'error',
      5000
    );
    return;
  }

  // Prefer the fresh show-response breakdown; the cache can be stale after
  // an /agent switch. Fall back to the cache for older agents.
  const breakdown = response.breakdown ?? ctx.kiro.getCachedContextBreakdown();
  if (breakdown) {
    ctx.setShowContextBreakdown(true, {
      ...breakdown,
      initialExpanded: explicitShow,
    });
    return;
  }

  // No breakdown at all — fall back to the attached-files alert.
  const entries: KasContextEntry[] = response.entries ?? [];
  if (entries.length === 0) {
    ctx.showAlert(
      response.message || 'No context files attached',
      'warning',
      3000
    );
    return;
  }
  const glyphs = getActiveGlyphs();
  const summary = entries
    .map((e) => `${e.matched === false ? `${glyphs.warning} ` : ''}${e.path}`)
    .join(', ');
  ctx.showAlert(
    `Context files: ${summary}`,
    entries.some((e) => e.matched === false) ? 'warning' : 'success',
    5000
  );
}

/**
 * Mutation flow (add/remove/clear): success surfaces as an alert with
 * the agent's message; agent-level failure (e.g. path-not-found, where
 * the inner success flag is false) shows an error tone.
 */
async function runMutation(
  ctx: CommandContext,
  call: () => Promise<{ success?: boolean; message?: string }>,
  label: string
): Promise<void> {
  let response: { success?: boolean; message?: string };
  try {
    response = await call();
  } catch (err) {
    ctx.showAlert(
      extractRpcErrorMessage(err, `/context ${label} failed`),
      'error',
      5000
    );
    return;
  }
  if (response.success === false) {
    ctx.showAlert(
      response.message || `/context ${label} failed`,
      'error',
      5000
    );
    return;
  }
  ctx.showAlert(response.message || 'Done', 'success', 3000);
}
