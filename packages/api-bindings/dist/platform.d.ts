import { GetPlatformInfoResponse } from "@aws/amazon-q-developer-cli-proto/fig";
import { AppBundleType, DesktopEnvironment, DisplayServerProtocol, Os } from "@aws/amazon-q-developer-cli-proto/fig";
export { AppBundleType, DesktopEnvironment, DisplayServerProtocol, Os };
export declare function getPlatformInfo(): Promise<GetPlatformInfoResponse>;
