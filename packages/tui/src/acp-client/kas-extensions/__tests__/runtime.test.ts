import * as acp from '@agentclientprotocol/sdk';
import type { KiroClient } from '@kiro/client';
import { describe, expect, it } from 'bun:test';
import {
  KiroClientExtensionRuntime,
  type Disposable,
  type NotificationContract,
  type RpcContract,
  type SessionLeaseHandlers,
} from '../runtime.js';

interface TrackedDisposable extends Disposable {
  disposeCalls: number;
}

interface ExtensionSubscription {
  method: string;
  handler: (params: Record<string, unknown>) => void;
  disposable: TrackedDisposable;
}

interface SessionUpdateRegistration {
  sessionId: string;
  handler: (notification: acp.SessionNotification) => Promise<void>;
  disposable: TrackedDisposable;
}

interface PermissionRegistration {
  sessionId: string;
  handler: (
    request: acp.RequestPermissionRequest
  ) => Promise<acp.RequestPermissionResponse>;
  disposable: TrackedDisposable;
}

function trackedDisposable(): TrackedDisposable {
  const disposable: TrackedDisposable = {
    disposeCalls: 0,
    dispose: () => {
      disposable.disposeCalls += 1;
    },
  };
  return disposable;
}

class FakeClient {
  extResponse: Record<string, unknown> = {};
  readonly extMethodCalls: Array<{
    method: string;
    params: Record<string, unknown>;
  }> = [];
  readonly extensionSubscriptions: ExtensionSubscription[] = [];
  readonly sessionUpdateRegistrations: SessionUpdateRegistration[] = [];
  readonly permissionRegistrations: PermissionRegistration[] = [];
  readonly loadSessionCalls: acp.LoadSessionRequest[] = [];
  readonly promptCalls: acp.PromptRequest[] = [];

  async sendExtMethod(
    method: string,
    params: Record<string, unknown>
  ): Promise<Record<string, unknown>> {
    this.extMethodCalls.push({ method, params });
    return this.extResponse;
  }

  onExtNotification(
    method: string,
    handler: (params: Record<string, unknown>) => void
  ): Disposable {
    const disposable = trackedDisposable();
    this.extensionSubscriptions.push({ method, handler, disposable });
    return disposable;
  }

  onSessionUpdate(
    sessionId: string,
    handler: (notification: acp.SessionNotification) => Promise<void>
  ): Disposable {
    const disposable = trackedDisposable();
    this.sessionUpdateRegistrations.push({
      sessionId,
      handler,
      disposable,
    });
    return disposable;
  }

  onPermissionRequest(
    sessionId: string,
    handler: (
      request: acp.RequestPermissionRequest
    ) => Promise<acp.RequestPermissionResponse>
  ): Disposable {
    const disposable = trackedDisposable();
    this.permissionRegistrations.push({
      sessionId,
      handler,
      disposable,
    });
    return disposable;
  }

  async loadSession(request: acp.LoadSessionRequest): Promise<void> {
    this.loadSessionCalls.push(request);
  }

  async prompt(request: acp.PromptRequest): Promise<void> {
    this.promptCalls.push(request);
  }

  emitExtensionNotification(
    method: string,
    params: Record<string, unknown>
  ): void {
    for (const subscription of this.extensionSubscriptions) {
      if (subscription.method === method) {
        subscription.handler(params);
      }
    }
  }

  get operationCount(): number {
    return (
      this.extMethodCalls.length +
      this.extensionSubscriptions.length +
      this.sessionUpdateRegistrations.length +
      this.permissionRegistrations.length +
      this.loadSessionCalls.length +
      this.promptCalls.length
    );
  }
}

const RPC_CONTRACT: RpcContract<{ input: string }, { value: string }> = {
  method: '_kiro/test/request',
  encode: ({ input }) => ({ input }),
  decode: (value) => {
    if (
      value !== null &&
      typeof value === 'object' &&
      'value' in value &&
      typeof value.value === 'string'
    ) {
      return { value: value.value };
    }
    return null;
  },
};

const NOTIFICATION_CONTRACT: NotificationContract<{
  message: string;
}> = {
  method: '_kiro/test/notification',
  decode: (value) => {
    if (
      value !== null &&
      typeof value === 'object' &&
      'message' in value &&
      typeof value.message === 'string'
    ) {
      return { message: value.message };
    }
    return null;
  },
};

function createFixture(onInvalidNotification?: (method: string) => void): {
  client: FakeClient;
  runtime: KiroClientExtensionRuntime;
} {
  const client = new FakeClient();
  const runtime = new KiroClientExtensionRuntime(
    client as unknown as KiroClient,
    onInvalidNotification
  );
  return { client, runtime };
}

function sessionHandlers(): SessionLeaseHandlers {
  return {
    onUpdate: async () => {},
    onPermission: async () => ({ outcome: { outcome: 'cancelled' } }),
  };
}

describe('KiroClientExtensionRuntime', () => {
  it('decodes RPC responses and forwards the typed request', async () => {
    const { client, runtime } = createFixture();
    client.extResponse = { value: 'accepted' };

    await expect(
      runtime.request(RPC_CONTRACT, { input: 'payload' })
    ).resolves.toEqual({ value: 'accepted' });
    expect(client.extMethodCalls).toEqual([
      {
        method: '_kiro/test/request',
        params: { input: 'payload' },
      },
    ]);
  });

  it('rejects an RPC response that fails decoding', async () => {
    const { client, runtime } = createFixture();
    client.extResponse = { value: 42 };

    await expect(
      runtime.request(RPC_CONTRACT, { input: 'payload' })
    ).rejects.toThrow('Invalid response from KAS extension _kiro/test/request');
  });

  it('delivers valid notifications and reports malformed ones without delivering them', () => {
    const invalidMethods: string[] = [];
    const received: string[] = [];
    const { client, runtime } = createFixture((method) => {
      invalidMethods.push(method);
    });

    runtime.subscribe(NOTIFICATION_CONTRACT, ({ message }) => {
      received.push(message);
    });
    client.emitExtensionNotification('_kiro/test/notification', {
      message: 'accepted',
    });
    client.emitExtensionNotification('_kiro/test/notification', {
      message: 42,
    });

    expect(received).toEqual(['accepted']);
    expect(invalidMethods).toEqual(['_kiro/test/notification']);
  });

  it('enforces one lease per session and permits a lease after disposal', () => {
    const { client, runtime } = createFixture();
    const handlers = sessionHandlers();
    const firstLease = runtime.leaseSession('session-1', handlers);

    expect(client.sessionUpdateRegistrations[0]).toMatchObject({
      sessionId: 'session-1',
      handler: handlers.onUpdate,
    });
    expect(client.permissionRegistrations[0]).toMatchObject({
      sessionId: 'session-1',
      handler: handlers.onPermission,
    });
    expect(() => runtime.leaseSession('session-1', handlers)).toThrow(
      'KAS session session-1 already has an extension lease'
    );
    expect(() => runtime.leaseSession('session-2', handlers)).not.toThrow();

    firstLease.dispose();
    firstLease.dispose();
    expect(client.sessionUpdateRegistrations[0]?.disposable.disposeCalls).toBe(
      1
    );
    expect(client.permissionRegistrations[0]?.disposable.disposeCalls).toBe(1);
    expect(() => runtime.leaseSession('session-1', handlers)).not.toThrow();
  });

  it('disposes subscriptions, leases, and the runtime idempotently', () => {
    const { client, runtime } = createFixture();
    const subscription = runtime.subscribe(NOTIFICATION_CONTRACT, () => {});
    const lease = runtime.leaseSession('session-1', sessionHandlers());

    subscription.dispose();
    subscription.dispose();
    lease.dispose();
    lease.dispose();

    const activeSubscription = runtime.subscribe(
      NOTIFICATION_CONTRACT,
      () => {}
    );
    const activeLease = runtime.leaseSession('session-2', sessionHandlers());
    runtime.dispose();
    runtime.dispose();
    activeSubscription.dispose();
    activeLease.dispose();

    expect(
      client.extensionSubscriptions.map(
        ({ disposable }) => disposable.disposeCalls
      )
    ).toEqual([1, 1]);
    expect(
      client.sessionUpdateRegistrations.map(
        ({ disposable }) => disposable.disposeCalls
      )
    ).toEqual([1, 1]);
    expect(
      client.permissionRegistrations.map(
        ({ disposable }) => disposable.disposeCalls
      )
    ).toEqual([1, 1]);
  });

  it('forwards replay and prompt with ACP argument shapes', async () => {
    const { client, runtime } = createFixture();

    await runtime.replaySession('session-1');
    await runtime.promptSession('session-1', 'continue');

    expect(client.loadSessionCalls).toEqual([
      {
        sessionId: 'session-1',
        cwd: process.cwd(),
        mcpServers: [],
      },
    ]);
    expect(client.promptCalls).toEqual([
      {
        prompt: [{ type: 'text', text: 'continue' }],
        sessionId: 'session-1',
      },
    ]);
  });

  it('rejects every operation after disposal without calling the client', async () => {
    const { client, runtime } = createFixture();
    const disposedError = 'KAS extension runtime is disposed';
    runtime.dispose();

    await expect(
      runtime.request(RPC_CONTRACT, { input: 'payload' })
    ).rejects.toThrow(disposedError);
    expect(() => runtime.subscribe(NOTIFICATION_CONTRACT, () => {})).toThrow(
      disposedError
    );
    expect(() => runtime.leaseSession('session-1', sessionHandlers())).toThrow(
      disposedError
    );
    await expect(runtime.replaySession('session-1')).rejects.toThrow(
      disposedError
    );
    await expect(
      runtime.promptSession('session-1', 'continue')
    ).rejects.toThrow(disposedError);
    expect(client.operationCount).toBe(0);
  });
});
