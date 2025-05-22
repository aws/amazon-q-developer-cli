import { type ServerOriginatedMessage, type ClientOriginatedMessage } from "@aws/amazon-q-developer-cli-proto/fig";
type shouldKeepListening = boolean;
export type APIResponseHandler = (response: ServerOriginatedMessage["submessage"]) => shouldKeepListening | void;
export declare function setHandlerForId(handler: APIResponseHandler, id: string): void;
export declare function sendMessage(submessage: ClientOriginatedMessage["submessage"], handler?: APIResponseHandler): void;
export {};
