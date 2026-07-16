import type { CommandContext } from '../types';
import type { KasCommand } from '../../kas-commands';
import type { DispatchOptions } from '../dispatcher';
import {
  normalizeRepoArg,
  formatCloneRepoInstruction,
  resolveSourceProviderConnection,
} from '../../utils/repo-attach';
import { dedupeRepoResources } from '../../utils/repo-multiselect';

/** `/repo [owner/name]` — attach a repo to the current cloud session, or open
 *  the picker when no arg is given. Cloud-only (early-returns otherwise). */
export async function handleRepo(
  _cmd: KasCommand,
  args: string,
  ctx: CommandContext,
  _options?: DispatchOptions
): Promise<void> {
  if (!ctx.cloudSessionActive) return;

  const direct = normalizeRepoArg(args);
  if (direct) {
    await ctx.sendMessage(formatCloneRepoInstruction(direct));
    return;
  }

  // The catalog fetch crosses KAS → BFF twice and can take seconds against a
  // real provider; without a status line the picker looks unresponsive.
  ctx.setLoadingMessage('Loading repositories...');
  const source = ctx.kiro.getRepoProviderSource();
  let list;
  try {
    list = await source.listSourceProviders();
  } finally {
    ctx.setLoadingMessage(null);
  }
  if (!list) {
    // Catalog unavailable (capability not advertised, or the provider catalog
    // errored server-side). Direct attach bypasses the catalog entirely, so
    // keep the user unblocked instead of dead-ending them.
    ctx.showAlert(
      'Repository list is unavailable right now. Attach directly with ' +
        '/repo <owner/name>, or manage providers at ' +
        'https://kiro.dev/settings/source-providers and try again.',
      'warning',
      8000
    );
    return;
  }

  const connected = list.providers.find(
    (p) => p.connectionStatus === 'connected'
  );
  if (!connected) {
    const { setupUrl } = resolveSourceProviderConnection(list);
    ctx.showAlert(
      setupUrl
        ? `Connect a source provider to attach a repository: ${setupUrl}`
        : 'No connected source provider. Connect one in the Kiro web portal, then try again.',
      'warning',
      6000
    );
    return;
  }

  ctx.setLoadingMessage('Loading repositories...');
  let page;
  try {
    page = await source.listSourceProviderResources({
      providerType: connected.providerType,
    });
  } finally {
    ctx.setLoadingMessage(null);
  }
  if (!page) {
    // Enumeration failed after a successful provider list — transient catalog
    // fault. Direct attach still works, so point there.
    ctx.showAlert(
      'Could not fetch the repository list. Attach directly with ' +
        '/repo <owner/name>, or try again shortly.',
      'warning',
      8000
    );
    return;
  }
  if (page.resources.length === 0) {
    ctx.showAlert('No repositories available to attach.', 'warning', 5000);
    return;
  }

  ctx.setShowRepoPicker(true, dedupeRepoResources(page.resources));
}
