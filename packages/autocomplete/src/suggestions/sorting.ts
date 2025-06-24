import { makeArray } from "@aws/amazon-q-developer-cli-shared/utils";
import { Suggestion } from "@aws/amazon-q-developer-cli-shared/internal";
import {
  SETTINGS,
  getSetting,
} from "@aws/amazon-q-developer-cli-api-bindings-wrappers";

// { command: { commandParam: count/lastUsedDate } }
// e.g. git: { add: 2, push: 4 }
type Index = Record<string, Record<string, number>>;

const frequencyIndex: Index = JSON.parse(
  localStorage.getItem("term_frequency_index_fig_only") || "{}",
);
const recencyIndex: Index = JSON.parse(
  localStorage.getItem("term_frequency_index_fig_only_recency") || "{}",
);

// This function is called every time a user does an autocomplete command
export const updateAutocompleteIndexFromUserInsert = (
  cmd: string,
  param: string,
) => {
  if (param.includes("↪")) return;

  const frequencyIndexCmd = frequencyIndex[cmd] || {};
  const recencyIndexCmd = recencyIndex[cmd] || {};

  frequencyIndex[cmd] = {
    ...frequencyIndexCmd,
    [param]: (frequencyIndexCmd[param] || 0) + 1,
  };

  recencyIndex[cmd] = {
    ...recencyIndexCmd,
    [param]: Date.now(),
  };

  // Update frequency index
  localStorage.setItem(
    "term_frequency_index_fig_only",
    JSON.stringify(frequencyIndex),
  );

  // Update recency index
  localStorage.setItem(
    "term_frequency_index_fig_only_recency",
    JSON.stringify(recencyIndex),
  );
};

export const updatePriorities = (suggestions: Suggestion[], cmd: string) => {
  let idxToUse;

  // Default setting for autocomplete.sortMethod is "recency".
  try {
    if (getSetting(SETTINGS.SORT_METHOD) !== "alphabetical") {
      idxToUse = recencyIndex;
    }
  } catch (_err) {
    idxToUse = recencyIndex;
  }

  const cmdRecency = idxToUse && idxToUse[cmd];

  return suggestions.map((suggestion) => {
    const name = makeArray(suggestion.name)[0] || "";
    const recency = name !== "../" && cmdRecency && cmdRecency[name];
    const recencyBoost = recency ? recency / 10000000000000 : 0;

    let priority = suggestion.priority || 50;
    if (suggestion.type !== "auto-execute") {
      priority = Math.max(Math.min(100, priority), 0);
    }

    if (recency && priority >= 50 && priority <= 75) {
      priority = 75 + recencyBoost;
    } else {
      priority += recencyBoost;
    }
    return { ...suggestion, priority };
  });
};
