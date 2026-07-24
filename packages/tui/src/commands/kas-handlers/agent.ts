import type { CommandContext } from '../types';
import type { KasCommand } from '../../kas-commands';
import { parseAgentSubcommand } from '../../acp-client';
import { getAgentDisplayName } from '../../utils/agentColors';
import { extractRpcErrorMessage } from '../../utils/error-handling';

/**
 * `/agent` list / switch for KAS. The available agents come from the
 * `kasAvailableAgents` store slice (parsed from the `mode` configOption,
 * normalized + filtered to the user-selectable set). Switching goes through
 * `ctx.kiro.setConfigOption('mode', …)`, which re-emits the normalized agent
 * events so the store + chip self-heal.
 */
export async function handleAgent(
  cmd: KasCommand,
  args: string,
  ctx: CommandContext
): Promise<void> {
  const parsed = parseAgentSubcommand({ value: args });
  switch (parsed.kind) {
    case 'list':
      return showAgentPicker(ctx, cmd);
    case 'swap':
      return switchAgent(ctx, parsed.name);
    case 'create':
      ctx.showAlert(
        '/agent create is not yet implemented in KAS mode',
        'error',
        5000
      );
      return;
    case 'edit':
      ctx.showAlert(
        '/agent edit is not yet implemented in KAS mode',
        'error',
        5000
      );
      return;
  }
}

function showAgentPicker(ctx: CommandContext, cmd: KasCommand): void {
  if (ctx.kasAvailableAgents.length === 0) {
    // Cloud: the sandbox owns the agent surface and pushes it over the
    // downlink shortly after attach — an empty list is a not-yet,
    // not a failure.
    if (ctx.cloudSessionActive) {
      ctx.showAlert(
        'Waiting for the sandbox to report its agents — try again in a moment',
        'warning',
        4000
      );
      return;
    }
    ctx.showAlert('No agents available', 'error', 3000);
    return;
  }
  const currentName = ctx.getCurrentAgent?.()?.name;
  const options = ctx.kasAvailableAgents.map((a) => {
    const isActive = a.id === currentName;
    const descBase = a.description ?? '';
    return {
      value: a.id,
      label: getAgentDisplayName(a.id, a.name),
      description: isActive
        ? `[active]${descBase ? ` ${descBase}` : ''}`
        : descBase,
      // Group by source (e.g. "Bundled", "Workspace") so the menu reflects
      // where each agent came from. Agents without source metadata fall into
      // the default (ungrouped) bucket.
      ...(a.source ? { group: capitalize(a.source) } : {}),
    };
  });
  ctx.setActiveCommand({ command: cmd, options });
}

async function switchAgent(
  ctx: CommandContext,
  agentName: string
): Promise<void> {
  ctx.setLoadingMessage(`Agent changing to ${agentName}`);
  try {
    await ctx.kiro.setConfigOption('mode', agentName);
  } catch (err) {
    ctx.setLoadingMessage(null);
    ctx.showAlert(
      extractRpcErrorMessage(err, 'Failed to switch agent'),
      'error',
      5000
    );
    return;
  }
  ctx.setLoadingMessage(null);
  if (ctx.getCurrentAgent?.()?.name !== agentName) {
    ctx.showAlert(`Agent '${agentName}' not available`, 'error', 5000);
    return;
  }
  ctx.showAlert(`Switched to ${agentName}`, 'success', 3000);
}

function capitalize(s: string): string {
  return s.length > 0 ? s[0]!.toUpperCase() + s.slice(1) : s;
}
