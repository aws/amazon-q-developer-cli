import * as acp from '@agentclientprotocol/sdk';
import type { KiroClient } from '@kiro/client';

export interface Disposable {
  dispose(): void;
}

export interface RpcContract<Params, Response> {
  method: string;
  encode(params: Params): Record<string, unknown>;
  decode(value: unknown): Response | null;
}

export interface NotificationContract<Payload> {
  method: string;
  decode(value: unknown): Payload | null;
}

export interface SessionLeaseHandlers {
  onUpdate(notification: acp.SessionNotification): Promise<void>;
  onPermission(
    request: acp.RequestPermissionRequest
  ): Promise<acp.RequestPermissionResponse>;
}

export interface KasExtensionRuntime {
  request<Params, Response>(
    contract: RpcContract<Params, Response>,
    params: Params
  ): Promise<Response>;
  subscribe<Payload>(
    contract: NotificationContract<Payload>,
    handler: (payload: Payload) => void
  ): Disposable;
  leaseSession(sessionId: string, handlers: SessionLeaseHandlers): Disposable;
  replaySession(sessionId: string): Promise<void>;
  promptSession(sessionId: string, content: string): Promise<void>;
  dispose(): void;
}

/**
 * Typed, disposable boundary around the generic KAS extension APIs.
 *
 * A session can have only one update/permission handler in `KiroClient`.
 * Exclusive leases make replacement explicit instead of silently overwriting
 * another feature's handler.
 */
export class KiroClientExtensionRuntime implements KasExtensionRuntime {
  private readonly subscriptions = new Set<Disposable>();
  private readonly sessionLeases = new Map<string, Disposable>();
  private disposed = false;

  constructor(
    private readonly client: KiroClient,
    private readonly onInvalidNotification: (method: string) => void = () => {}
  ) {}

  async request<Params, Response>(
    contract: RpcContract<Params, Response>,
    params: Params
  ): Promise<Response> {
    this.assertActive();
    const raw = await this.client.sendExtMethod(
      contract.method,
      contract.encode(params)
    );
    const decoded = contract.decode(raw);
    if (decoded === null) {
      throw new Error(`Invalid response from KAS extension ${contract.method}`);
    }
    return decoded;
  }

  subscribe<Payload>(
    contract: NotificationContract<Payload>,
    handler: (payload: Payload) => void
  ): Disposable {
    this.assertActive();
    const raw = this.client.onExtNotification(contract.method, (params) => {
      const decoded = contract.decode(params);
      if (decoded === null) {
        this.onInvalidNotification(contract.method);
        return;
      }
      handler(decoded);
    });
    return this.trackSubscription(raw);
  }

  leaseSession(sessionId: string, handlers: SessionLeaseHandlers): Disposable {
    this.assertActive();
    if (this.sessionLeases.has(sessionId)) {
      throw new Error(
        `KAS session ${sessionId} already has an extension lease`
      );
    }

    const update = this.client.onSessionUpdate(sessionId, handlers.onUpdate);
    const permission = this.client.onPermissionRequest(
      sessionId,
      handlers.onPermission
    );
    let disposed = false;
    const lease: Disposable = {
      dispose: () => {
        if (disposed) return;
        disposed = true;
        update.dispose();
        permission.dispose();
        if (this.sessionLeases.get(sessionId) === lease) {
          this.sessionLeases.delete(sessionId);
        }
      },
    };
    this.sessionLeases.set(sessionId, lease);
    return lease;
  }

  async replaySession(sessionId: string): Promise<void> {
    this.assertActive();
    await this.client.loadSession({
      sessionId,
      cwd: process.cwd(),
      mcpServers: [],
    });
  }

  async promptSession(sessionId: string, content: string): Promise<void> {
    this.assertActive();
    await this.client.prompt({
      prompt: [{ type: 'text', text: content }],
      sessionId,
    });
  }

  dispose(): void {
    if (this.disposed) return;
    this.disposed = true;
    for (const disposable of [...this.subscriptions]) {
      disposable.dispose();
    }
    this.subscriptions.clear();
    for (const lease of [...this.sessionLeases.values()]) {
      lease.dispose();
    }
    this.sessionLeases.clear();
  }

  private trackSubscription(raw: Disposable): Disposable {
    let disposed = false;
    const subscription: Disposable = {
      dispose: () => {
        if (disposed) return;
        disposed = true;
        raw.dispose();
        this.subscriptions.delete(subscription);
      },
    };
    this.subscriptions.add(subscription);
    return subscription;
  }

  private assertActive(): void {
    if (this.disposed) {
      throw new Error('KAS extension runtime is disposed');
    }
  }
}
