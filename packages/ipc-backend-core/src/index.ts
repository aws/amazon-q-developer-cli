import type {
  InsertTextRequest,
  InterceptRequest,
} from "@amzn/fig-io-proto/figterm";
import type { Clientbound_RunProcessRequest as RunProcessRequest } from "@amzn/fig-io-proto/remote";
import type {
  RunProcessResponse,
  EditBufferChangedNotification,
} from "@amzn/fig-io-proto/fig";
import type {
  InterceptedKeyHook,
  PostExecHook,
  PreExecHook,
  PromptHook,
} from "@amzn/fig-io-proto/local";

export type {
  InsertTextRequest,
  InterceptRequest,
  RunProcessRequest,
  RunProcessResponse,
  EditBufferChangedNotification,
  InterceptedKeyHook,
  PostExecHook,
  PreExecHook,
  PromptHook,
};

export interface IpcBackend {
  // Request
  insertText: (sessionId: string, request: InsertTextRequest) => void;
  intercept: (sessionId: string, request: InterceptRequest) => void;

  // Request -> Response
  runProcess: (
    sessionId: string,
    request: RunProcessRequest,
  ) => RunProcessResponse;

  // Notifications
  onEditBufferChange: (
    callback: (notification: EditBufferChangedNotification) => void,
  ) => void;

  onPrompt: (callback: (notification: PromptHook) => void) => void;

  onPreExec: (callback: (notification: PreExecHook) => void) => void;

  onPostExec: (callback: (notification: PostExecHook) => void) => void;

  onInterceptedKey: (
    callback: (notification: InterceptedKeyHook) => void,
  ) => void;
}
