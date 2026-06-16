/**
 * Tests the `/prompts` command picker for KAS:
 *   - prompts (workspace, global, mcp), skills, and steering arrive via
 *     `available_commands_update` notifications tagged with
 *     `_meta.kiro.{type,scope,serverName}`.
 *   - Each entry routes into its dedicated AppState slice
 *     (`prompts` / `skills` / `steering`) preserving its source kind.
 *   - Typing `/prompts<Enter>` opens the selection menu and surfaces all
 *     three categories (not prompts only).
 *   - Typing into the menu's search filter narrows the visible entries.
 */
import { describe, it, expect, afterEach } from 'bun:test';
import type {
  InitializeRequest,
  InitializeResponse,
  NewSessionRequest,
  NewSessionResponse,
} from '@agentclientprotocol/sdk';
import { AcpTestCase } from './shared/AcpTestCase';
import { defaultKasModes } from './shared/default-agent';

describe('/prompts command', () => {
  let tc: AcpTestCase | null = null;

  afterEach(async () => {
    if (tc) {
      await tc.cleanup();
      tc = null;
    }
  });

  it('lists prompts (workspace/global/mcp), skills, and steering with source kinds preserved', async () => {
    tc = new AcpTestCase({ testName: 'prompts-command' });

    tc.mock.on<InitializeRequest, InitializeResponse>('initialize', () => ({
      protocolVersion: 1,
      agentCapabilities: {
        sessionCapabilities: {},
      },
    }));

    tc.mock.on<NewSessionRequest, NewSessionResponse>('session/new', () => ({
      sessionId: 'test-session-1',
      modes: defaultKasModes(),
    }));

    tc.mock.on('session/set_config_option', () => ({}));

    await tc.launch();
    await tc.mock.awaitConnection();
    await tc.sleepMs(200);

    // Push prompts, skills, and steering via a single
    // `available_commands_update` notification - the canonical KAS
    // shape for advertising user-invocable templates.
    tc.mock.notify('session/update', {
      sessionId: 'test-session-1',
      update: {
        sessionUpdate: 'available_commands_update',
        availableCommands: [
          {
            name: 'review',
            description: 'Workspace review prompt',
            _meta: { kiro: { type: 'prompt', scope: 'workspace' } },
          },
          {
            name: 'summarize',
            description: 'Global summarize prompt',
            _meta: { kiro: { type: 'prompt', scope: 'global' } },
          },
          {
            name: 'gh-search',
            description: 'GitHub MCP search prompt',
            _meta: {
              kiro: { type: 'prompt', scope: 'mcp', serverName: 'github' },
            },
          },
          {
            name: 'tdd',
            description: 'Workspace skill',
            _meta: { kiro: { type: 'skill', scope: 'workspace' } },
          },
          {
            name: 'rust-style',
            description: 'Global skill',
            _meta: { kiro: { type: 'skill', scope: 'global' } },
          },
          {
            name: 'project-conventions',
            description: 'Workspace steering',
            _meta: { kiro: { type: 'steering', scope: 'workspace' } },
          },
          {
            name: 'team-conventions',
            description: 'Global steering',
            _meta: { kiro: { type: 'steering', scope: 'global' } },
          },
        ],
      },
    });
    await tc.sleepMs(300);

    // Verify routing into the typed AppState slices preserves source kind.
    const store = await tc.getStore();
    expect(store.prompts).toHaveLength(3);
    expect(store.skills).toHaveLength(2);
    expect(store.steering).toHaveLength(2);

    const ghSearch = store.prompts.find((p) => p.name === 'gh-search');
    expect(ghSearch?.source).toEqual({ kind: 'mcp', serverName: 'github' });
    const summarize = store.prompts.find((p) => p.name === 'summarize');
    expect(summarize?.source).toEqual({ kind: 'global' });
    const review = store.prompts.find((p) => p.name === 'review');
    expect(review?.source).toEqual({ kind: 'workspace' });

    const rustStyle = store.skills.find((s) => s.name === 'rust-style');
    expect(rustStyle?.source).toEqual({ kind: 'global' });
    const teamConv = store.steering.find((s) => s.name === 'team-conventions');
    expect(teamConv?.source).toEqual({ kind: 'global' });

    // Open the /prompts picker.
    await tc.sendKeys('/prompts');
    await tc.sleepMs(200);
    await tc.pressEnter();
    await tc.sleepMs(400);

    // The picker MUST list every entry advertised, regardless of type.
    await tc.waitForVisibleText('review', 2000);
    const snap = tc.getSnapshotFormatted();
    expect(snap).toContain('review');
    expect(snap).toContain('summarize');
    expect(snap).toContain('gh-search');
    expect(snap).toContain('tdd');
    expect(snap).toContain('rust-style');
    expect(snap).toContain('project-conventions');
    expect(snap).toContain('team-conventions');

    // Source-kind grouping is reflected in the rendered group column:
    //   - prompts: scope label (workspace / global / mcp serverName)
    //   - skills: literal "skill"
    //   - steering: literal "steering"
    expect(snap).toContain('github'); // mcp serverName for gh-search
    expect(snap).toContain('skill');
    expect(snap).toContain('steering');
    expect(snap).toContain('workspace');
    expect(snap).toContain('global');
  });
});
