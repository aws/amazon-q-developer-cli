import type {
  EditBufferChangedNotification,
  InsertTextRequest,
  InterceptRequest,
  IpcBackend,
  RunProcessRequest,
  RunProcessResponse,
  InterceptedKeyHook,
  PostExecHook,
  PreExecHook,
  PromptHook,
} from "@amzn/fig-io-ipc-backend-core";
import {
  Clientbound,
  Clientbound_Request,
  Hostbound,
} from "@amzn/fig-io-proto/remote";

type SubscriptionStorage<T> = ((notification: T) => void)[];

export interface Message2 {
  data: string;
}

export interface CsWebsocket {
  on(event: "message2", listener: (message: Message2) => void): this;
  send(message: string): void;
}

const NONCE = 0xbeef;

export class WebsocketMuxBackend implements IpcBackend {
  editBufferSubscriptions: SubscriptionStorage<EditBufferChangedNotification> =
    [];
  promptSubscriptions: SubscriptionStorage<PromptHook> = [];
  preExecSubscriptions: SubscriptionStorage<PreExecHook> = [];
  postExecSubscriptions: SubscriptionStorage<PostExecHook> = [];
  interceptedKeySubscriptions: SubscriptionStorage<InterceptedKeyHook> = [];

  text: string = "";

  constructor(private websocket: CsWebsocket) {
    console.log("WebsocketMuxBackend", websocket);
    websocket.on("message2", async (message) => {
      this.text = `${this.text}${message.data}`;

      const lines = this.text.split("\n");
      for (let i = 0; i < lines.length - 1; i++) {
        const line = lines[i];
        try {
          const uint8Array = base64ToBytes(line);
          const hostbound = Hostbound.decode(uint8Array);
          console.log("Received message", hostbound);
          this.handleHostbound(hostbound);
        } catch (e) {
          console.log("Error parsing message", e);
          break;
        }
      }
      this.text = lines[lines.length - 1]?.trimStart() ?? "";
    });
  }

  private handleHostbound(message: Hostbound) {
    const packet = message.packet;
    switch (packet?.$case) {
      case "request": {
        const request = packet.request.request;
        switch (request?.$case) {
          case "editBuffer": {
            this.editBufferSubscriptions.forEach((callback) => {
              callback(request.editBuffer);
            });
            break;
          }
          case "prompt": {
            this.promptSubscriptions.forEach((callback) => {
              callback(request.prompt);
            });
            break;
          }
          case "preExec": {
            this.preExecSubscriptions.forEach((callback) => {
              callback(request.preExec);
            });
            break;
          }
          case "postExec": {
            this.postExecSubscriptions.forEach((callback) => {
              callback(request.postExec);
            });
            break;
          }
          case "interceptedKey": {
            this.postExecSubscriptions.forEach((callback) => {
              callback(request.interceptedKey);
            });
            break;
          }
          default: {
            break;
          }
        }
        break;
      }
      case "response": {
        switch (packet.response.response?.$case) {
          case "runProcess": {
            break;
          }
          case "diagnostics": {
            break;
          }
          case "pseudoterminalExecute": {
            break;
          }
          case "readFile": {
            break;
          }
          case "error": {
            break;
          }
        }
        break;
      }
      default: {
        break;
      }
    }
  }

  // Helper requests

  private sendRequest(request: Clientbound_Request["request"]): void {
    console.log("Sending request", request, this.websocket);
    const clientbound: Clientbound = {
      packet: {
        $case: "request",
        request: {
          nonce: NONCE,
          request,
        },
      },
    };
    const message = Clientbound.encode(clientbound).finish();
    console.log("Sending request BYTES:", message);
    this.websocket.send(bytesToBase64(message));
  }

  insertText(sessionId: string, request: InsertTextRequest): void {
    console.log("insertText");
    this.sendRequest({
      $case: "insertText",
      insertText: request,
    });
  }

  intercept(sessionId: string, request: InterceptRequest): void {
    console.log("intercept");
    this.sendRequest({
      $case: "intercept",
      intercept: request,
    });
  }

  runProcess(
    sessionId: string,
    request: RunProcessRequest,
  ): RunProcessResponse {
    this.sendRequest({
      $case: "runProcess",
      runProcess: request,
    });

    return {
      stdout: "",
      stderr: "",
      exitCode: 0,
    };
  }

  onEditBufferChange(
    callback: (notification: EditBufferChangedNotification) => void,
  ): void {
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

// From https://developer.mozilla.org/en-US/docs/Glossary/Base64#the_unicode_problem.
function base64ToBytes(base64: string) {
  const binString = atob(base64);
  return Uint8Array.from(binString, (m) => m.codePointAt(0)!);
}

// From https://developer.mozilla.org/en-US/docs/Glossary/Base64#the_unicode_problem.
function bytesToBase64(bytes: Uint8Array) {
  const binString = String.fromCodePoint(...bytes);
  return btoa(binString);
}
