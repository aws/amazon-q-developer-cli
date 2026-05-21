/**
 * Tests the `/knowledge` command wire format: CLI → KAS via the real
 * `KasAcpClient` + `@kiro/client` SDK talking to a mock ACP server.
 */
import { describe, it, expect, afterEach } from 'bun:test';
import type {
  InitializeRequest,
  InitializeResponse,
  NewSessionRequest,
  NewSessionResponse,
} from '@agentclientprotocol/sdk';
import { AcpTestCase } from './shared/AcpTestCase';

describe('/knowledge command', () => {
  let tc: AcpTestCase | null = null;

  afterEach(async () => {
    if (tc) {
      await tc.cleanup();
      tc = null;
    }
  });

  it('sends _kiro/knowledge request with correct params', async () => {
    tc = new AcpTestCase({ testName: 'knowledge-command' });

    const cannedEntries = [
      {
        name: 'test-entry',
        id: 'entry-1',
        description: 'Test',
        item_count: 1,
        path: '/test',
      },
    ];

    tc.mock.on<InitializeRequest, InitializeResponse>('initialize', () => ({
      protocolVersion: 1,
      agentCapabilities: {
        sessionCapabilities: {},
        _meta: {
          kiro: {
            extensionMethods: [
              {
                method: '_kiro/knowledge',
                name: '/knowledge',
                description: 'Manage knowledge',
              },
            ],
          },
        },
      },
    }));

    tc.mock.on<NewSessionRequest, NewSessionResponse>('session/new', () => ({
      sessionId: 'test-session-1',
      modes: {
        currentModeId: 'vibe',
        availableModes: [{ id: 'vibe', name: 'Vibe' }],
      },
    }));

    tc.mock.on('session/set_config_option', () => ({}));

    tc.mock.on('_kiro/knowledge', () => ({
      entries: cannedEntries,
      message: 'Knowledge entries loaded',
    }));

    await tc.launch();
    await tc.mock.awaitConnection();
    await tc.sleepMs(200);

    // Send /knowledge by typing the body and pressing Enter as separate
    // events. The slash autocomplete popup that opens on '/' consumes a
    // single Enter as "select highlighted item", which dispatches the
    // command.
    await tc.sendKeys('/knowledge');
    await tc.sleepMs(200);
    await tc.pressEnter();
    await tc.sleepMs(500);

    // Assert the request was sent with correct params
    const knowledgeReqs = tc.mock.receivedRequests('_kiro/knowledge');
    expect(knowledgeReqs.length).toBeGreaterThanOrEqual(1);

    const params = knowledgeReqs[0]!.params as {
      sessionId: string;
      subcommand: string;
    };
    expect(params.subcommand).toBe('show');
    expect(params.sessionId).toBe('test-session-1');

    // Assert the store received the entries
    const store = await tc.getStore();
    expect(store.showKnowledgePanel).toBe(true);
    expect(store.knowledgeEntries).toHaveLength(1);
    expect(store.knowledgeEntries[0]!.name).toBe('test-entry');
  });
});
