import logger from "loglevel";

import { executeCommand } from "@aws/amazon-q-developer-cli-api-bindings-wrappers";
import { runPipingConsoleMethods } from "../utils";
import {
  runCachedGenerator,
  GeneratorContext,
  haveContextForGenerator,
} from "./helpers";

export async function getCustomSuggestions(
  generator: Fig.Generator,
  context: GeneratorContext,
): Promise<Fig.Suggestion[]> {
  if (!generator.custom) {
    return [];
  }

  if (!haveContextForGenerator(context)) {
    logger.info("Don't have context for custom generator");
    return [];
  }

  const {
    tokenArray,
    currentWorkingDirectory,
    currentProcess,
    isDangerous,
    searchTerm,
    environmentVariables,
  } = context;

  try {
    const result = await runCachedGenerator(
      generator,
      context,
      () =>
        runPipingConsoleMethods(() =>
          generator.custom!(tokenArray, executeCommand, {
            currentWorkingDirectory,
            currentProcess,
            sshPrefix: "",
            searchTerm,
            environmentVariables,
            isDangerous,
          }),
        ),
      generator.cache?.cacheKey,
    );

    return result.map((name) => ({ ...name, type: name.type || "arg" }));
  } catch (e) {
    logger.error("we had an error with the custom function generator", e);

    return [];
  }
}
