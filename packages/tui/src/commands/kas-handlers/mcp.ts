import type { KasCommand } from '../../kas-commands';
import type { DispatchOptions } from '../dispatcher';
import type { CommandContext } from '../types';

export async function handleMcp(
  cmd: KasCommand,
  args: string,
  ctx: CommandContext,
  _options?: DispatchOptions
): Promise<void> {
  const value = args.trim();
  if (!value) {
    ctx.setActiveCommand({ command: cmd, options: [] });
  }

  const servers = [...ctx.mcpServerCache];
  const isRegistryList = value === 'list';

  ctx.setShowMcpPanel(
    true,
    servers,
    'list',
    isRegistryList ? [...ctx.mcpRegistryCache] : undefined
  );

  if (value) {
    const message = isRegistryList
      ? `${servers.length} configured, ${ctx.mcpRegistryCache.length} registry servers`
      : `${servers.length} configured server${servers.length === 1 ? '' : 's'}`;
    ctx.showAlert(message, 'success', 5000);
  }
}
