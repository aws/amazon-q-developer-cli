import React from "react";
import { basename, resolve } from "node:path";
import { render } from "twinki";
import { ShowcaseClient } from "./acp-client.js";
import { App } from "./App.js";
import { themeById } from "./themes.js";
import { scanWorkspace } from "./workspace.js";

function valuesAfter(args: string[], flag: string): string[] {
  return args.flatMap((value, index) => (value === flag && args[index + 1] ? [args[index + 1]!] : []));
}

async function main(): Promise<void> {
  const args = process.argv.slice(2);
  const valueAfter = (flag: string): string | undefined => {
    const index = args.indexOf(flag);
    return index >= 0 ? args[index + 1] : undefined;
  };
  const cwd = resolve(valueAfter("--cwd") ?? process.cwd());
  const command = valueAfter("--command") ?? "kiro-cli";
  const engine = valueAfter("--engine") ?? "v3";
  const explicitArgs = valuesAfter(args, "--agent-arg");
  const isKiro = basename(command).startsWith("kiro-cli");
  const commandArgs = isKiro ? ["acp", "--agent-engine", engine, ...explicitArgs] : explicitArgs;
  const client = new ShowcaseClient({
    command,
    args: commandArgs,
    cwd,
  });
  const instance = render(
    <App
      client={client}
      workspaceRoot={cwd}
      files={scanWorkspace(cwd)}
      initialTheme={themeById(valueAfter("--theme"))}
      engine={isKiro ? `KAS ${engine.toUpperCase()}` : basename(command)}
      steeringEnabled={isKiro && engine === "v3"}
    />,
    {
      fullscreen: true,
      mouse: true,
      textSelection: true,
      exitOnCtrlC: true,
    },
  );
  const stop = () => instance.unmount();
  process.once("SIGINT", stop);
  process.once("SIGTERM", stop);
  try {
    await instance.waitUntilExit();
  } finally {
    process.off("SIGINT", stop);
    process.off("SIGTERM", stop);
    await client.close();
  }
}

main().catch((error: unknown) => {
  console.error(error instanceof Error ? error.message : String(error));
  process.exitCode = 1;
});
