import { extractRpcErrorMessage } from '../../utils/error-handling';
import type { CommandContext } from '../types';
import type { KasCommand } from '../../kas-commands';
import type { DispatchOptions } from '../dispatcher';

export async function handleCompact(
  _cmd: KasCommand,
  args: string,
  ctx: CommandContext,
  _options?: DispatchOptions
): Promise<void> {
  const result = await ctx.kiro.executeCommand({
    command: 'compact',
    args: { ...(args.trim() && { value: args.trim() }) },
  } as any);
  if (!result.success) {
    ctx.showAlert(
      extractRpcErrorMessage(result.message, 'Compaction failed'),
      'error',
      5000
    );
  }
}
