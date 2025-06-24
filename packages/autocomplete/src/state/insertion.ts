import logger from "loglevel";
import { StoreApi } from "zustand";
import { Shell } from "@aws/amazon-q-developer-cli-api-bindings";
import { SpecLocationSource } from "@fig/autocomplete-shared";
import {
  SpecLocation,
  Suggestion,
} from "@aws/amazon-q-developer-cli-shared/internal";
import {
  makeArray,
  longestCommonPrefix,
  ensureTrailingSlash,
} from "@aws/amazon-q-developer-cli-shared/utils";
import { SETTINGS } from "@aws/amazon-q-developer-cli-api-bindings-wrappers";
import { trackEvent } from "../telemetry";
import { NamedSetState, AutocompleteState, Visibility } from "./types";
import {
  isMatchingType,
  getNameMatch,
  getQueryTermForSuggestion,
} from "../suggestions/helpers";
import { updateAutocompleteIndexFromUserInsert } from "../suggestions/sorting";
import { InsertPrefixError } from "./errors";

const sendTextToTerminal = (
  state: AutocompleteState,
  item: Suggestion,
  text: string,
  isFullCompletion: boolean,
  rootCommand: string,
) => {
  const {
    parserResult: { searchTerm },
    figState: { buffer },
  } = state;
  const queryTerm = getQueryTermForSuggestion(item, searchTerm);
  // See if we need to move the cursor
  let valToInsert = text;
  const cursorIdx = valToInsert.indexOf("{cursor}");
  let moveCursor = "";
  if (cursorIdx > -1) {
    valToInsert = valToInsert.replace(/{cursor}/, "");
    moveCursor = "\x1b[D".repeat(valToInsert.length - cursorIdx);
  }

  // Get the number of characters the user has already input so we can delete them
  let numCharsToDelete = 0;

  if (
    (item.type !== "auto-execute" && item.type !== "special") ||
    !isFullCompletion
  ) {
    // This assumes any search term that uses a space was done so by escaping
    // the space with a backslash.
    // A better solution is to map the search term to the actual user input string...
    numCharsToDelete = queryTerm.replace(/\s/, "\\ ").length;
  }

  if (item.type === "shortcut" && searchTerm.startsWith("?")) {
    numCharsToDelete += 1;
  }

  if (rootCommand === "?") {
    numCharsToDelete += 2;
  }

  // Don't delete characters if we don't have to
  if (
    valToInsert.startsWith(queryTerm) &&
    numCharsToDelete >= queryTerm.length
  ) {
    numCharsToDelete -= queryTerm.length;
    valToInsert = valToInsert.slice(queryTerm.length);
  }

  const finalStringToInsert =
    "\b".repeat(numCharsToDelete) + valToInsert + moveCursor;

  logger.info(`inserting: ${finalStringToInsert}`);

  Shell.insert(
    finalStringToInsert,
    { insertionBuffer: buffer },
    state.figState.shellContext?.sessionId,
  );
  return {
    insertedChars: valToInsert.length,
    insertedCharsFull: finalStringToInsert.length,
  };
};

const insertString = (
  state: AutocompleteState,
  item: Suggestion,
  text: string,
  isFullCompletion: boolean,
) => {
  const { command, updateVisibilityPostInsert, parserResult } = state;
  const { commandIndex, annotations } = parserResult;
  const rootCommand = command?.tokens[commandIndex]?.text || "";

  const inserted = sendTextToTerminal(
    state,
    item,
    text,
    isFullCompletion,
    rootCommand,
  );

  let specLocation: { location: SpecLocation; name: string } | undefined;
  annotations.forEach((annotation) => {
    if ("specLocation" in annotation) {
      const { specLocation: location, spec } = annotation;
      specLocation = {
        location,
        name: spec.name[0],
      };
    }
  });

  if (isFullCompletion) {
    const suggestionName = makeArray(item.name)[0] || "";
    updateAutocompleteIndexFromUserInsert(rootCommand, suggestionName);
  }

  const metadata: Record<string, string | boolean> | null = specLocation
    ? {
        specName: specLocation.name,
        specLocation: specLocation.location.name,
        specLocationType: specLocation.location.type,
        spec_is_script: specLocation.location.type === SpecLocationSource.LOCAL,
      }
    : null;

  trackEvent("autocomplete-insert", {
    ...metadata,
    rootCommand,
    suggestion_type: item.type ?? null,
    insertionLength: `${inserted.insertedChars}`,
    // Includes backspaces and cursor adjustments.
    insertionLengthFull: `${inserted.insertedCharsFull}`,
    app: fig.constants?.version || "",
    terminal: state.figState.shellContext?.terminal ?? null,
    shell: state.figState.shellContext?.shellPath?.split("/")?.at(-1) ?? null,
  });

  logger.info("Inserted string, updating visibility");
  updateVisibilityPostInsert(item, isFullCompletion);
};

const escapeInsertion = (str: string, isFolder: boolean) => {
  // Check if insertion character has any special chars
  // TODO: ultimately replace this with a regex
  const specialCharsNotSpace = "\\?*'\"#|<>()[]!&".split("");
  if (specialCharsNotSpace.every((char) => !str.includes(char))) {
    return !str.includes(" ") ? str : `${str.replace(/\s/g, "\\ ")}`;
  }

  if (isFolder) {
    return `'${str.slice(0, -1).replace(/'/g, "'\"'\"'")}'/`;
  }
  return `'${str.replace(/'/g, "'\"'\"'")}'`;
};

const getFullInsertion = (
  item: Suggestion,
  searchTerm: string,
  fuzzySearch: boolean,
  preferVerboseSuggestions: boolean,
): string => {
  const queryTerm = getQueryTermForSuggestion(item, searchTerm);

  const isFolder = item.type === "folder";
  const isFileOrFolder = item.type === "file" || isFolder;

  if (item.insertValue && !isFileOrFolder) {
    return item.insertValue;
  }

  const match = getNameMatch(
    item,
    queryTerm,
    fuzzySearch,
    preferVerboseSuggestions,
  );
  if (!match) return escapeInsertion("", isFolder);
  return escapeInsertion(
    item.separatorToAdd ? `${match}${item.separatorToAdd}{cursor}` : match,
    isFolder,
  );
};

const makeInsertTextForItem =
  (get: StoreApi<AutocompleteState>["getState"]) =>
  (item: Suggestion, execute = false) => {
    const state = get();
    const {
      parserResult: { searchTerm },
      fuzzySearchEnabled,
      settings,
    } = state;
    let text = getFullInsertion(
      item,
      searchTerm,
      fuzzySearchEnabled,
      settings[SETTINGS.PREFER_VERBOSE_SUGGESTIONS] as boolean,
    );

    if (execute && item.type !== "auto-execute") {
      text += "\n";
    }
    // Replace multiple newlines at the end of string with just one.
    text = text.replace(/\n+$/g, "\n");

    if (
      !text.endsWith("\n") &&
      item.shouldAddSpace &&
      (settings[SETTINGS.INSERT_SPACE_AUTOMATICALLY] ?? true)
    ) {
      text += " ";
    }

    return insertString(state, item, text, true);
  };

const makeInsertCommonPrefix =
  (get: StoreApi<AutocompleteState>["getState"]) => () => {
    const state = get();
    const {
      suggestions,
      selectedIndex,
      parserResult: { searchTerm },
      fuzzySearchEnabled,
      settings,
      insertTextForItem,
    } = state;
    const selectedItem = suggestions[selectedIndex];

    const queryTerm = getQueryTermForSuggestion(selectedItem, searchTerm);
    if (suggestions.length === 1) {
      return insertTextForItem(selectedItem);
    }

    let { type: itemType } = selectedItem;

    if (itemType === "auto-execute") {
      itemType = selectedItem.originalType;
    }

    if (itemType && ["special", "auto-execute"].includes(itemType)) {
      throw new InsertPrefixError(
        `Cannot insert common prefix for type ${itemType}`,
      );
    }

    const typeFilterItem = {
      ...selectedItem,
      type: itemType,
      name:
        selectedItem.type === "auto-execute" &&
        selectedItem.originalType === "folder"
          ? makeArray(selectedItem.name ?? []).map(ensureTrailingSlash)
          : selectedItem.name,
    };

    const sameTypeSuggestionNames = suggestions
      .filter((suggestion) => isMatchingType(suggestion, typeFilterItem))
      .map((suggestion) =>
        (makeArray(suggestion.name || [])[0] ?? "").toLowerCase(),
      )
      .filter((name) => name.startsWith(queryTerm.toLowerCase()));

    if (sameTypeSuggestionNames.length === 1) {
      if (selectedItem.type === "auto-execute") {
        throw new InsertPrefixError(`Cannot auto-execute using common prefix`);
      }
      return insertTextForItem(selectedItem);
    }

    const uncasedSharedPrefix = longestCommonPrefix(sameTypeSuggestionNames);
    const sharedPrefix = makeArray(selectedItem.name || [])[0].slice(
      0,
      uncasedSharedPrefix.length,
    );
    if (!sharedPrefix || sharedPrefix === queryTerm) {
      throw new InsertPrefixError("No remaining prefix to insert");
    }
    const text = sharedPrefix.replace(/\s/g, "\\ ");
    if (
      text ===
      getFullInsertion(
        selectedItem,
        searchTerm,
        fuzzySearchEnabled,
        settings[SETTINGS.PREFER_VERBOSE_SUGGESTIONS] as boolean,
      )
    ) {
      if (selectedItem.type === "auto-execute") {
        throw new InsertPrefixError(`Cannot auto-execute using common prefix`);
      }
      return insertTextForItem(selectedItem);
    }
    return insertString(state, selectedItem, text, false);
  };

export const createInsertionState = (
  setNamed: NamedSetState<AutocompleteState>,
  get: StoreApi<AutocompleteState>["getState"],
) => ({
  insertCommonPrefix: makeInsertCommonPrefix(get),
  insertTextForItem: makeInsertTextForItem(get),
  updateVisibilityPostInsert: (
    suggestion: Suggestion,
    isFullCompletion: boolean,
  ) => {
    if (isFullCompletion) {
      setNamed("updateVisibilityPostInsert", {
        visibleState: Visibility.HIDDEN_BY_INSERTION,
        lastInsertedSuggestion: suggestion,
        justInserted: true,
      });
    } else {
      setNamed("updateVisibilityPostInsert", { justInserted: true });
    }
  },
});
