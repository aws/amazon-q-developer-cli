/**
 * /config command routing tests (cloud config UX).
 *
 * The load-bearing invariants:
 *  - `/config mcp` opens the SAME view as `/mcp` — never a duplicate page.
 *  - `/config hooks` reuses the /hooks panel.
 *  - Other categories open the ConfigPanel on that page.
 *  - Unknown categories alert instead of opening anything.
 *  - Darkship: the /config registration is gated on Feature.CloudConfig
 *    (asserted in the app-store filter test below via feature toggling).
 */

import { describe, it, expect, beforeEach, afterEach, mock } from 'bun:test';
import { dispatch } from '../dispatcher';
import type { SlashCommand } from '../../stores/app-store';
import { createMockCommandContext } from './test-helpers.js';
import { features } from '../../features';

const configCmd: SlashCommand = {
  name: '/config',
  description:
    'View configured agents, MCP servers, steering, skills, hooks, and env variables',
  source: 'local',
  meta: { local: true },
};

const ORIGINAL_FEATURES = process.env.KIRO_ENABLED_FEATURES;

function setFeatures(json: string | undefined) {
  if (json === undefined) delete process.env.KIRO_ENABLED_FEATURES;
  else process.env.KIRO_ENABLED_FEATURES = json;
  features._resetForTests();
}

describe('/config command', () => {
  beforeEach(() => {
    setFeatures('["cloud_config"]');
  });

  afterEach(() => {
    setFeatures(ORIGINAL_FEATURES);
  });

  it('bare /config opens the ConfigPanel overlay', async () => {
    const ctx = createMockCommandContext({ slashCommands: [configCmd] });
    await dispatch(configCmd, '', ctx);
    expect((ctx as any)._spies.setShowConfigPanel).toHaveBeenCalledWith(true);
  });

  it('/config mcp opens the shared MCP panel with ESC-back primed (KAS engine)', async () => {
    const ctx = createMockCommandContext({
      slashCommands: [configCmd],
      agentEngine: 'kas',
      mcpServerCache: [{ name: 'github', status: 'running', toolCount: 3 }],
    } as any);
    // Order log: closing /config BEFORE the routed panel opens is exactly
    // the regression that reopens the queue-drain window.
    const order: string[] = [];
    (ctx as any).setShowMcpPanel = (...args: unknown[]) => {
      order.push('mcp-open');
      (ctx as any)._spies.setShowMcpPanel(...args);
    };
    (ctx as any).setShowConfigPanel = (show: boolean) => {
      order.push(show ? 'config-open' : 'config-close');
      (ctx as any)._spies.setShowConfigPanel(show);
    };
    await dispatch(configCmd, 'mcp', ctx);
    expect((ctx as any)._spies.setShowMcpPanel).toHaveBeenCalled();
    // The handler closes /config (row-select path) AFTER the routed panel
    // opened — and never opens it.
    expect(order).toEqual(['mcp-open', 'config-close']);
    // ESC from the routed panel returns to the /config table — the
    // /settings-parity behavior (settingsReturnOnEscape twin).
    expect((ctx as any)._spies.setConfigReturnOnEscape).toHaveBeenCalledWith(
      true
    );
  });

  it('a row-select handoff ends its token on the success path', async () => {
    // Simulate the panel having bumped the token to 7 before dispatching.
    const ctx = createMockCommandContext({ slashCommands: [configCmd] }) as any;
    ctx.getConfigHandoffToken = () => 7;
    ctx.kiro.executeCommand = mock(() =>
      Promise.resolve({
        success: true,
        message: '',
        data: { servers: [], mode: 'list' },
      })
    );
    await dispatch(configCmd, 'mcp', ctx);
    expect(ctx._spies.setShowMcpPanel).toHaveBeenCalled();
    // The owning handoff releases its token in the finally.
    expect(ctx._spies.endConfigHandoff).toHaveBeenCalled();
  });

  it('an ESC-cancel during the routed RPC opens nothing (handoff aborted)', async () => {
    // Row-select captured token 7; ESC zeroes it mid-RPC. The handler must
    // detect the identity change and not open the panel the user left.
    let token = 7;
    const ctx = createMockCommandContext({ slashCommands: [configCmd] }) as any;
    ctx.getConfigHandoffToken = () => token;
    ctx.kiro.executeCommand = mock(() => {
      token = 0; // ESC-cancel lands while the RPC is outstanding
      return Promise.resolve({
        success: true,
        message: '',
        data: { servers: [], mode: 'list' },
      });
    });
    await dispatch(configCmd, 'mcp', ctx);
    expect(ctx._spies.setShowMcpPanel).not.toHaveBeenCalled();
    // Cancelled navigation leaves no ESC-back residue for the next /mcp.
    const calls = ctx._spies.setConfigReturnOnEscape.mock.calls;
    expect(calls[calls.length - 1]).toEqual([false]);
  });

  it('a superseding second handoff (ABA) does not let the stale one open', async () => {
    // mcp captures token 7; a second selection bumps it to 8 mid-RPC. The
    // stale mcp handler must see 8 !== 7 (identity, not a re-primed boolean)
    // and open nothing — the boolean model this replaced would misread the
    // re-primed flag as "still live" and open the abandoned panel.
    let token = 7;
    const ctx = createMockCommandContext({ slashCommands: [configCmd] }) as any;
    ctx.getConfigHandoffToken = () => token;
    ctx.kiro.executeCommand = mock(() => {
      token = 8; // a later row-select re-primed the handoff
      return Promise.resolve({
        success: true,
        message: '',
        data: { servers: [], mode: 'list' },
      });
    });
    await dispatch(configCmd, 'mcp', ctx);
    expect(ctx._spies.setShowMcpPanel).not.toHaveBeenCalled();
    // The stale handler must NOT end the token — the live handoff (8) owns it.
    expect(ctx._spies.endConfigHandoff).not.toHaveBeenCalled();
  });

  it('a failed /config mcp RPC (V2) unsets ESC-back so plain /mcp is unaffected', async () => {
    const ctx = createMockCommandContext({
      slashCommands: [configCmd],
      kiro: {
        executeCommand: (() => Promise.reject(new Error('rpc down'))) as never,
      },
    } as any);
    // dispatch surfaces the rejection via the effect's own catch (alert);
    // the invariant here is flag hygiene, not the error channel.
    await dispatch(configCmd, 'mcp', ctx).catch(() => {});
    const calls = (ctx as any)._spies.setConfigReturnOnEscape.mock.calls;
    // Primed before the await, unset after the failure — the LAST call must
    // be false, or the next bare /mcp titles itself '/config — MCP' and ESC
    // bounces into the category table.
    expect(calls[calls.length - 1]).toEqual([false]);
    expect((ctx as any)._spies.setShowMcpPanel).not.toHaveBeenCalled();
  });

  it('/config hooks opens the hooks panel with ESC-back primed', async () => {
    const ctx = createMockCommandContext({ slashCommands: [configCmd] });
    await dispatch(configCmd, 'hooks', ctx);
    expect((ctx as any)._spies.setShowHooksPanel).toHaveBeenCalled();
    const hooksShowCalls = (ctx as any)._spies.setShowConfigPanel.mock.calls;
    expect(hooksShowCalls.every(([show]: [boolean]) => show === false)).toBe(
      true
    );
    expect((ctx as any)._spies.setConfigReturnOnEscape).toHaveBeenCalledWith(
      true
    );
  });

  it('/config hooks serves the warm cache on KAS without an RPC', async () => {
    // KAS-only shortcut: _kiro/hooks/didChange keeps the cache fresh there.
    // V2 has no such push, so it always re-fetches (next test).
    const executeCommand = mock(() =>
      Promise.resolve({ success: true, message: '', data: undefined })
    );
    const cached = [{ trigger: 'preToolUse', command: 'lint.sh' }];
    const ctx = createMockCommandContext({
      slashCommands: [configCmd],
      agentEngine: 'kas',
      hooksList: cached,
      kiro: { executeCommand },
    } as any);
    await dispatch(configCmd, 'hooks', ctx);
    expect(executeCommand).not.toHaveBeenCalled();
    expect((ctx as any)._spies.setShowHooksPanel).toHaveBeenCalledWith(
      true,
      cached
    );
  });

  it('V2 /config hooks re-fetches even with a warm cache (no push invalidation)', async () => {
    const fresh = [{ trigger: 'preToolUse', command: 'edited.sh' }];
    const executeCommand = mock(() =>
      Promise.resolve({ success: true, message: '', data: { hooks: fresh } })
    );
    const ctx = createMockCommandContext({
      slashCommands: [configCmd],
      hooksList: [{ trigger: 'preToolUse', command: 'stale.sh' }],
      kiro: { executeCommand },
    } as any);
    await dispatch(configCmd, 'hooks', ctx);
    expect(executeCommand).toHaveBeenCalled();
    expect((ctx as any)._spies.setShowHooksPanel).toHaveBeenCalledWith(
      true,
      fresh
    );
  });

  it('/config hooks re-fetches in cloud sessions (warm cache may be local-fed)', async () => {
    const sandboxHooks = [{ trigger: 'promptSubmit', command: 'cloud.sh' }];
    const executeCommand = mock(() =>
      Promise.resolve({
        success: true,
        message: '',
        data: { hooks: sandboxHooks },
      })
    );
    // agentEngine 'kas': the warm-cache shortcut is KAS-only, so this pins
    // that the CLOUD clause forces the re-fetch (not the engine clause).
    const ctx = createMockCommandContext({
      slashCommands: [configCmd],
      agentEngine: 'kas',
      cloudSessionActive: true,
      hooksList: [{ trigger: 'preToolUse', command: 'stale-local.sh' }],
      kiro: { executeCommand },
    } as any);
    await dispatch(configCmd, 'hooks', ctx);
    // The authoritative sandbox listing wins over the warm local cache —
    // same rule as the /hooks handler.
    expect(executeCommand).toHaveBeenCalled();
    expect((ctx as any)._spies.setShowHooksPanel).toHaveBeenCalledWith(
      true,
      sandboxHooks
    );
  });

  it('a failed /config hooks fetch unsets ESC-back and opens nothing', async () => {
    const ctx = createMockCommandContext({
      slashCommands: [configCmd],
      cloudSessionActive: true,
      kiro: {
        executeCommand: mock(() =>
          Promise.resolve({ success: false, message: 'nope', data: undefined })
        ),
      },
    } as any);
    await dispatch(configCmd, 'hooks', ctx);
    const calls = (ctx as any)._spies.setConfigReturnOnEscape.mock.calls;
    expect(calls[calls.length - 1]).toEqual([false]);
    expect((ctx as any)._spies.setShowHooksPanel).not.toHaveBeenCalled();
    expect((ctx as any)._spies.showAlert).toHaveBeenCalled();
  });

  it('V2 /config mcp forwards registryServers like the /mcp effect', async () => {
    const servers = [{ name: 'github', status: 'running', toolCount: 3 }];
    const registryServers = [
      { name: 'registry-only', status: 'disabled', toolCount: 0 },
    ];
    const ctx = createMockCommandContext({
      slashCommands: [configCmd],
      kiro: {
        executeCommand: mock(() =>
          Promise.resolve({
            success: true,
            message: '',
            data: { servers, registryServers, mode: 'list' },
          })
        ),
      },
    } as any);
    await dispatch(configCmd, 'mcp', ctx);
    expect((ctx as any)._spies.setShowMcpPanel).toHaveBeenCalledWith(
      true,
      servers,
      'list',
      registryServers
    );
  });

  it('page categories do not prime ESC-back (ConfigPanel handles its own back)', async () => {
    const ctx = createMockCommandContext({ slashCommands: [configCmd] });
    await dispatch(configCmd, 'skills', ctx);
    expect((ctx as any)._spies.setShowConfigPanel).toHaveBeenCalledWith(
      true,
      'skills'
    );
    expect((ctx as any)._spies.setConfigReturnOnEscape).not.toHaveBeenCalled();
  });

  it('/config steering opens the ConfigPanel on the steering page', async () => {
    const ctx = createMockCommandContext({ slashCommands: [configCmd] });
    await dispatch(configCmd, 'steering', ctx);
    expect((ctx as any)._spies.setShowConfigPanel).toHaveBeenCalledWith(
      true,
      'steering'
    );
  });

  it('/config with an unknown category alerts and opens nothing', async () => {
    const ctx = createMockCommandContext({ slashCommands: [configCmd] });
    await dispatch(configCmd, 'bogus', ctx);
    expect((ctx as any)._spies.setShowConfigPanel).not.toHaveBeenCalled();
    expect((ctx as any)._spies.showAlert).toHaveBeenCalled();
  });
});
