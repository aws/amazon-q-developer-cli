import fuzzysort from "@aws/amazon-q-developer-cli-fuzzysort";
import logger from "loglevel";
import { Suggestion } from "@aws/amazon-q-developer-cli-shared/internal";
import {
  makeArray,
  longestCommonPrefix,
  ensureTrailingSlash,
} from "@aws/amazon-q-developer-cli-shared/utils";

const SHORTCUT_PREFIX = "?";

const isFileOrFolder = (suggestion: Suggestion) =>
  ["file", "folder"].includes(suggestion.type || "");

export const isMatchingType = (a: Suggestion, b: Suggestion) => {
  if (makeArray(a.name ?? [])[0] === "../") {
    return makeArray(b.name ?? [])[0] === "../";
  }
  return (isFileOrFolder(a) && isFileOrFolder(b)) || a.type === b.type;
};

const getMostVerboseName = (names: string[]): string | undefined => {
  let longest = "";
  for (const name of names) {
    if (name.length > longest.length) {
      longest = name;
    }
  }
  return longest || undefined;
};

export const getFuzzyNameMatches = (names: string[], query: string) => {
  if (!query) {
    return names.map((name) => ({ target: name, score: 0, indexes: [] }));
  }
  return names.map((name) => fuzzysort.single(query, name));
};

// IMPORTANT!: Always return scores: -10 < score <= 0
export const getPrefixNameMatches = (
  names: string[],
  query: string,
): ({ score: number; target: string } | null)[] => {
  const queryLower = query.toLowerCase();
  return names.map((name) => {
    const nameLower = name.toLowerCase();
    if (name === query) return { score: 0, target: name };
    if (nameLower === queryLower) return { score: -1, target: name };
    if (name.startsWith(query)) return { score: -2, target: name };
    if (nameLower.startsWith(queryLower)) return { score: -3, target: name };
    return null;
  });
};

export function bestScoreMatch<T extends { score: number } | null>(a: T[]): T {
  return a.reduce((best, curr) => {
    if (!best) return curr;
    return !curr || curr.score < best.score ? best : curr;
  }, a[0]);
}

export const getNameMatch = (
  suggestion: Suggestion,
  query: string,
  fuzzy: boolean,
  preferVerboseSuggestions: boolean,
): string | undefined => {
  // try to match name property
  const namesArray = makeArray(suggestion.name || []);
  const matchObjs = fuzzy
    ? getFuzzyNameMatches(namesArray, query)
    : getPrefixNameMatches(namesArray, query);

  const matches = matchObjs
    .map((x) => x?.target)
    .filter((x): x is string => x !== null && x !== undefined);

  const bestNameMatch = (names: string[]) =>
    (suggestion.type === "option" || suggestion.type === "subcommand") &&
    preferVerboseSuggestions
      ? getMostVerboseName(names)
      : names[0];

  if (matches.length || !suggestion.displayName) {
    return bestNameMatch(matches);
  }
  // if no name matches were found it means that the suggestion the user entered was being shown
  // because the displayValue is still matching in some of his parts
  // so we return a random name
  return bestNameMatch(namesArray);
};

// Returns first exact matching string (case sensitive or not) in name array.
export const getExactNameMatch = (
  suggestion: Suggestion,
  query: string,
): string | undefined => {
  const names = makeArray(suggestion.name || []) as string[];
  return names.find((name) => name.toLowerCase() === query.toLowerCase());
};

export const getQueryTermForSuggestion = (
  suggestion: Suggestion,
  searchTerm: string,
): string => {
  const getQueryTerm =
    suggestion?.getQueryTerm || suggestion?.generator?.getQueryTerm;
  if (getQueryTerm) {
    if (typeof getQueryTerm === "string") {
      // If getQueryTerm is a string, get everything after the last occurrence of the string
      return searchTerm.slice(searchTerm.lastIndexOf(getQueryTerm) + 1);
    }
    try {
      return getQueryTerm(searchTerm);
    } catch (e) {
      logger.error("there was an error with getQueryTerm", e);
    }
  }

  if (
    suggestion?.type === "shortcut" &&
    searchTerm.startsWith(SHORTCUT_PREFIX)
  ) {
    return searchTerm.slice(SHORTCUT_PREFIX.length);
  }

  return searchTerm;
};

export const getCommonSuggestionPrefix = (
  selectedIndex: number,
  suggestions: Suggestion[],
): string | null => {
  const selectedItem = suggestions[selectedIndex];
  if (!selectedItem) {
    return null;
  }
  let { type: itemType } = selectedItem;

  if (itemType === "auto-execute") {
    itemType = selectedItem.originalType;
  }

  if (itemType && ["special", "auto-execute"].includes(itemType)) {
    return null;
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
    );

  if (sameTypeSuggestionNames.length < 2) {
    return null;
  }
  return longestCommonPrefix(sameTypeSuggestionNames);
};
