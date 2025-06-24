import { sendOpenInExternalApplicationRequest } from "./requests.js";

export function open(url: string) {
  return sendOpenInExternalApplicationRequest({ url });
}
