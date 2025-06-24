import { Internal } from "@fig/autocomplete-shared";
import { getLogger } from "loglevel";
import {
  parseArguments,
  TokenType,
  flattenAnnotations,
  BasicAnnotation,
  Annotation,
  getSpecPath,
  loadSubcommandCached,
  serializeSpecLocation,
} from "@aws/amazon-q-developer-cli-autocomplete-parser";
import {
  getAllCommandsWithAlias,
  AliasMap,
  Command,
} from "@aws/amazon-q-developer-cli-shell-parser";
import { SpecLocationSource } from "@aws/amazon-q-developer-cli-shared/utils";
import { Suggestion } from "@aws/amazon-q-developer-cli-shared/internal";
import {
  executeCommand,
  executeLoginShell,
  SETTINGS,
  getSetting,
} from "@aws/amazon-q-developer-cli-api-bindings-wrappers";
import { History } from "@aws/amazon-q-developer-cli-api-bindings";
import {
  AnnotatedCommand,
  HistoryEntry,
  Graph,
  mirrorSubcommand,
  walkSubcommand,
  HistoryContext,
} from "./helpers";
import { MissingSpecError } from "./errors";

const historyLogger = getLogger("history");
historyLogger.setDefaultLevel("warn");

type HistoryArgValues = {
  values: { historyEntryIndex: number; value: string }[];
};

type HistoryValueIndex = Record<
  string,
  Internal.Subcommand<HistoryArgValues, unknown, unknown>
>;

type HistorySource = {
  entries: HistoryEntry[];
  entryGraph?: Graph;
  subcommandGraph?: Graph;
  historyValueIndex?: HistoryValueIndex;
};

const historySources: Record<string, HistorySource> = {
  zsh: { entries: [] },
  fig: { entries: [] },
  bash: { entries: [] },
  fish: { entries: [] },
  customCommand: { entries: [] },
};

const indexArgumentValues = (
  historyValueIndex: HistoryValueIndex,
  entry: HistoryEntry,
  entryIndex: number,
) => {
  const { commands } = entry;
  commands.forEach(({ parserResult }) => {
    if (parserResult) {
      const annotations = flattenAnnotations(parserResult.annotations);

      // Split up annotations into separate loadSpec components.
      // Each list of annotations in perSpecAnnotations starts with a { spec, specLocation };
      const perSpecAnnotations = annotations.reduce((acc, annotation) => {
        if ("spec" in annotation) {
          acc.push([annotation]);
        } else {
          acc[acc.length - 1]?.push(annotation);
        }
        return acc;
      }, [] as BasicAnnotation[][]);

      // For each string of annotations, we
      // 1. load/create the mirrored subcommand in the historyValueIndex.
      // 2. Walk down the mirrored subcommand and add arg values from each annotation.
      for (let i = 0; i < perSpecAnnotations.length; i += 1) {
        const [root, ...other] = perSpecAnnotations[i];
        if (!("spec" in root)) {
          throw new MissingSpecError("Undefined spec or specLocation");
        }

        const { spec, specLocation } = root;
        const specKey = serializeSpecLocation(specLocation);

        if (!historyValueIndex[specKey]) {
          historyValueIndex[specKey] = mirrorSubcommand(
            spec,
            () => ({ values: [] }) as HistoryArgValues,
            () => ({}),
            () => ({}),
          );
        }

        try {
          walkSubcommand(historyValueIndex[specKey], other, {
            arg: (arg, annotation) => {
              arg.values.push({
                historyEntryIndex: entryIndex,
                value: annotation.text,
              });
            },
          });
        } catch (err) {
          historyLogger.error("Error walking subcommand:", err);
        }
      }
    }
  });
};

const awaitBatched = async <T>(
  batchSize: number,
  getPromiseArray: Array<() => Promise<T>>,
): Promise<T[]> => {
  const results: T[] = [];
  for (let i = 0; i < getPromiseArray.length; i += batchSize) {
    const promises = getPromiseArray
      .slice(i, i + batchSize)
      .map((getPromise) => getPromise());
    const batchResults = await Promise.all(promises);
    results.push(...batchResults);
  }
  return results;
};

export const loadHistorySource = async (
  items: HistoryEntry[],
  aliases: AliasMap,
): Promise<HistorySource> => {
  const entryMap: Record<string, HistoryEntry> = {};
  const allLocations: string[] = [];

  const fakeCWD = "_fig/fake_cwd";
  const getCommandLocation = async (
    command: AnnotatedCommand,
    context?: Partial<HistoryContext>,
  ) =>
    getSpecPath(command.command.tokens[0]?.text ?? "", context?.cwd ?? fakeCWD);

  for (let i = 0; i < items.length; i += 1) {
    const { text, context } = items[i];
    if (!(text in entryMap)) {
      let commands: AnnotatedCommand[] = [];
      try {
        commands = getAllCommandsWithAlias(text, aliases).map((command) => ({
          command,
        }));
      } catch (err) {
        historyLogger.warn("Error getting commands in history", err);
      }
      entryMap[text] = { text, commands };

      for (const command of commands) {
        const location = await getCommandLocation(command, context);
        if (location.type === SpecLocationSource.GLOBAL) {
          allLocations.push(location.name);
        }
      }
    }
  }

  const locations = [...new Set(allLocations)];
  const locationPromises = locations.map(
    (name) => () =>
      loadSubcommandCached(
        { name, type: SpecLocationSource.GLOBAL },
        undefined,
        historyLogger,
      ).catch((e) => e),
  );

  // Pre-load all specs from history.
  await awaitBatched(10, locationPromises);

  const entries: HistoryEntry[] = [];
  for (let i = 0; i < items.length; i += 1) {
    const { text, context } = items[i];
    const entry = { ...entryMap[text], context };
    for (let j = 0; j < entry.commands.length; j += 1) {
      const command = entry.commands[j];
      const location = await getCommandLocation(command, context);
      if (
        !command.parserResult &&
        location.type === SpecLocationSource.GLOBAL
      ) {
        try {
          const parseContext: Fig.ShellContext = {
            currentWorkingDirectory: context?.cwd ?? fakeCWD,
            sshPrefix: "",
            currentProcess: "",
            environmentVariables: {},
          };
          command.parserResult = await parseArguments(
            command.command,
            parseContext,
            true,
            historyLogger,
          );
        } catch (_err) {
          // skip errors in parsing commands.
        }
      }
    }
    entries.push(entry);
  }

  const historyValueIndex = {};
  entries.forEach((entry, index) => {
    try {
      indexArgumentValues(historyValueIndex, entry, index);
    } catch (err) {
      historyLogger.warn({ err });
    }
  });

  return { entries, historyValueIndex };
};

const loadFigHistory = async (): Promise<HistoryEntry[]> =>
  History.query(
    "SELECT command, shell, session_id, cwd, start_time, exit_code FROM history",
    [],
  ).then((history) =>
    history.map(
      (entry) =>
        ({
          text: entry.command,
          context: {
            cwd: entry.cwd,
            shell: entry.shell,
            exitCode: entry.exit_code,
            terminalSession: entry.session_id,
            time: entry.start_time,
          },
          commands: [],
        }) as HistoryEntry,
    ),
  );

export const loadHistory = async (aliases: AliasMap) => {
  const loadHistoryCommand = async (
    command: string,
    shell?: string,
  ): Promise<HistorySource> => {
    let items: HistoryEntry[] = [];
    try {
      let history: string;
      if (shell) {
        history = await executeLoginShell({ command, shell });
      } else {
        history = (
          await executeCommand({
            command: "zsh",
            args: ["-c", command],
          })
        ).stdout;
      }
      items = history
        .split("\n")
        .map((text) => ({ text, context: {}, commands: [] }));
    } catch (err) {
      historyLogger.error(err as Error);
    }

    const key = shell ?? "customCommand";
    const start = performance.now();
    const result = await loadHistorySource(items, aliases);
    historyLogger.info("Got history entries", {
      ...result,
      key,
      time: performance.now() - start,
    });
    historySources[key] = result;
    return result;
  };

  const entries = await loadFigHistory();
  const start = performance.now();
  historySources.fig = await loadHistorySource(entries, aliases);
  historyLogger.info("Got fig history", {
    ...historySources.fig,
    time: performance.now() - start,
  });

  const command = getSetting<string>(SETTINGS.HISTORY_COMMAND);
  if (command) {
    loadHistoryCommand(command);
  } else {
    await loadHistoryCommand("fc -R; fc -ln 1", "zsh").catch(() => {});
    await loadHistoryCommand("fc -ln 1", "bash").catch(() => {});
    await loadHistoryCommand("history search", "fish").catch(() => {});
  }
};

const getFirstWord = (text: string) => text.split(" ", 1)[0] || "";

export function findLastIndex<T>(
  array: Array<T>,
  predicate: (value: T, index: number) => boolean,
): number {
  for (let l = array.length - 1; l >= 0; l -= 1) {
    if (predicate(array[l], l)) return l;
  }
  return -1;
}

export const getHistoryArgSuggestions = (
  annotations: Annotation[],
  currentProcess: string,
): Fig.TemplateSuggestion[] => {
  const shell = currentProcess.slice(currentProcess.lastIndexOf("/") + 1);
  const basicAnnotations = flattenAnnotations(annotations);
  const index = findLastIndex(
    basicAnnotations,
    (annotation) => "spec" in annotation,
  );

  const root = basicAnnotations[index];
  const specAnnotations = basicAnnotations.slice(index + 1);

  if (!root || !("spec" in root) || specAnnotations.length === 0) {
    return [];
  }

  const specKey = serializeSpecLocation(root.specLocation);

  const getValuesFromSource = (source: HistorySource) => {
    const subcommand = source.historyValueIndex?.[specKey];
    if (!subcommand) {
      return [];
    }

    const getValuesForArgType = (
      type: TokenType.OptionArg | TokenType.SubcommandArg,
    ): Fig.TemplateSuggestion[] => {
      const result = walkSubcommand(subcommand, [
        ...specAnnotations.slice(0, -1),
        { text: "", type },
      ]);
      historyLogger.info({ result, type });
      if (result.type === type) {
        return (
          result.arg?.values
            ?.filter((item) => item.value)
            .map((item) => {
              const { historyEntryIndex, value } = item;
              const { context } = source.entries[historyEntryIndex];
              return {
                name: value,
                type: "arg",
                context: {
                  templateType: "history",
                  ...(context || {}),
                },
              };
            }) ?? []
        );
      }
      return [];
    };

    return [
      ...getValuesForArgType(TokenType.SubcommandArg),
      ...getValuesForArgType(TokenType.OptionArg),
    ];
  };

  const allValues: Fig.TemplateSuggestion[] = [];

  if (getSetting(SETTINGS.HISTORY_COMMAND)) {
    allValues.push(...getValuesFromSource(historySources.customCommand));
  } else if (
    getSetting(SETTINGS.HISTORY_MERGE_SHELLS) &&
    ["bash", "zsh"].includes(shell)
  ) {
    allValues.push(
      ...getValuesFromSource(historySources.zsh),
      ...getValuesFromSource(historySources.bash),
    );
  } else if (shell in historySources) {
    allValues.push(...getValuesFromSource(historySources[shell]));
  }

  if (allValues.length === 0) {
    allValues.push(...getValuesFromSource(historySources.fig));
  }

  return allValues;
};

export const getFullHistorySuggestions = (
  buffer: string,
  command: Command | null,
  currentProcess: string,
): Suggestion[] => {
  const shell = currentProcess.slice(currentProcess.lastIndexOf("/") + 1);
  const tokens = command?.tokens ?? [];
  const searchTermToken = tokens[tokens.length - 1];
  const prefixLength = searchTermToken?.originalNode
    ? searchTermToken.originalNode.startIndex +
      searchTermToken.originalNode.text.length -
      searchTermToken.originalNode.innerText.length
    : 0;
  const prefix = buffer.slice(0, prefixLength);
  const frequencies: Record<string, number> = {};
  const suggestions: string[] = [];
  const historyEntries: HistoryEntry[] = [];

  if (getSetting(SETTINGS.HISTORY_COMMAND)) {
    historyEntries.push(...historySources.customCommand.entries);
  } else if (
    getSetting(SETTINGS.HISTORY_MERGE_SHELLS) &&
    ["bash", "zsh"].includes(shell)
  ) {
    historyEntries.push(
      ...historySources.zsh.entries,
      ...historySources.bash.entries,
    );
  } else if (shell in historySources) {
    historyEntries.push(...historySources[shell].entries);
  }

  for (let i = 0; i < historyEntries.length; i += 1) {
    const { text } = historyEntries[i];
    if (text.length >= prefix.length && text.startsWith(prefix)) {
      const name = text.slice(prefix.length).trimEnd();
      suggestions.push(name);

      const firstWord = getFirstWord(name);
      if (firstWord) {
        if (!frequencies[firstWord]) {
          frequencies[firstWord] = 0;
        }
        frequencies[firstWord] += 1;
      }
    }
  }

  return [...new Set(suggestions)].map((suggestion) => {
    const frequency = frequencies[getFirstWord(suggestion)] || 0;
    return {
      type: "history",
      description: "past command",
      name: suggestion,
      insertValue: suggestion,
      icon: "📚",
      priority: frequency > 1 ? 75 + Math.min(frequency, 10) / 10 : 50,
    };
  });
};
