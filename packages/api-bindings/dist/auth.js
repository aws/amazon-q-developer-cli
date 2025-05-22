import { AuthStatusResponse_AuthKind, AuthBuilderIdPollCreateTokenResponse_PollStatus as PollStatus, } from "@aws/amazon-q-developer-cli-proto/fig";
import { sendAuthBuilderIdStartDeviceAuthorizationRequest, sendAuthBuilderIdPollCreateTokenRequest, sendAuthFinishPkceAuthorizationRequest, sendAuthStatusRequest, sendAuthStartPkceAuthorizationRequest, sendAuthCancelPkceAuthorizationRequest, } from "./requests.js";
export function status() {
    return sendAuthStatusRequest({}).then((res) => {
        let authKind;
        switch (res.authKind) {
            case AuthStatusResponse_AuthKind.BUILDER_ID:
                authKind = "BuilderId";
                break;
            case AuthStatusResponse_AuthKind.IAM_IDENTITY_CENTER:
                authKind = "IamIdentityCenter";
                break;
            default:
                break;
        }
        return {
            authed: res.authed,
            authKind,
            startUrl: res.startUrl,
            region: res.region,
        };
    });
}
export function startPkceAuthorization({ region, issuerUrl, } = {}) {
    return sendAuthStartPkceAuthorizationRequest({
        region,
        issuerUrl,
    });
}
export function finishPkceAuthorization({ authRequestId, }) {
    return sendAuthFinishPkceAuthorizationRequest({ authRequestId });
}
export function cancelPkceAuthorization() {
    return sendAuthCancelPkceAuthorizationRequest({});
}
export function builderIdStartDeviceAuthorization({ region, startUrl, } = {}) {
    return sendAuthBuilderIdStartDeviceAuthorizationRequest({
        region,
        startUrl,
    });
}
export async function builderIdPollCreateToken({ authRequestId, expiresIn, interval, }) {
    for (let i = 0; i < Math.ceil(expiresIn / interval); i += 1) {
        await new Promise((resolve) => {
            setTimeout(resolve, interval * 1000);
        });
        const pollStatus = await sendAuthBuilderIdPollCreateTokenRequest({
            authRequestId,
        });
        switch (pollStatus.status) {
            case PollStatus.COMPLETE:
                return;
            case PollStatus.PENDING:
                break;
            case PollStatus.ERROR:
                console.error("Failed to poll builder id token", pollStatus);
                throw new Error(pollStatus.error);
            default:
                throw new Error(`Unknown poll status: ${pollStatus.status}`);
        }
    }
}
//# sourceMappingURL=auth.js.map