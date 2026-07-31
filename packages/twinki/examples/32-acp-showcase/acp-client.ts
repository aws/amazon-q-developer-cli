import { execFile, spawn, type ChildProcessWithoutNullStreams } from "node:child_process";
import { Readable, Writable } from "node:stream";
import { setTimeout as delay } from "node:timers/promises";
import * as acp from "@agentclientprotocol/sdk";
import { sanitizeTerminalText } from "twinki";
import type { AcpEvent, PermissionPrompt } from "./types.js";

const GET_ACCESS_TOKEN = "_kiro/auth/getAccessToken";
const STEER = "_session/steer";
const CLEAR_STEERING = "_session/steer/clear";

export interface ShowcaseClientOptions {
  command: string;
  args: string[];
  cwd: string;
}

interface PendingPermission {
  id: string;
  prompt: PermissionPrompt;
  resolve: (response: acp.RequestPermissionResponse) => void;
}

type Listener = (event: AcpEvent) => void;

function encode(value: unknown): string | undefined {
  if (typeof value === "string") return sanitizeTerminalText(value);
  if (value === undefined) return undefined;
  try {
    return sanitizeTerminalText(JSON.stringify(value));
  } catch {
    return undefined;
  }
}

function signal(child: ChildProcessWithoutNullStreams, value: NodeJS.Signals): void {
  if (child.pid === undefined) return;
  try {
    process.kill(process.platform === "win32" ? child.pid : -child.pid, value);
  } catch (error) {
    if (!(error instanceof Error && "code" in error && error.code === "ESRCH")) {
      throw error;
    }
  }
}

async function terminate(child: ChildProcessWithoutNullStreams): Promise<void> {
  child.stdin.end();
  if (child.exitCode !== null || child.signalCode !== null) return;
  const stopped = new Promise<void>((resolve) => child.once("exit", () => resolve()));
  signal(child, "SIGTERM");
  await Promise.race([stopped, delay(700)]);
  if (child.exitCode === null && child.signalCode === null) {
    signal(child, "SIGKILL");
    await Promise.race([stopped, delay(200)]);
  }
  child.stdin.destroy();
  child.stdout.destroy();
  child.stderr.destroy();
}

export class ShowcaseClient implements acp.Client {
  private child: ChildProcessWithoutNullStreams | null = null;
  private connection: acp.ClientSideConnection | null = null;
  private sessionId: string | null = null;
  private readonly listeners = new Set<Listener>();
  private readonly permissions: PendingPermission[] = [];
  private permissionSequence = 0;
  private turnActive = false;
  private closing = false;
  private stderrTail = "";

  constructor(private readonly options: ShowcaseClientOptions) {}

  subscribe(listener: Listener): () => void {
    this.listeners.add(listener);
    return () => this.listeners.delete(listener);
  }

  async start(): Promise<void> {
    if (this.connection) return;
    this.emit({ type: "connection", state: "connecting" });
    this.closing = false;
    const child = spawn(this.options.command, this.options.args, {
      cwd: this.options.cwd,
      detached: process.platform !== "win32",
      env: { ...process.env, KIRO_ACP_MODE: "1" },
      stdio: ["pipe", "pipe", "pipe"],
    });
    this.child = child;
    const spawned = new Promise<void>((resolve, reject) => {
      child.once("spawn", resolve);
      child.once("error", reject);
    });
    child.stderr.setEncoding("utf8");
    child.stderr.on("data", (chunk: string) => {
      this.stderrTail = `${this.stderrTail}${chunk}`.slice(-3000);
    });
    child.on("exit", (code, reason) => {
      if (this.closing) return;
      const detail = this.stderrTail.trim().split("\n").at(-1);
      this.emit({
        type: "error",
        message: `ACP exited (${reason ?? `code ${code ?? "unknown"}`})${detail ? `: ${detail}` : ""}`,
      });
    });
    try {
      await spawned;
      const stream = acp.ndJsonStream(
        Writable.toWeb(child.stdin),
        Readable.toWeb(child.stdout) as ReadableStream<Uint8Array>,
      );
      const connection = new acp.ClientSideConnection(() => this, stream);
      this.connection = connection;
      const initialized = await connection.initialize({
        protocolVersion: acp.PROTOCOL_VERSION,
        clientCapabilities: {},
        clientInfo: { name: "twinki-acp-showcase", version: "0.1.0" },
      });
      const created = await connection.newSession({
        cwd: this.options.cwd,
        mcpServers: [],
      });
      this.sessionId = created.sessionId;
      this.emit({
        type: "connection",
        state: "ready",
        agentName: initialized.agentInfo?.name ?? this.options.command,
      });
    } catch (error) {
      this.emit({
        type: "error",
        message: error instanceof Error ? error.message : String(error),
      });
      await this.close();
    }
  }

  async prompt(text: string): Promise<void> {
    if (!this.connection || !this.sessionId || this.turnActive) return;
    this.turnActive = true;
    this.emit({ type: "connection", state: "running" });
    try {
      const result = await this.connection.prompt({
        sessionId: this.sessionId,
        prompt: [{ type: "text", text }],
      });
      this.emit({ type: "turn_done", stopReason: result.stopReason });
    } catch (error) {
      this.emit({
        type: "error",
        message: `Prompt failed: ${error instanceof Error ? error.message : String(error)}`,
      });
    } finally {
      this.turnActive = false;
      this.cancelPermissions();
    }
  }

  async steer(text: string): Promise<void> {
    if (!this.connection || !this.sessionId || !this.turnActive) {
      throw new Error("No active turn to steer");
    }
    await this.connection.extMethod(STEER, {
      sessionId: this.sessionId,
      message: text,
    });
  }

  async clearSteering(): Promise<void> {
    if (!this.connection || !this.sessionId || !this.turnActive) return;
    await this.connection.extMethod(CLEAR_STEERING, {
      sessionId: this.sessionId,
    });
  }

  async cancel(): Promise<void> {
    if (!this.connection || !this.sessionId || !this.turnActive) return;
    this.emit({ type: "connection", state: "cancelling" });
    this.cancelPermissions();
    try {
      await this.clearSteering();
    } catch {
      // Cancellation still proceeds when an agent has no steering extension.
    }
    await this.connection.cancel({ sessionId: this.sessionId });
  }

  choosePermission(optionId: string): void {
    const pending = this.permissions.shift();
    if (!pending) return;
    pending.resolve({
      outcome: { outcome: "selected", optionId },
    });
    this.publishPermission();
  }

  dismissPermission(): void {
    const pending = this.permissions.shift();
    pending?.resolve({ outcome: { outcome: "cancelled" } });
    this.publishPermission();
  }

  async requestPermission(request: acp.RequestPermissionRequest): Promise<acp.RequestPermissionResponse> {
    if (this.closing) return { outcome: { outcome: "cancelled" } };
    const id = `permission-${++this.permissionSequence}`;
    const prompt: PermissionPrompt = {
      id,
      toolName: request.toolCall.title ?? request.toolCall.kind ?? "Tool",
      detail: encode(request.toolCall.rawInput),
      choices: request.options.map((option) => ({
        id: option.optionId,
        label: option.name,
        kind: option.kind,
      })),
    };
    return new Promise((resolve) => {
      this.permissions.push({ id, prompt, resolve });
      if (this.permissions.length === 1) this.publishPermission();
    });
  }

  async sessionUpdate(notification: acp.SessionNotification): Promise<void> {
    if (!this.sessionId || notification.sessionId === this.sessionId) {
      this.emit({ type: "session_update", update: notification.update });
    }
  }

  async extMethod(method: string, _params: Record<string, unknown>): Promise<Record<string, unknown>> {
    if (method !== GET_ACCESS_TOKEN) {
      throw new Error(`Unsupported ACP extension: ${method}`);
    }
    return getKiroToken(this.options.command);
  }

  async extNotification(_method: string, _params: Record<string, unknown>): Promise<void> {}

  async close(): Promise<void> {
    if (this.closing) return;
    this.closing = true;
    this.cancelPermissions();
    this.connection = null;
    this.sessionId = null;
    const child = this.child;
    this.child = null;
    if (child) await terminate(child);
  }

  private publishPermission(): void {
    this.emit({
      type: "permission",
      prompt: this.permissions[0]?.prompt ?? null,
    });
  }

  private cancelPermissions(): void {
    for (const pending of this.permissions.splice(0)) {
      pending.resolve({ outcome: { outcome: "cancelled" } });
    }
    this.emit({ type: "permission", prompt: null });
  }

  private emit(event: AcpEvent): void {
    for (const listener of this.listeners) listener(event);
  }
}

function getKiroToken(command: string): Promise<Record<string, unknown>> {
  return new Promise((resolve, reject) => {
    execFile(command, ["chat", "_", "get-kas-token"], { timeout: 30_000 }, (error, stdout) => {
      if (error) {
        reject(new Error(`Kiro authentication failed: ${error.message}`));
        return;
      }
      try {
        const envelope = JSON.parse(stdout) as {
          kind?: string;
          data?: Record<string, unknown>;
        };
        if (envelope.kind === "getKasToken" && envelope.data) {
          resolve(envelope.data);
        } else {
          reject(new Error("Kiro returned an unexpected token response"));
        }
      } catch {
        reject(new Error("Kiro returned invalid token JSON"));
      }
    });
  });
}
