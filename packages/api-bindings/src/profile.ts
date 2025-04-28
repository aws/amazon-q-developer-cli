import { sendListAvailableProfilesRequest } from "./requests.js";

export async function listAvailableProfiles() {
  return sendListAvailableProfilesRequest({});
}
