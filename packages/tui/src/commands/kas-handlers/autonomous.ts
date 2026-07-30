import type { CommandContext } from '../types';
import type { KasCommand } from '../../kas-commands';
import {
  KAS_AUTONOMOUS_AGENT_ID,
  KAS_DEFAULT_AGENT_ID,
} from '../../constants/agents';
import { extractRpcErrorMessage } from '../../utils/error-handling';

/**
 * `/autonomous on|off` for KAS cloud sessions. Autonomous mode is the bundled
 * `autonomous` KAS mode (hidden from the `/agent` picker): turning it on
 * switches the session mode via ACP `session/set_mode`
 * (`ctx.kiro.setSessionMode`), and turning it off switches back to the
 * default agent. `session/set_mode` returns an empty response, KAS emits no
 * `current_mode_update` after it, and KAS silently no-ops the verb for
 * relayed sessions — so the KAS client verifies the switch by reading the
 * current mode back and rejects (no state change, no chip) when the server
 * did not apply it. On verified success the client broadcasts AgentSwitched,
 * so the store's current agent is already updated by the time the success
 * line is emitted. The active state is derived from the store's current
 * agent, so server-pushed mode updates and `/agent` switches keep it
 * consistent without extra bookkeeping. With no (or an invalid) argument, a
 * selection picker offers `on` / `off` with the current state tagged
 * `[current]`.
 */
export async function handleAutonomous(
  cmd: KasCommand,
  args: string,
  ctx: CommandContext
): Promise<void> {
  if (!ctx.cloudSessionActive) return;

  const arg = args.trim().toLowerCase();
  const isActive = () =>
    ctx.getCurrentAgent?.()?.name === KAS_AUTONOMOUS_AGENT_ID;

  if (arg !== 'on' && arg !== 'off') {
    return showAutonomousPicker(ctx, cmd, isActive());
  }

  if (arg === 'on') {
    if (isActive()) {
      ctx.addSystemMessage('Autonomous mode is already on', true);
      return;
    }
    const previousAgent = ctx.getCurrentAgent?.()?.name;
    if (!(await switchMode(ctx, KAS_AUTONOMOUS_AGENT_ID))) return;
    ctx.addSystemMessage(
      previousAgent && previousAgent !== KAS_DEFAULT_AGENT_ID
        ? 'Autonomous mode on, agent switched to Kiro Default'
        : 'Autonomous mode on',
      true
    );
    return;
  }

  if (!isActive()) {
    ctx.addSystemMessage('Autonomous mode is already off', true);
    return;
  }
  if (!(await switchMode(ctx, KAS_DEFAULT_AGENT_ID))) return;
  ctx.addSystemMessage('Autonomous mode off', true);
}

/**
 * Open the on/off picker with the currently-active state tagged `[current]`.
 * `executeOnSelect` makes a pick execute immediately (routing back through
 * `executeCommandWithArg` into this handler) instead of taking the
 * Tab-subcommand prefill path these option values would otherwise match.
 */
function showAutonomousPicker(
  ctx: CommandContext,
  cmd: KasCommand,
  active: boolean
): void {
  const options = [
    { value: 'on', label: 'on', description: active ? '[current]' : '' },
    { value: 'off', label: 'off', description: active ? '' : '[current]' },
  ];
  ctx.setActiveCommand({ command: cmd, options, executeOnSelect: true });
}

/**
 * Switch the session mode via `session/set_mode`. The KAS client verifies
 * the switch with a read-back (see `KasAcpClient.setSessionMode`) and
 * rejects when the server did not apply it, so a resolved call is evidence
 * the mode actually changed. Returns false — after surfacing the error —
 * when the RPC rejects or verification fails.
 */
async function switchMode(
  ctx: CommandContext,
  agentName: string
): Promise<boolean> {
  ctx.setLoadingMessage(
    agentName === KAS_AUTONOMOUS_AGENT_ID
      ? 'Turning autonomous mode on'
      : 'Turning autonomous mode off'
  );
  try {
    await ctx.kiro.setSessionMode(agentName);
  } catch (err) {
    ctx.setLoadingMessage(null);
    ctx.showAlert(
      extractRpcErrorMessage(err, 'Failed to switch autonomous mode'),
      'error',
      5000
    );
    return false;
  }
  ctx.setLoadingMessage(null);
  return true;
}
