import type {
  InsertTextRequest,
  InterceptRequest,
  IpcBackend,
  RunProcessRequest,
  RunProcessResponse,
  InterceptedKeyHook,
  PostExecHook,
  PreExecHook,
  PromptHook,
} from "@aws/amazon-q-developer-cli-ipc-backend-core";
import { Clientbound, Hostbound } from "@aws/amazon-q-developer-cli-proto/mux";
import { PacketStream } from "./packetStream.js";
import { CsWebsocket, Socket } from "./socket.js";
import { clientboundToPacket, packetToHostbound } from "./mux.js";
import { EditBufferHook } from "@aws/amazon-q-developer-cli-proto/local";
import { create } from "@bufbuild/protobuf";
import { RunProcessResponseSchema } from "@aws/amazon-q-developer-cli-proto/fig";

export { CsWebsocket };

type SubscriptionStorage<T> = ((notification: T) => void)[];

export class WebsocketMuxBackend implements IpcBackend {
  editBufferSubscriptions: SubscriptionStorage<EditBufferHook> = [];
  promptSubscriptions: SubscriptionStorage<PromptHook> = [];
  preExecSubscriptions: SubscriptionStorage<PreExecHook> = [];
  postExecSubscriptions: SubscriptionStorage<PostExecHook> = [];
  interceptedKeySubscriptions: SubscriptionStorage<InterceptedKeyHook> = [];

  packetStream: PacketStream;

  constructor(websocket: CsWebsocket) {
    const socket = Socket.cs(websocket);
    const packetStream = new PacketStream(socket);
    this.packetStream = packetStream;

    (async () => {
      const stream = packetStream.getReader().getReader();
      while (true) {
        const { done, value } = await stream.read();

        if (value) {
          const hostbound = await packetToHostbound(value);
          this.handleHostbound(hostbound);
        }

        if (done) break;
      }
    })();
  }

  private handleHostbound(message: Hostbound) {
    const submessage = message.submessage;
    console.log(submessage);
    switch (submessage?.case) {
      case "editBuffer":
        this.editBufferSubscriptions.forEach((callback) => {
          callback(submessage.value);
        });
        break;
      case "interceptedKey":
        this.interceptedKeySubscriptions.forEach((callback) => {
          callback(submessage.value);
        });
        break;
      case "postExec":
        this.postExecSubscriptions.forEach((callback) => {
          callback(submessage.value);
        });
        break;
      case "preExec":
        this.preExecSubscriptions.forEach((callback) => {
          callback(submessage.value);
        });
        break;
      case "prompt":
        this.promptSubscriptions.forEach((callback) => {
          callback(submessage.value);
        });
        break;
      case "runProcessResponse":
        break;
    }
  }

  // Helper requests

  private async sendRequest(
    sessionId: string,
    clientbound: Clientbound["submessage"],
  ) {
    const packet = await clientboundToPacket({
      sessionId,
      submessage: clientbound,
    });
    this.packetStream.getWriter().write(packet);
  }

  insertText(sessionId: string, request: InsertTextRequest): void {
    console.log("insertText");
    this.sendRequest(sessionId, {
      case: "insertText",
      value: request,
    });
  }

  intercept(sessionId: string, request: InterceptRequest): void {
    console.log("intercept");
    this.sendRequest(sessionId, {
      case: "intercept",
      value: request,
    });
  }

  runProcess(
    sessionId: string,
    request: RunProcessRequest,
  ): RunProcessResponse {
    this.sendRequest(sessionId, {
      case: "runProcess",
      value: request,
    });

    return create(RunProcessResponseSchema, {
      stdout: "",
      stderr: "",
      exitCode: 0,
    });
  }

  onEditBufferChange(callback: (notification: EditBufferHook) => void): void {
    this.editBufferSubscriptions.push(callback);
  }

  onPrompt(callback: (notification: PromptHook) => void): void {
    this.promptSubscriptions.push(callback);
  }

  onPreExec(callback: (notification: PreExecHook) => void): void {
    this.preExecSubscriptions.push(callback);
  }

  onPostExec(callback: (notification: PostExecHook) => void): void {
    this.postExecSubscriptions.push(callback);
  }

  onInterceptedKey(callback: (notification: InterceptedKeyHook) => void): void {
    this.interceptedKeySubscriptions.push(callback);
  }
}
