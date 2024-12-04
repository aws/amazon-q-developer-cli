import {
  LocalStateMap,
  State,
  States,
} from "@aws/amazon-q-developer-cli-api-bindings-wrappers";
import { ensureTrailingSlash } from "@aws/amazon-q-developer-cli-shared/utils";
import { AuthClient } from "./auth.js";
import { GenericRequestError } from "./errors.js";

const getApiUrl = (state: LocalStateMap): string =>
  state[States.DEVELOPER_API_HOST] ??
  state[States.DEVELOPER_AE_API_HOST] ??
  "https://api.fig.io";

State.subscribe({
  changes(oldState, newState) {
    if (getApiUrl(oldState) !== getApiUrl(newState)) {
      window.resetCaches?.();
    }
    return undefined;
  },
});

export const makeRequest = async (
  route?: string,
  authClient?: AuthClient | undefined,
  body: BodyInit | null = null,
  options: {
    additionalHeaders?: HeadersInit;
    requestType: "GET" | "POST";
  } = {
    requestType: "GET",
  },
) => {
  const creds = await authClient?.getCredentialsFromFile();
  const authHeader = creds
    ? authClient?.makeAuthHeaderForCredentials(creds)
    : undefined;
  const currentState = State.current();
  const url = `${ensureTrailingSlash(getApiUrl(currentState))}${route ?? ""}`;

  const response = await fetch(url, {
    method: options.requestType === "POST" ? "POST" : "GET",
    headers: {
      ...(authHeader && { Authorization: authHeader }),
      ...(options.additionalHeaders && { ...options.additionalHeaders }),
    },
    ...(body && { body }),
  });

  if (response.ok) {
    return response;
  }
  throw new GenericRequestError(`Could not make request to ${url}`);
};
