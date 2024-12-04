import {
  ServerOriginatedMessage,
  ClientOriginatedMessage,
} from "@aws/amazon-q-developer-cli-proto/fig";

import { b64ToBytes, bytesToBase64 } from "./utils.js";

interface GlobalAPIError {
  error: string;
}

const FigGlobalErrorOccurred = "FigGlobalErrorOccurred";
const FigProtoMessageReceived = "FigProtoMessageReceived";

type shouldKeepListening = boolean;

export type APIResponseHandler = (
  response: ServerOriginatedMessage["submessage"],
) => shouldKeepListening | void;

let messageId = 0;
const handlers: Record<number, APIResponseHandler> = {};

export function setHandlerForId(handler: APIResponseHandler, id: number) {
  handlers[id] = handler;
}

const receivedMessage = (response: ServerOriginatedMessage): void => {
  if (response.id === undefined) {
    return;
  }

  const handler = handlers[response.id];

  if (!handler) {
    return;
  }

  const keepListeningOnID = handlers[response.id](response.submessage);

  if (!keepListeningOnID) {
    delete handlers[response.id];
  }
};

export function sendMessage(
  message: ClientOriginatedMessage["submessage"],
  handler?: APIResponseHandler,
) {
  const request: ClientOriginatedMessage = {
    id: (messageId += 1),
    submessage: message,
  };

  if (handler && request.id) {
    handlers[request.id] = handler;
  }

  const buffer = ClientOriginatedMessage.encode(request).finish();

  if (
    window?.fig?.constants?.supportApiProto &&
    window?.fig?.constants?.apiProtoUrl
  ) {
    const url = new URL(window.fig.constants.apiProtoUrl);

    if (
      request.submessage?.$case &&
      typeof request.submessage?.$case === "string"
    ) {
      url.pathname = `/${request.submessage.$case}`;
    } else {
      url.pathname = "/unknown";
    }

    fetch(url, {
      method: "POST",
      headers: {
        "content-type": "application/fig-api",
      },
      body: buffer,
    }).then(async (res) => {
      const body = new Uint8Array(await res.arrayBuffer());
      const m = ServerOriginatedMessage.decode(body);
      receivedMessage(m);
    });
    return;
  }

  const b64 = bytesToBase64(buffer);

  if (window.ipc && window.ipc.postMessage) {
    window.ipc.postMessage(b64);
  } else if (window.webkit) {
    if (!window.webkit?.messageHandlers?.proto) {
      console.error(
        "This version of Amazon Q does not support using protocol buffers. Please update.",
      );
      return;
    }
    window.webkit.messageHandlers.proto.postMessage(b64);
  } else {
    console.error(
      "Cannot send request. Fig.js is not supported in this browser.",
    );
  }
}

const setupEventListeners = (): void => {
  document.addEventListener(FigGlobalErrorOccurred, (event: Event) => {
    const response = (event as CustomEvent).detail as GlobalAPIError;
    console.error(response.error);
  });

  document.addEventListener(FigProtoMessageReceived, (event: Event) => {
    const raw = (event as CustomEvent).detail as string;
    const bytes = b64ToBytes(raw);
    const message = ServerOriginatedMessage.decode(bytes);
    receivedMessage(message);
  });
};

// We want this to be run automatically
if (!window?.fig?.quiet) {
  console.log("[q] setting up event listeners...");
}
setupEventListeners();
