import { sendUserLogoutRequest } from "./requests.js";

export async function logout() {
  return sendUserLogoutRequest({});
}
