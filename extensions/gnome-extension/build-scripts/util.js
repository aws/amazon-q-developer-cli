import { exec } from "node:child_process";
import { env, getuid } from "node:process";
import { promisify } from "node:util";

/** @constant @type {(command: string) => Promise<string>} */
const run = (() => {
  const aexec = promisify(exec);
  return async (command) => {
    return (await aexec(command)).stdout.trim();
  };
})();

/** @public @async @function @param {{ directory: boolean }} options @returns {Promise<string>} */
export async function mktemp(options) {
  options = {
    directory: false,
    ...options,
  };

  if (env.XDG_RUNTIME_DIR !== undefined) {
    return run(
      `mktemp ${options.directory ? "--directory" : ""} '${env.XDG_RUNTIME_DIR}/fig-gse-build-temp-XXXX'`,
    );
  }

  try {
    return await run(
      `mktemp ${options.directory ? "--directory" : ""} '/run/user/${getuid()}/fig-gse-build-temp-XXXX'`,
    );
  } catch {
    return await run(
      `mktemp ${options.directory ? "--directory" : ""} 'fig-gse-build-temp-XXXX'`,
    );
  }
}
