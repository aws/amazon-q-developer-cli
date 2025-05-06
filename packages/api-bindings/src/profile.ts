import { ProfileSchema } from "@aws/amazon-q-developer-cli-proto/fig";
import {
  sendListAvailableProfilesRequest,
  sendSetProfileRequest,
} from "./requests.js";
import { create } from "@bufbuild/protobuf";

export async function listAvailableProfiles() {
  return sendListAvailableProfilesRequest({});
}

export async function setProfile(profileName: string, arn: string) {
  return sendSetProfileRequest({
    profile: create(ProfileSchema, { arn, profileName }),
  });
}
