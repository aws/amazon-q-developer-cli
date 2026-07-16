import type { CommandContext } from '../types';
import type { KasCommand } from '../../kas-commands';
import type { DispatchOptions } from '../dispatcher';
import { quitCloudSessionKeepRunning } from '../../utils/cloud-detach-notice';

/**
 * `/disconnect` — detach from the current cloud session without stopping it.
 * The session keeps running on the sandbox; the notice tells the user how to
 * reattach. Same keep-running detach path as Ctrl+D and /quit's "keep
 * running", so it delegates to the shared helper (idempotent notice + close +
 * exit). The `exit` seam defaults to process.exit; tests inject a spy so the
 * detach sequence can be asserted without terminating the runner.
 */
export async function handleDisconnect(
  _cmd: KasCommand,
  _args: string,
  ctx: CommandContext,
  _options?: DispatchOptions,
  exit: (code: number) => void = process.exit
): Promise<void> {
  if (!ctx.cloudSessionActive) return;
  quitCloudSessionKeepRunning(ctx.kiro, exit);
}
