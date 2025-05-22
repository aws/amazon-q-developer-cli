import { ClientOriginatedMessageSchema, ServerOriginatedMessageSchema, } from "@aws/amazon-q-developer-cli-proto/fig";
import { b64ToBytes, bytesToBase64 } from "./utils.js";
import { create, fromBinary, toBinary } from "@bufbuild/protobuf";
const FigGlobalErrorOccurred = "FigGlobalErrorOccurred";
const FigProtoMessageReceived = "FigProtoMessageReceived";
let messageId = 0;
const handlers = {};
export function setHandlerForId(handler, id) {
    handlers[id] = handler;
}
const receivedMessage = (response) => {
    if (response.id === undefined) {
        return;
    }
    const id = response.id.toString();
    const handler = handlers[id];
    if (!handler) {
        return;
    }
    const keepListeningOnID = handlers[id](response.submessage);
    if (!keepListeningOnID) {
        delete handlers[id];
    }
};
export function sendMessage(submessage, handler) {
    const request = create(ClientOriginatedMessageSchema, {
        id: BigInt((messageId += 1)),
        submessage,
    });
    if (handler && request.id) {
        handlers[request.id.toString()] = handler;
    }
    const buffer = toBinary(ClientOriginatedMessageSchema, request);
    if (window?.fig?.constants?.supportApiProto &&
        window?.fig?.constants?.apiProtoUrl) {
        const url = new URL(window.fig.constants.apiProtoUrl);
        if (typeof request.submessage?.case === "string") {
            url.pathname = `/${request.submessage.case}`;
        }
        else {
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
            const message = fromBinary(ServerOriginatedMessageSchema, body);
            receivedMessage(message);
        });
        return;
    }
    const postMessage = window?.ipc?.postMessage;
    if (postMessage && typeof postMessage === "function") {
        const b64 = bytesToBase64(buffer);
        postMessage(b64);
        return;
    }
    console.error("Cannot send request. Fig.js is not supported in this browser.");
}
const setupEventListeners = () => {
    document.addEventListener(FigGlobalErrorOccurred, (event) => {
        const response = event.detail;
        console.error(response.error);
    });
    document.addEventListener(FigProtoMessageReceived, (event) => {
        const raw = event.detail;
        const bytes = b64ToBytes(raw);
        const message = fromBinary(ServerOriginatedMessageSchema, bytes);
        receivedMessage(message);
    });
};
// We want this to be run automatically
if (!window?.fig?.quiet) {
    console.log("[q] setting up event listeners...");
}
setupEventListeners();
//# sourceMappingURL=core.js.map