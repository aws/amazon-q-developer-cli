import type { CommandContext } from '../types';
import type { KasCommand } from '../../kas-commands';
import type { DispatchOptions } from '../dispatcher';

/**
 * KAS-mode dispatch handler for `/tools`.
 *
 * Opens the full-screen tools panel (matching the V2/Rust experience) from the
 * cached `toolsList`. Unlike `/hooks` (which fetches via `executeCommand`), the
 * tool listing is pushed by KAS through the `_kiro/tools/didChange`
 * notification and cached in the store's `toolsList` by the stream-event
 * handler, kept current on agent swaps / MCP changes. The panel re-renders
 * automatically if a later notification updates the list while it is open.
 *
 * KAS exposes tools as tags (built-in category tags + per-tool MCP
 * `@server/tool` tags) with no per-tool permission status, so the panel hides
 * its Status column for this data.
 */
export async function handleTools(
  _cmd: KasCommand,
  _args: string,
  ctx: CommandContext,
  _options?: DispatchOptions
): Promise<void> {
  ctx.setShowToolsPanel(true, [...ctx.toolsList]);
}
