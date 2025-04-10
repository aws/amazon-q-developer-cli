import { GetPlatformInfoResponse } from "@aws/amazon-q-developer-cli-proto/fig";
import { sendGetPlatformInfoRequest } from "./requests.js";
import {
  AppBundleType,
  DesktopEnvironment,
  DisplayServerProtocol,
  Os,
} from "@aws/amazon-q-developer-cli-proto/fig";

export { AppBundleType, DesktopEnvironment, DisplayServerProtocol, Os };

export function getPlatformInfo(): Promise<GetPlatformInfoResponse> {
  return sendGetPlatformInfoRequest({});
}
