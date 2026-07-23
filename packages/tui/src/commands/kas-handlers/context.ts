import { extractRpcErrorMessage } from '../../utils/error-handling';
import type {
  KasContextEntry,
  KasContextShowResponse,
} from '../../types/session-client';
import type { CommandContext } from '../types';
import type { KasCommand } from '../../kas-commands';
import type { DispatchOptions } from '../dispatcher';
import { getActiveGlyphs } from '../../hooks/useGlyphs';

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
  const tokens = shellSplit(args);
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

  // Cloud sessions: add/remove/clear mutate the context with LOCAL file
  // paths, which mean nothing inside the sandbox — refuse up front instead
  // of failing oddly. `show` stays available (read-only). Strictly
  // cloud-gated (dark-ship): local sessions never reach this branch.
  if (ctx.cloudSessionActive) {
    ctx.showAlert(
      `/context ${subcommand} is not available for a cloud session yet.`,
      'error',
      5000
    );
    return;
  }

  if (subcommand === 'clear') {
    return runMutation(ctx, () => ctx.kiro.contextClear(), 'clear');
  }

  // add / remove — each take one or more paths; --force/-f is honoured for add.
  let force = false;
  const paths: string[] = [];
  for (const tok of tokens.slice(1)) {
    if (tok === '--force' || tok === '-f') {
      force = true;
    } else {
      // shellSplit already resolved quotes/escapes — use the token as-is,
      // matching Rust (shell_split → add_resource, no re-strip). Re-stripping
      // would eat quote chars that are part of the literal filename.
      paths.push(tok);
    }
  }
  if (paths.length === 0) {
    ctx.showAlert(`Usage: /context ${subcommand} <path>...`, 'error', 5000);
    return;
  }

  if (subcommand === 'add') {
    return runMultiMutation(
      ctx,
      paths,
      (p) => ctx.kiro.contextAdd(p, { force }),
      'Added'
    );
  }
  return runMultiMutation(
    ctx,
    paths,
    (p) => ctx.kiro.contextRemove(p),
    'Removed'
  );
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
  const breakdown = response.breakdown ?? ctx.getContextBreakdownCache();
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

/**
 * add/remove flow: run the mutation once per path (the RPC takes a
 * single path) and aggregate. Mirrors the V2 Rust UX — one path names
 * itself, many collapse to a count — and surfaces any per-path failures.
 */
async function runMultiMutation(
  ctx: CommandContext,
  paths: string[],
  call: (path: string) => Promise<{ success?: boolean; message?: string }>,
  verb: 'Added' | 'Removed'
): Promise<void> {
  let ok = 0;
  const failures: string[] = [];
  for (const path of paths) {
    try {
      const res = await call(path);
      if (res.success === false) {
        failures.push(res.message || `'${path}'`);
      } else {
        ok += 1;
      }
    } catch (err) {
      failures.push(extractRpcErrorMessage(err, `'${path}'`));
    }
  }

  const preposition = verb === 'Added' ? 'to' : 'from';
  if (failures.length > 0) {
    // Partial success: name the paths that DID mutate so the user isn't left
    // thinking the whole command failed when earlier paths already applied.
    const prefix =
      ok > 0 ? `${verb} ${ok} path(s) ${preposition} context. ` : '';
    ctx.showAlert(`${prefix}Failed: ${failures.join('; ')}`, 'error', 5000);
    return;
  }
  const summary =
    paths.length === 1
      ? `${verb} '${paths[0]}' ${preposition} context`
      : `${verb} ${ok} path(s) ${preposition} context`;
  ctx.showAlert(summary, 'success', 3000);
}

/**
 * Split a slash-command argument string into tokens, respecting quotes
 * and backslash escapes so paths with spaces survive as one token.
 * Mirrors V2's Rust `shell_split` — globs (`*`, `?`, `[`) pass through
 * literally for the agent to expand.
 */
function shellSplit(input: string): string[] {
  const tokens: string[] = [];
  let cur = '';
  let quote: '"' | "'" | null = null;
  let has = false;
  for (let i = 0; i < input.length; i++) {
    const c = input.charAt(i);
    if (quote) {
      if (c === quote) quote = null;
      else cur += c;
    } else if (c === '"' || c === "'") {
      quote = c;
      has = true;
    } else if (c === '\\' && i + 1 < input.length) {
      cur += input.charAt(++i);
      has = true;
    } else if (/\s/.test(c)) {
      if (has) tokens.push(cur);
      cur = '';
      has = false;
    } else {
      cur += c;
      has = true;
    }
  }
  if (has) tokens.push(cur);
  return tokens;
}
