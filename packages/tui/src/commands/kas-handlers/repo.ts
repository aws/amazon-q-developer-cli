import type { CommandContext } from '../types';
import type { KasCommand } from '../../kas-commands';
import type { DispatchOptions } from '../dispatcher';
import {
  normalizeRepoArg,
  formatCloneRepoInstruction,
} from '../../utils/repo-attach';

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

  const source = ctx.kiro.getRepoProviderSource();
  const list = await source.listSourceProviders();
  if (!list) {
    ctx.showAlert(
      'The repository picker is unavailable in this session.',
      'error',
      5000
    );
    return;
  }

  const connected = list.providers.find(
    (p) => p.connectionStatus === 'connected'
  );
  if (!connected) {
    const handoff = list.providers.find(
      (p) => p.connectionStatus === 'not_connected'
    );
    const setupUrl =
      handoff && handoff.connectionStatus === 'not_connected'
        ? handoff.setupUrl
        : undefined;
    ctx.showAlert(
      setupUrl
        ? `Connect a source provider to attach a repository: ${setupUrl}`
        : 'No connected source provider. Connect one in the Kiro web portal, then try again.',
      'warning',
      5000
    );
    return;
  }

  const page = await source.listSourceProviderResources({
    providerType: connected.providerType,
  });
  if (!page || page.resources.length === 0) {
    ctx.showAlert('No repositories available to attach.', 'warning', 5000);
    return;
  }

  ctx.setShowRepoPicker(true, page.resources);
}
