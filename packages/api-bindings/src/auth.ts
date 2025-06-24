import {
  AuthBuilderIdStartDeviceAuthorizationResponse,
  AuthCancelPkceAuthorizationResponse,
  AuthStartPkceAuthorizationResponse,
  AuthStatusResponse_AuthKind,
  AuthBuilderIdPollCreateTokenResponse_PollStatus as PollStatus,
} from "@aws/amazon-q-developer-cli-proto/fig";
import {
  sendAuthBuilderIdStartDeviceAuthorizationRequest,
  sendAuthBuilderIdPollCreateTokenRequest,
  sendAuthFinishPkceAuthorizationRequest,
  sendAuthStatusRequest,
  sendAuthStartPkceAuthorizationRequest,
  sendAuthCancelPkceAuthorizationRequest,
} from "./requests.js";
import { AuthFinishPkceAuthorizationResponse } from "@aws/amazon-q-developer-cli-proto/fig";
import { AuthFinishPkceAuthorizationRequest } from "@aws/amazon-q-developer-cli-proto/fig";

export function status() {
  return sendAuthStatusRequest({}).then((res) => {
    let authKind: "BuilderId" | "IamIdentityCenter" | undefined;
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

export function startPkceAuthorization({
  region,
  issuerUrl,
}: {
  region?: string;
  issuerUrl?: string;
} = {}): Promise<AuthStartPkceAuthorizationResponse> {
  return sendAuthStartPkceAuthorizationRequest({
    region,
    issuerUrl,
  });
}

export function finishPkceAuthorization({
  authRequestId,
}: Omit<
  AuthFinishPkceAuthorizationRequest,
  "$typeName"
>): Promise<AuthFinishPkceAuthorizationResponse> {
  return sendAuthFinishPkceAuthorizationRequest({ authRequestId });
}

export function cancelPkceAuthorization(): Promise<AuthCancelPkceAuthorizationResponse> {
  return sendAuthCancelPkceAuthorizationRequest({});
}

export function builderIdStartDeviceAuthorization({
  region,
  startUrl,
}: {
  region?: string;
  startUrl?: string;
} = {}): Promise<AuthBuilderIdStartDeviceAuthorizationResponse> {
  return sendAuthBuilderIdStartDeviceAuthorizationRequest({
    region,
    startUrl,
  });
}

export async function builderIdPollCreateToken({
  authRequestId,
  expiresIn,
  interval,
}: AuthBuilderIdStartDeviceAuthorizationResponse) {
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
