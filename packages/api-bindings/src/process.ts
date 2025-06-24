import { create } from "@bufbuild/protobuf";
import { sendRunProcessRequest } from "./requests.js";
import {
  DurationSchema,
  EnvironmentVariableSchema,
} from "@aws/amazon-q-developer-cli-proto/fig_common";

export async function run({
  executable,
  args,
  environment,
  workingDirectory,
  terminalSessionId,
  timeout,
}: {
  executable: string;
  args: string[];
  environment?: Record<string, string | undefined>;
  workingDirectory?: string;
  terminalSessionId?: string;
  timeout?: number;
}) {
  const env = environment ?? {};
  return sendRunProcessRequest({
    executable,
    arguments: args,
    env: Object.keys(env).map((key) =>
      create(EnvironmentVariableSchema, { key, value: env[key] }),
    ),
    workingDirectory,
    terminalSessionId,
    timeout: timeout
      ? create(DurationSchema, {
          nanos: Math.floor((timeout % 1000) * 1_000_000_000),
          secs: BigInt(Math.floor(timeout / 1000)),
        })
      : undefined,
  });
}
