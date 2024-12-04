import { fread } from "@aws/amazon-q-developer-cli-api-bindings-wrappers";
import { fs } from "@aws/amazon-q-developer-cli-api-bindings";
import { CredentialsError } from "./errors.js";

export interface AuthClient {
  makeAuthHeader: (accessToken?: string, idToken?: string) => string;
  makeAuthHeaderForCredentials: (credentials: Credentials) => string;
  getCredentialsFromFile: () => Promise<Credentials>;
  writeCredentialsToFile: (credentials: Credentials) => Promise<void>;
}

export type Platform = "macos" | "linux" | "windows";

/* eslint-disable camelcase */
export type Credentials = {
  refresh_token_expired?: boolean;
  access_token?: string;
  id_token?: string;
  refresh_token?: string;
};
/* eslint-enable camelcase */

// export interface MakeAuthClientOptions {
//   os?: Platform;
//   figDataDir?: string;
// }

// export const makeAuthClient = (options: MakeAuthClientOptions): AuthClient => {
//   const { figDataDir, os } = options;
//   /* eslint-disable no-nested-ternary */
//   const credentialsPath = figDataDir
//     ? `${figDataDir}/credentials.json`
//     : os === "linux"
//     ? "~/.local/share/fig/credentials.json"
//     : os === "windows"
//     ? "~/AppData/Local/fig/credentials.json"
//     : "~/Library/Application Support/fig/credentials.json";
//   /* eslint-enable no-nested-ternary */

//   const makeAuthHeader = (
//     accessToken: string | undefined,
//     idToken: string | undefined,
//   ) => `Bearer ${btoa(JSON.stringify({ accessToken, idToken }))}`;

//   const makeAuthHeaderForCredentials = (creds: Credentials) =>
//     makeAuthHeader(creds.access_token, creds.id_token);

//   const writeCredentialsToFile = async (credentials: Credentials) => {
//     try {
//       await fs.write(credentialsPath, JSON.stringify(credentials));
//     } catch (e) {
//       throw new CredentialsError(`Error reading credentials file ${e}`);
//     }
//   };

//   const getCredentialsFromFile = async () => {
//     try {
//       const contents = await fread(credentialsPath);
//       return JSON.parse(contents) as Credentials;
//     } catch (e) {
//       throw new CredentialsError(`Error reading credentials file ${e}`);
//     }
//   };

//   return {
//     makeAuthHeader,
//     makeAuthHeaderForCredentials,
//     getCredentialsFromFile,
//     writeCredentialsToFile,
//   };
// };
