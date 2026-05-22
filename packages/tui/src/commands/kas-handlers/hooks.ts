import { logger } from '../../utils/logger';
import type { HookInfo } from '../../stores/app-store';
import type { CommandContext } from '../types';
import type { KasCommand } from '../../kas-commands';
import type { DispatchOptions } from '../dispatcher';

export async function handleHooks(
  _cmd: KasCommand,
  _args: string,
  ctx: CommandContext,
  _options?: DispatchOptions
): Promise<void> {
  const result = await ctx.kiro.executeCommand({
    command: 'hooks',
    args: {},
  } as any);

  if (!result.success) {
    ctx.showAlert(result.message || 'Unable to fetch hooks', 'error', 5000);
    return;
  }

  const data = result.data as { hooks?: HookInfo[] } | undefined;
  const hooks = data?.hooks ?? [];
  ctx.setShowHooksPanel(true, hooks);
}
