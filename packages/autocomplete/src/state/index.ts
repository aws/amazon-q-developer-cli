import React from "react";
import logger from "loglevel";
import { Mutate, StateCreator, StoreApi } from "zustand";
import { createWithEqualityFn } from "zustand/traditional";

import { Suggestion } from "@aws/amazon-q-developer-cli-shared/internal";
import {
  fieldsAreEqual,
  makeArray,
  memoizeOne,
} from "@aws/amazon-q-developer-cli-shared/utils";
import { AliasMap, getCommand } from "@aws/amazon-q-developer-cli-shell-parser";
import {
  ArgumentParserResult,
  initialParserState,
} from "@aws/amazon-q-developer-cli-autocomplete-parser";
import {
  getSetting,
  SETTINGS,
  SettingsMap,
} from "@aws/amazon-q-developer-cli-api-bindings-wrappers";
import { type Types } from "@aws/amazon-q-developer-cli-api-bindings";
import { detailedDiff } from "deep-object-diff";
import { trackEvent } from "../telemetry.js";
import { FigState, initialFigState } from "../fig/hooks";
import { AutocompleteState, NamedSetState, Visibility } from "./types";

import { updatePriorities } from "../suggestions/sorting";
import {
  deduplicateSuggestions,
  filterSuggestions,
  getAllSuggestions,
} from "../suggestions";

import { GeneratorState } from "../generators/helpers";

import { getFullHistorySuggestions } from "../history";
import { createGeneratorState } from "./generators";
import { createInsertionState } from "./insertion";

const initialState: Partial<AutocompleteState> = {
  figState: initialFigState,
  parserResult: initialParserState,
  generatorStates: [] as GeneratorState[],
  command: null,

  visibleState: Visibility.HIDDEN_UNTIL_KEYPRESS,
  lastInsertedSuggestion: null,
  justInserted: false,

  selectedIndex: 0,
  suggestions: [] as Suggestion[],
  hasChangedIndex: false,

  historyModeEnabled: false,
  fuzzySearchEnabled: false,
  userFuzzySearchEnabled: getSetting(SETTINGS.FUZZY_SEARCH, false) as boolean,
  settings: {} as SettingsMap,
};

const getCommandMemoized = memoizeOne(getCommand);

const getEnvVarsMemoized = memoizeOne(
  (arr: Types.EnvironmentVariable[]) =>
    Object.fromEntries(
      arr
        .map((pair) => [pair.key, pair.value])
        .filter((pair) => pair[0] && pair[1]),
    ),
  ([a], [b]) =>
    a.length === b.length &&
    a.every(
      (elem, index) =>
        elem.key === b[index].key && elem.value === b[index].value,
    ),
);

const getAliasMemoized = memoizeOne(
  (alias: string, shellPath?: string) => {
    const separator = shellPath?.includes("fish") ? " " : "=";
    return alias
      .replace(/^alias\s/gm, "")
      .split("\n")
      .reduce((acc, a) => {
        try {
          const [key, ...value] = a.split(separator);
          acc[key] = value.join(separator).replace(/^'/, "").replace(/'$/, "");
        } catch (err) {
          logger.error(`Error parsing alias: ${a}`, err);
        }
        return acc;
      }, {} as AliasMap);
  },
  ([aliasA, shellPathA], [aliasB, shellPathB]) =>
    aliasA === aliasB && shellPathA === shellPathB,
);

const computeSuggestions = (
  state: AutocompleteState,
  settings: SettingsMap,
) => {
  const {
    parserResult,
    command,
    figState: { buffer, processUserIsIn },
  } = state;
  const { annotations } = parserResult;

  const historySuggestions = getFullHistorySuggestions(
    buffer,
    command,
    processUserIsIn || "",
  );

  const specSuggestions = getAllSuggestions(
    parserResult.currentArg,
    parserResult.completionObj,
    parserResult.passedOptions,
    parserResult.suggestionFlags,
    state.generatorStates,
    annotations,
  );

  let suggestions = specSuggestions;
  const historyMode = settings[SETTINGS.HISTORY_MODE] as string;
  if (historyMode === "show") {
    const existingNames = new Set();
    suggestions.forEach((suggestion) => {
      makeArray(suggestion.name || []).forEach((name) =>
        existingNames.add(name || ""),
      );
    });
    // Remove suggestions whose names are already in fig's suggestions
    suggestions.push(
      ...historySuggestions.filter((x) => !existingNames.has(x.name)),
    );

    const historyPriorities: Record<string, number> = {};
    for (let i = 0; i < historySuggestions.length; i += 1) {
      const name = makeArray(historySuggestions[i].name)[0] || "";
      historyPriorities[name.split(" ", 1)[0]] =
        historySuggestions[i].priority || 50;
    }
    suggestions = updatePriorities(suggestions, command?.tokens[0]?.text ?? "");

    // Take highest priority if suggestion was in historySuggestions
    suggestions = suggestions.map((suggestion) => ({
      ...suggestion,
      priority: Math.max(
        suggestion.priority || 0,
        ...makeArray(suggestion.name).map(
          (name) => historyPriorities[name || ""] || 0,
        ),
      ),
    }));
  } else if (historyMode === "history_only") {
    suggestions = historySuggestions;
  } else if (state.historyModeEnabled) {
    suggestions = historySuggestions;
  }

  suggestions = updatePriorities(
    suggestions,
    command?.tokens[0]?.text ?? "",
  ).sort((a, b) => (b.priority || 0) - (a.priority || 0));

  const filtered = filterSuggestions(
    suggestions,
    parserResult.searchTerm,
    state.fuzzySearchEnabled,
    parserResult.currentArg?.suggestCurrentToken ??
      getSetting(SETTINGS.ALWAYS_SUGGEST_CURRENT_TOKEN, false),
    settings,
  );

  // Deduplication is relatively slow, so we only do it if there aren't
  // enough suggestions to cause any noticeable performance issues.
  if (filtered.length > 0 && filtered.length <= 50) {
    return deduplicateSuggestions(filtered, [
      "name",
      "insertValue",
      "displayName",
      "args",
    ]);
  }
  return filtered;
};

const updateSuggestions =
  (creator: StateCreator<AutocompleteState>): StateCreator<AutocompleteState> =>
  (set, get, api) =>
    creator(
      (partial, replace) => {
        const state = get();
        const update = typeof partial === "function" ? partial(state) : partial;
        const updatedState = { ...state, ...update };

        if (logger.getLevel() <= logger.levels.DEBUG) {
          logger.debug({ diff: detailedDiff(state, updatedState) });
        }

        logger.debug({ state, updatedState });
        logger.info({ tokens: updatedState.command?.tokens });

        const suggestionsMayChange =
          !fieldsAreEqual(updatedState, state, [
            "command",
            "fuzzySearchEnabled",
            "parserResult",
            "generatorStates",
            "historyModeEnabled",
          ]) ||
          !fieldsAreEqual(updatedState.figState, state.figState, [
            "cwd",
            "buffer",
          ]);
        const wasVisible = state.visibleState === Visibility.VISIBLE;
        const willBeVisible = updatedState.visibleState === Visibility.VISIBLE;

        if (
          updatedState.visibleState === Visibility.HIDDEN_UNTIL_KEYPRESS ||
          (willBeVisible === wasVisible && !suggestionsMayChange)
        ) {
          set(updatedState, replace);
          return;
        }

        // Recompute suggestions if we change visibility state or will be visible and
        // suggestions might change.
        const { settings } = state;
        const suggestions = computeSuggestions(updatedState, settings);
        logger.info("Recomputed suggestions", { suggestions });

        if (willBeVisible && !updatedState.figState.cwd) {
          trackEvent("autocomplete-no-context", {});
        }

        // If old state is visible, and user has changed the selected item try to
        // keep the same item selected.
        let selectedIndex = 0;
        let hasChangedIndex = false;
        const lastSelected = state.suggestions[state.selectedIndex];

        if (state.hasChangedIndex && willBeVisible && lastSelected) {
          const newIndex = suggestions.findIndex((suggestion: Suggestion) =>
            fieldsAreEqual(suggestion, lastSelected, [
              "name",
              "type",
              "insertValue",
              "description",
            ]),
          );
          if (newIndex !== -1) {
            selectedIndex = newIndex;
            hasChangedIndex = state.hasChangedIndex;
          }
        }

        set(
          { ...state, ...update, suggestions, selectedIndex, hasChangedIndex },
          replace,
        );
      },
      get,
      api,
    );

type Get<T, K, F> = K extends keyof T ? T[K] : F;
type NamedStateCreator<T> = (
  setState: NamedSetState<T>,
  getState: Get<Mutate<StoreApi<T>, []>, "getState", never>,
  store: Mutate<StoreApi<T>, []>,
) => T;

const log =
  <T>(config: NamedStateCreator<T>): StateCreator<T, [], []> =>
  (set, get, api) => {
    const namedSet: NamedSetState<T> = (name, partial, replace) => {
      console.groupCollapsed(`applying update: ${name}`);
      logger.info({ name, partial, replace });
      set(partial, replace);
      console.groupEnd();
    };
    return config(namedSet, get, api);
  };

export const useAutocompleteStore = createWithEqualityFn<AutocompleteState>(
  updateSuggestions(
    log((setNamed, get) => {
      const generatorState = createGeneratorState(setNamed, get);
      const insertionState = createInsertionState(setNamed, get);

      return {
        ...(initialState as AutocompleteState),
        ...insertionState,

        setParserResult: (
          parserResult: ArgumentParserResult,
          hasBackspacedToNewToken: boolean,
          largeBufferChange: boolean,
        ) =>
          setNamed("setParserResult", (state) => {
            let { visibleState, generatorStates } = state;

            // Trigger all generators on currentArg change.
            const hasNewArg = !fieldsAreEqual(
              parserResult,
              state.parserResult,
              ["currentArg", "completionObj"],
            );

            console.log(parserResult);

            const newGeneratorStates =
              generatorState.triggerGenerators(parserResult);
            if (
              newGeneratorStates.length !== generatorStates.length ||
              generatorStates.some((s, idx) => s !== newGeneratorStates[idx])
            ) {
              // Update if we've triggered at least one generator.
              generatorStates = newGeneratorStates;
            }

            const originalVisibleState = visibleState;
            switch (originalVisibleState) {
              case Visibility.HIDDEN_UNTIL_KEYPRESS: {
                visibleState = Visibility.VISIBLE;
                break;
              }
              case Visibility.HIDDEN_BY_INSERTION: {
                const insertionTriggeredGenerator = state.generatorStates.some(
                  (oldState, idx) =>
                    oldState.generator ===
                      state.lastInsertedSuggestion?.generator &&
                    generatorStates[idx] !== oldState,
                );

                visibleState =
                  hasNewArg || insertionTriggeredGenerator
                    ? Visibility.VISIBLE
                    : Visibility.HIDDEN_UNTIL_KEYPRESS;
                break;
              }
              default:
                break;
            }

            if (getSetting<boolean>(SETTINGS.ONLY_SHOW_ON_TAB, false)) {
              visibleState = originalVisibleState;
              if (hasNewArg) {
                visibleState = Visibility.HIDDEN_UNTIL_SHOWN;
              }
            }

            if (hasBackspacedToNewToken) {
              visibleState = Visibility.HIDDEN_UNTIL_KEYPRESS;
            }

            if (largeBufferChange && !state.justInserted) {
              visibleState = Visibility.HIDDEN_UNTIL_KEYPRESS;
            }

            const userFuzzySearchEnabled = hasNewArg
              ? getSetting<boolean>(SETTINGS.FUZZY_SEARCH, false)
              : state.userFuzzySearchEnabled;

            const isFuzzySearchEnabled = () => {
              // here we decide whether to use user's fuzzy search setting or not
              switch (
                parserResult.currentArg
                  ? parserResult.currentArg.filterStrategy
                  : parserResult.completionObj.filterStrategy
              ) {
                case "prefix":
                  return false;
                case "fuzzy":
                  return true;
                case "default":
                default:
                  return userFuzzySearchEnabled;
              }
            };

            return {
              parserResult,
              visibleState,
              fuzzySearchEnabled: isFuzzySearchEnabled(),
              userFuzzySearchEnabled,
              generatorStates,
              justInserted: false,
            };
          }),

        scroll: (index: number, visibleState: Visibility) =>
          setNamed("scroll", (state) => {
            const selectedIndex = Math.max(
              Math.min(index, state.suggestions.length - 1),
              0,
            );
            return {
              selectedIndex,
              visibleState,
              hasChangedIndex: selectedIndex !== state.selectedIndex,
            };
          }),

        setVisibleState: (visibleState: Visibility) =>
          setNamed("setVisibleState", { visibleState }),

        setHistoryModeEnabled: (
          historyModeEnabled: React.SetStateAction<boolean>,
        ) =>
          setNamed("setHistoryModeEnabled", (state) => ({
            historyModeEnabled:
              typeof historyModeEnabled === "function"
                ? historyModeEnabled(state.historyModeEnabled)
                : historyModeEnabled,
          })),

        setUserFuzzySearchEnabled: (
          userFuzzySearchEnabled: React.SetStateAction<boolean>,
        ) =>
          setNamed("setUserFuzzySearchEnabled", (state) => ({
            userFuzzySearchEnabled:
              typeof userFuzzySearchEnabled === "function"
                ? userFuzzySearchEnabled(state.userFuzzySearchEnabled)
                : userFuzzySearchEnabled,
          })),

        setFigState: (newFigState: React.SetStateAction<FigState>) =>
          /*
           * IMPORTANT: any computation in this function must be memoized,
           * otherwise it will trigger setParserResult every time it's called
           */
          setNamed("setFigState", (state) => {
            const figState =
              typeof newFigState === "function"
                ? newFigState(state.figState)
                : newFigState;

            const {
              sshContextString,
              cwd,
              buffer,
              cursorLocation,
              aliases,
              shellContext,
            } = figState;

            // Set globalCWD and SSH string for completion specs that use executeShellCommand
            if (sshContextString) {
              window.globalSSHString = sshContextString;
            }

            if (shellContext) {
              window.globalTerminalSessionId = shellContext.sessionId;

              if (shellContext.environmentVariables) {
                figState.environmentVariables = getEnvVarsMemoized(
                  shellContext.environmentVariables,
                );
              }

              if (shellContext.alias) {
                figState.aliases = getAliasMemoized(
                  shellContext.alias,
                  shellContext.shellPath,
                );
              }
            }

            const error = (msg: string): AutocompleteState => {
              logger.debug(msg);
              return {
                ...state,
                ...initialState,
                settings: state.settings,
                figState,
              };
            };

            if (cwd) {
              window.globalCWD = cwd;
            }

            const cursorChar = buffer.charAt(cursorLocation);
            if (cursorChar.trim()) {
              return error("Cursor is in the middle of a word.");
            }

            const bufferSliced = buffer.slice(0, cursorLocation);

            try {
              const command = getCommandMemoized(
                bufferSliced,
                aliases,
                cursorLocation,
              );
              return { figState, command };
            } catch (err) {
              return error(`Failed to get token array: ${err}`);
            }
          }),

        error: (error: string) =>
          setNamed("error", (state) => {
            logger.warn(error);
            return {
              ...initialState,
              figState: state.figState,
              settings: state.settings,
            };
          }),

        setSettings: (settings: React.SetStateAction<SettingsMap>) =>
          setNamed("setSettings", (state) => ({
            settings:
              typeof settings === "function"
                ? settings(state.settings)
                : settings,
          })),
      };
    }),
  ),
);
