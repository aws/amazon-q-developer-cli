/**
 * Registry of /config subcommands.
 *
 * Each category the /config panel lists is also reachable as a typed
 * subcommand (`/config mcp`, `/config steering`, …), and selecting a row in
 * the panel dispatches through the SAME handler as the typed form, so the
 * two entry points cannot drift.
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
function recordRoutedCategory(category: 'mcp' | 'hooks') {
  // engine omitted: /config is KAS-only, the recorder defaults to v3.
  recordTuiConfigPanel({
    category,
    version: getCliVersion(),
  });
}

/**
 * Open the shared /mcp view. /config is KAS-only, so the KAS cache branch is
 * the live path; the RPC branch below is defensive (kept unit-tested) should
 * the command ever dispatch on another engine. The ESC-back flag is primed
 * unconditionally: whether reached by typing `/config mcp` or selecting the
 * panel row, ESC returns to /config. Bare /mcp never routes through here, so
 * its ESC still closes outright.
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
      recordRoutedCategory('mcp');
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
    // registryServers is forwarded so registry rows render here exactly as
    // they do under bare /mcp.
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
    recordRoutedCategory('mcp');
  } finally {
    // End the handoff only if it's still ours — a superseding selection owns
    // the token now and its own finally will clear it.
    if (ownsHandoff && ctx.getConfigHandoffToken() === myToken) {
      ctx.endConfigHandoff();
    }
  }
}

/**
 * Open the shared /hooks view with ESC-back to /config primed. The warm
 * cache is local-fed, so cloud sessions re-fetch the sandbox's authoritative
 * hooks; an empty cache re-fetches too.
 */
async function openHooks({ ctx }: ConfigHandleContext) {
  // Nonzero token = row-select handoff; compared by identity after awaits.
  const myToken = ctx.getConfigHandoffToken();
  const ownsHandoff = myToken !== 0;
  try {
    ctx.setConfigReturnOnEscape(true);
    // Warm-cache shortcut: _kiro/hooks/didChange keeps this cache fresh on
    // KAS (the only engine /config registers on; the engine check is
    // defensive). A cloud session's warm cache may be local-fed, so cloud
    // and cold caches re-fetch the authoritative listing.
    if (
      ctx.agentEngine === 'kas' &&
      !ctx.cloudSessionActive &&
      ctx.hooksList.length > 0
    ) {
      ctx.setShowHooksPanel(true, [...ctx.hooksList]);
      // Close /config only after the routed panel is open, so a paused
      // message queue never sees a no-panel window to drain into.
      ctx.setShowConfigPanel(false);
      recordRoutedCategory('hooks');
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
      // No panel opened: unset the flag so the next plain /hooks is
      // unaffected.
      ctx.setConfigReturnOnEscape(false);
      ctx.showAlert(result.message || 'Unable to fetch hooks', 'error', 5000);
      return;
    }
    const data = result.data as { hooks?: HookInfo[] } | undefined;
    ctx.setShowHooksPanel(true, data?.hooks ?? []);
    ctx.setShowConfigPanel(false);
    recordRoutedCategory('hooks');
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

/**
 * Open the selectable /agent picker — the agents row is not a read-only
 * page; it routes to the same picker as /agent so the user can switch right
 * there.
 */
async function openAgents({ ctx }: ConfigHandleContext) {
  // Nonzero token = row-select handoff; compared by identity after awaits.
  const myToken = ctx.getConfigHandoffToken();
  const ownsHandoff = myToken !== 0;
  try {
    const agentCmd = ctx.kasCommands.find((c) => c.name === '/agent');
    if (!agentCmd) {
      ctx.showAlert('Agent picker unavailable', 'error', 3000);
      return;
    }
    const { handleAgent } = await import('./kas-handlers/agent.js');
    if (ownsHandoff && ctx.getConfigHandoffToken() !== myToken) {
      // ESC-cancelled or superseded during the import: the abandoned nav
      // must not open the picker or close /config.
      return;
    }
    // The picker opens only for a non-empty agent list; an empty one alerts
    // instead, and /config must stay open under that alert — no close, no
    // view count, no ESC-back stash to leak into the next menu.
    const pickerWillOpen = ctx.kasAvailableAgents.length > 0;
    // ESC-back to /config. Bare /agent never routes through here, so its
    // ESC still closes outright.
    if (pickerWillOpen) {
      ctx.setConfigReturnOnEscape(true);
    }
    await handleAgent(agentCmd, '', ctx);
    if (!pickerWillOpen) return;
    // Close /config only after the picker is open, so a paused message
    // queue never sees a no-panel window to drain into.
    ctx.setShowConfigPanel(false);
    recordTuiConfigPanel({ category: 'agents', version: getCliVersion() });
  } finally {
    if (ownsHandoff && ctx.getConfigHandoffToken() === myToken) {
      ctx.endConfigHandoff();
    }
  }
}

export const configSubcommands: readonly ConfigSubcommand[] = [
  { value: 'agents', handle: openAgents },
  { value: 'mcp', handle: openMcp },
  { value: 'powers', handle: openPage('powers') },
  { value: 'steering', handle: openPage('steering') },
  { value: 'skills', handle: openPage('skills') },
  { value: 'hooks', handle: openHooks },
  // 'env' and 'secrets' are deferred entirely (KAS propagates no source for
  // either): no row, no page, and the typed token gets the "Unknown config
  // category" alert.
];

/**
 * Look up a subcommand handler from a raw `/config <token>` argument;
 * aliases and casing are normalized first.
 */
export function findConfigSubcommand(
  token: string
): ConfigSubcommand | undefined {
  const category = resolveConfigSubcommand(token);
  if (!category) return undefined;
  return configSubcommands.find((s) => s.value === category);
}
