import { describe, it, expect, mock } from 'bun:test';
import { handleAutonomous } from '../autonomous';
import { createMockCommandContext } from '../../__tests__/test-helpers';
import type { KasCommand } from '../../../kas-commands';
import { KasCommandName } from '../../../kas-commands';

const AUTONOMOUS_CMD: KasCommand = {
  name: KasCommandName.Autonomous,
  description: 'Turn autonomous mode on or off',
  meta: { cloudOnly: true, subcommands: ['on', 'off'] },
};

/**
 * A mock cloud-session context whose getCurrentAgent tracks setSessionMode,
 * mirroring the KAS client's verified read-back + AgentSwitched broadcast
 * after a mode switch the server actually applied.
 */
function createModeTrackingContext(initialAgent: string) {
  let agent = initialAgent;
  const setSessionMode = mock((modeId: string) => {
    agent = modeId;
    return Promise.resolve();
  });
  const ctx = createMockCommandContext({
    kiro: { setSessionMode } as any,
  });
  ctx.cloudSessionActive = true;
  ctx.getCurrentAgent = () => ({ name: agent });
  return { ctx, setSessionMode };
}

describe('handleAutonomous', () => {
  it('turns on from the default agent via session/set_mode and emits the plain on message', async () => {
    const { ctx, setSessionMode } = createModeTrackingContext('default');
    await handleAutonomous(AUTONOMOUS_CMD, 'on', ctx);
    expect(setSessionMode).toHaveBeenCalledWith('autonomous');
    expect(ctx._spies.addSystemMessage).toHaveBeenCalledWith(
      'Autonomous mode on',
      true
    );
  });

  it('turns on from a non-default agent and notes the switch to Kiro Default', async () => {
    const { ctx, setSessionMode } = createModeTrackingContext('spec');
    await handleAutonomous(AUTONOMOUS_CMD, 'on', ctx);
    expect(setSessionMode).toHaveBeenCalledWith('autonomous');
    expect(ctx._spies.addSystemMessage).toHaveBeenCalledWith(
      'Autonomous mode on, agent switched to Kiro Default',
      true
    );
  });

  it('turns off back to the default agent', async () => {
    const { ctx, setSessionMode } = createModeTrackingContext('autonomous');
    await handleAutonomous(AUTONOMOUS_CMD, 'off', ctx);
    expect(setSessionMode).toHaveBeenCalledWith('default');
    expect(ctx._spies.addSystemMessage).toHaveBeenCalledWith(
      'Autonomous mode off',
      true
    );
  });

  it('opens the on/off picker with [current] on off when inactive and no arg is given', async () => {
    const { ctx, setSessionMode } = createModeTrackingContext('default');
    await handleAutonomous(AUTONOMOUS_CMD, '', ctx);
    expect(setSessionMode).not.toHaveBeenCalled();
    expect(ctx._spies.showAlert).not.toHaveBeenCalled();
    const call = (ctx._spies.setActiveCommand as any).mock.calls[0][0];
    expect(call.command).toBe(AUTONOMOUS_CMD);
    expect(call.executeOnSelect).toBe(true);
    expect(call.options).toEqual([
      { value: 'on', label: 'on', description: '' },
      { value: 'off', label: 'off', description: '[current]' },
    ]);
  });

  it('opens the picker with [current] on on when active, including for an invalid arg', async () => {
    const { ctx, setSessionMode } = createModeTrackingContext('autonomous');
    await handleAutonomous(AUTONOMOUS_CMD, 'bogus', ctx);
    expect(setSessionMode).not.toHaveBeenCalled();
    const call = (ctx._spies.setActiveCommand as any).mock.calls[0][0];
    expect(call.executeOnSelect).toBe(true);
    expect(call.options).toEqual([
      { value: 'on', label: 'on', description: '[current]' },
      { value: 'off', label: 'off', description: '' },
    ]);
  });

  it('does not resend the mode change when already on', async () => {
    const { ctx, setSessionMode } = createModeTrackingContext('autonomous');
    await handleAutonomous(AUTONOMOUS_CMD, 'on', ctx);
    expect(setSessionMode).not.toHaveBeenCalled();
    expect(ctx._spies.addSystemMessage).toHaveBeenCalledWith(
      'Autonomous mode is already on',
      true
    );
  });

  it('does not resend the mode change when already off', async () => {
    const { ctx, setSessionMode } = createModeTrackingContext('default');
    await handleAutonomous(AUTONOMOUS_CMD, 'off', ctx);
    expect(setSessionMode).not.toHaveBeenCalled();
    expect(ctx._spies.addSystemMessage).toHaveBeenCalledWith(
      'Autonomous mode is already off',
      true
    );
  });

  it('no-ops entirely outside cloud sessions', async () => {
    const { ctx, setSessionMode } = createModeTrackingContext('default');
    ctx.cloudSessionActive = false;
    await handleAutonomous(AUTONOMOUS_CMD, 'on', ctx);
    await handleAutonomous(AUTONOMOUS_CMD, '', ctx);
    expect(setSessionMode).not.toHaveBeenCalled();
    expect(ctx._spies.setActiveCommand).not.toHaveBeenCalled();
    expect(ctx._spies.addSystemMessage).not.toHaveBeenCalled();
    expect(ctx._spies.showAlert).not.toHaveBeenCalled();
  });

  it('surfaces a verification failure (relayed no-op) with no success line', async () => {
    // The KAS client rejects when the read-back shows the server did not
    // apply the mode (relayed sessions silently no-op session/set_mode);
    // the handler must surface that error and emit nothing else.
    const ctx = createMockCommandContext({
      currentAgent: { name: 'default' },
      kiro: {
        setSessionMode: mock(() =>
          Promise.reject(
            new Error(
              'Switching modes is not supported on this session yet — it requires an updated Kiro agent server'
            )
          )
        ),
      } as any,
    });
    ctx.cloudSessionActive = true;
    await handleAutonomous(AUTONOMOUS_CMD, 'on', ctx);
    expect(ctx._spies.addSystemMessage).not.toHaveBeenCalled();
    expect(ctx._spies.showAlert).toHaveBeenCalledWith(
      'Switching modes is not supported on this session yet — it requires an updated Kiro agent server',
      'error',
      5000
    );
  });

  it('surfaces an RPC failure and emits no success line', async () => {
    const ctx = createMockCommandContext({
      currentAgent: { name: 'default' },
      kiro: {
        setSessionMode: mock(() => Promise.reject(new Error('boom'))),
      } as any,
    });
    ctx.cloudSessionActive = true;
    await handleAutonomous(AUTONOMOUS_CMD, 'on', ctx);
    expect(ctx._spies.addSystemMessage).not.toHaveBeenCalled();
    expect(ctx._spies.showAlert).toHaveBeenCalledWith('boom', 'error', 5000);
  });
});
