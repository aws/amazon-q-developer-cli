import { sendGetPlatformInfoRequest } from "./requests.js";
import { AppBundleType, DesktopEnvironment, DisplayServerProtocol, Os, } from "@aws/amazon-q-developer-cli-proto/fig";
export { AppBundleType, DesktopEnvironment, DisplayServerProtocol, Os };
export function getPlatformInfo() {
    return sendGetPlatformInfoRequest({});
}
//# sourceMappingURL=platform.js.map