/**
 * Registry of /config subcommands — the /config twin of
 * `settings-subcommands.ts`.
 *
 * Each category the /config panel lists is also reachable as a typed
 * subcommand (`/config mcp`, `/config steering`, …), and selecting a row in
 * the panel dispatches through the SAME handler as the typed form, so the
 * two entry points cannot drift. Handlers either:
 *
 *   - hand off to a shared panel (/mcp, /hooks) with `configReturnOnEscape`
 *     primed so ESC walks back to the /config table (mirroring how
 *     /settings theme primes `settingsReturnOnEscape`), or
 *   - open the ConfigPanel on an in-panel category page.
 *
 * To add a new category, add an entry here and to the model's
 * `CONFIG_SUBCOMMANDS` — no dispatcher or effect changes needed.
 *
 * The whole surface is dark-shipped behind `Feature.CloudConfig`; /config is
 * only registered when the flag is on.
 */

import type { CommandContext } from './types.js';
import type { HookInfo } from '../stores/app-store.js';
import {
  resolveConfigSubcommand,
  CONFIG_MCP_LOADER,
  CONFIG_HOOKS_LOADER,
  type ConfigCategoryId,
} from '../components/ui/config-panel-model.js';
import { recordTuiConfigPanel } from '../utils/tui-telemetry-observer.js';
import { getCliVersion } from '../utils/version.js';

export interface ConfigHandleContext {
  ctx: CommandContext;
}

export interface ConfigSubcommand {
  /** Category id, doubling as the `/config <value>` token. */
  value: ConfigCategoryId;
  /** Dispatch logic for this subcommand. */
  handle: (h: ConfigHandleContext) => void | Promise<void>;
}

/**
 * Category-view usage counter for the routed (mcp/hooks) categories. Page
 * categories are counted by the store's setShowConfigPanel instead, so every
 * category view is counted exactly once regardless of entry point.
 */
function recordRoutedCategory(ctx: CommandContext, category: 'mcp' | 'hooks') {
  recordTuiConfigPanel({
    category,
    version: getCliVersion(),
    engine: ctx.agentEngine === 'kas' ? 'v3' : 'v2',
  });
}

/**
 * Open the shared /mcp view (KAS cache panel or V2 RPC — same as /mcp).
 * The ESC-back flag is primed unconditionally, exactly as every /settings
 * subcommand handler primes settingsReturnOnEscape: whether reached by
 * typing `/config mcp` or selecting the panel row, ESC returns to /config.
 * Bare /mcp never routes through here, so its ESC still closes outright.
 */
async function openMcp({ ctx }: ConfigHandleContext) {
  // Row-select path: the panel bumped the handoff token before dispatching.
  // Capture it on entry and compare by IDENTITY after the await — a match
  // means this handoff still owns the panel; a change means ESC-cancelled
  // (token 0) or superseded by a later selection (token bumped), and the
  // routed panel must not open. Typed `/config mcp` has token 0 throughout,
  // so ownsHandoff is false and the guard is a no-op there.
  const myToken = ctx.getConfigHandoffToken();
  const ownsHandoff = myToken !== 0;
  try {
    ctx.setConfigReturnOnEscape(true);
    if (ctx.agentEngine === 'kas') {
      ctx.setShowMcpPanel(true, [...ctx.mcpServerCache], 'list');
      // Close /config AFTER the routed panel is open (row-select path; no-op
      // for the typed form) so a paused message queue never sees a no-panel
      // window to drain into mid-navigation.
      ctx.setShowConfigPanel(false);
      recordRoutedCategory(ctx, 'mcp');
      return;
    }
    let result;
    // Feedback for the RPC window — claim-only, so an out-of-band owner's
    // spinner (compaction) is never overwritten; cleared only if still ours.
    const loader = CONFIG_MCP_LOADER;
    const claimed = ctx.claimLoadingMessage(loader);
    try {
      result = await ctx.kiro.executeCommand({
        command: 'mcp',
        args: {},
      });
    } catch (error) {
      // No panel opened: unset the flag, or the NEXT plain /mcp would title
      // itself '/config — MCP' and ESC would bounce into the category table.
      ctx.setConfigReturnOnEscape(false);
      throw error;
    } finally {
      if (claimed) ctx.clearLoadingMessage(loader);
    }
    if (ownsHandoff && ctx.getConfigHandoffToken() !== myToken) {
      // ESC-cancelled or superseded mid-RPC: /config already handled by the
      // owner of the change; do not open a panel over the abandoned nav.
      ctx.setConfigReturnOnEscape(false);
      return;
    }
    // Same payload contract as the /mcp effect — registryServers included,
    // so the "same view" promise holds for registry rows too.
    const data = result?.data as
      | { servers?: unknown[]; registryServers?: unknown[]; mode?: string }
      | undefined;
    ctx.setShowMcpPanel(
      true,
      (data?.servers as never[]) ?? [],
      data?.mode ?? 'list',
      data?.registryServers as never[] | undefined
    );
    ctx.setShowConfigPanel(false);
    // Counted only once a panel actually opened — a rejected RPC above must
    // not record a view the user never saw.
    recordRoutedCategory(ctx, 'mcp');
  } finally {
    // End the handoff only if it's still ours — a superseding selection owns
    // the token now and its own finally will clear it.
    if (ownsHandoff && ctx.getConfigHandoffToken() === myToken) {
      ctx.endConfigHandoff();
    }
  }
}

/**
 * Open the existing /hooks panel (ESC-back primed — see openMcp). Mirrors
 * the /hooks handler's fetch rule: the warm cache is local-fed, so cloud
 * sessions re-fetch the sandbox's authoritative hooks; an empty cache
 * re-fetches too.
 */
async function openHooks({ ctx }: ConfigHandleContext) {
  const myToken = ctx.getConfigHandoffToken(); // see openMcp
  const ownsHandoff = myToken !== 0;
  try {
    ctx.setConfigReturnOnEscape(true);
    // Warm-cache shortcut is KAS-only: _kiro/hooks/didChange keeps that
    // cache fresh. V2 has no such push (plain V2 /hooks always re-fetches),
    // and a cloud session's warm cache may be local-fed — both must
    // re-fetch.
    if (
      ctx.agentEngine === 'kas' &&
      !ctx.cloudSessionActive &&
      ctx.hooksList.length > 0
    ) {
      ctx.setShowHooksPanel(true, [...ctx.hooksList]);
      ctx.setShowConfigPanel(false); // after the routed panel opens — see openMcp
      recordRoutedCategory(ctx, 'hooks');
      return;
    }
    let result;
    const loader = CONFIG_HOOKS_LOADER;
    const claimed = ctx.claimLoadingMessage(loader);
    try {
      result = await ctx.kiro.executeCommand({
        command: 'hooks',
        args: {},
      } as never);
    } catch (error) {
      ctx.setConfigReturnOnEscape(false);
      throw error;
    } finally {
      if (claimed) ctx.clearLoadingMessage(loader);
    }
    if (ownsHandoff && ctx.getConfigHandoffToken() !== myToken) {
      ctx.setConfigReturnOnEscape(false); // ESC-cancelled or superseded mid-RPC
      return;
    }
    if (!result.success) {
      // No panel opened — same flag hygiene as the rejection path.
      ctx.setConfigReturnOnEscape(false);
      ctx.showAlert(result.message || 'Unable to fetch hooks', 'error', 5000);
      return;
    }
    const data = result.data as { hooks?: HookInfo[] } | undefined;
    ctx.setShowHooksPanel(true, data?.hooks ?? []);
    ctx.setShowConfigPanel(false);
    recordRoutedCategory(ctx, 'hooks');
  } finally {
    if (ownsHandoff && ctx.getConfigHandoffToken() === myToken) {
      ctx.endConfigHandoff();
    }
  }
}

/** Open the ConfigPanel on an in-panel category page. */
function openPage(category: ConfigCategoryId) {
  return ({ ctx }: ConfigHandleContext) => {
    ctx.setShowConfigPanel(true, category);
  };
}

export const configSubcommands: readonly ConfigSubcommand[] = [
  { value: 'agents', handle: openPage('agents') },
  { value: 'mcp', handle: openMcp },
  { value: 'powers', handle: openPage('powers') },
  { value: 'steering', handle: openPage('steering') },
  { value: 'skills', handle: openPage('skills') },
  { value: 'hooks', handle: openHooks },
  { value: 'env', handle: openPage('env') },
  // 'secrets' is deferred entirely (not p0): no row, no page, and the typed
  // token gets the "Unknown config category" alert via the model's resolver.
];

/**
 * Look up a subcommand handler from a raw `/config <token>` argument
 * (aliases and case handled by the model's resolver).
 */
export function findConfigSubcommand(
  token: string
): ConfigSubcommand | undefined {
  const category = resolveConfigSubcommand(token);
  if (!category) return undefined;
  return configSubcommands.find((s) => s.value === category);
}
