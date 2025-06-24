import { SettingsMap } from "@aws/amazon-q-developer-cli-api-bindings-wrappers";
import * as app from "./fig.json";
import { create } from "@bufbuild/protobuf";
import {
  ActionAvailability,
  ActionSchema,
} from "@aws/amazon-q-developer-cli-proto/fig";

const SELECT_SUGGESTION_ACTION_PREFIX = "selectSuggestion";

export enum AutocompleteAction {
  INSERT_SELECTED = "insertSelected",
  INSERT_COMMON_PREFIX = "insertCommonPrefix",
  INSERT_COMMON_PREFIX_OR_NAVIGATE_DOWN = "insertCommonPrefixOrNavigateDown",
  INSERT_COMMON_PREFIX_OR_INSERT_SELECTED = "insertCommonPrefixOrInsertSelected",
  INSERT_SELECTED_AND_EXECUTE = "insertSelectedAndExecute",
  EXECUTE = "execute",

  HIDE_AUTOCOMPLETE = "hideAutocomplete",
  SHOW_AUTOCOMPLETE = "showAutocomplete",
  TOGGLE_AUTOCOMPLETE = "toggleAutocomplete",

  NAVIGATE_UP = "navigateUp",
  NAVIGATE_DOWN = "navigateDown",

  HIDE_DESCRIPTION = "hideDescription",
  SHOW_DESCRIPTION = "showDescription",
  TOGGLE_DESCRIPTION = "toggleDescription",

  TOGGLE_HISTORY_MODE = "toggleHistoryMode",

  TOGGLE_FUZZY_SEARCH = "toggleFuzzySearch",

  INCREASE_SIZE = "increaseSize",
  DECREASE_SIZE = "decreaseSize",

  SELECT_SUGGESTION_1 = "selectSuggestion1",
  SELECT_SUGGESTION_2 = "selectSuggestion2",
  SELECT_SUGGESTION_3 = "selectSuggestion3",
  SELECT_SUGGESTION_4 = "selectSuggestion4",
  SELECT_SUGGESTION_5 = "selectSuggestion5",
  SELECT_SUGGESTION_6 = "selectSuggestion6",
  SELECT_SUGGESTION_7 = "selectSuggestion7",
  SELECT_SUGGESTION_8 = "selectSuggestion8",
  SELECT_SUGGESTION_9 = "selectSuggestion9",
  SELECT_SUGGESTION_10 = "selectSuggestion10",
}

export const ACTIONS = app.contributes.actions.map((action) => {
  let availability: ActionAvailability | undefined;
  switch (action.availability) {
    case "ALWAYS":
      availability = ActionAvailability.ALWAYS;
      break;
    case "WHEN_FOCUSED":
      availability = ActionAvailability.WHEN_FOCUSED;
      break;
    case "WHEN_HIDDEN":
      availability = ActionAvailability.WHEN_HIDDEN;
      break;
    case "WHEN_VISIBLE":
      availability = ActionAvailability.WHEN_VISIBLE;
      break;
  }

  return create(ActionSchema, {
    ...action,
    availability,
  });
});

const defaultSelectSuggestionKeybindings = app.contributes.actions.reduce(
  (p, v) => {
    if (v.identifier.startsWith("selectSuggestion")) {
      return Object.assign(p, { [v.identifier]: v.defaultBindings });
    }
    return p;
  },
  {},
);
let selectSuggestionKeybindings: Record<string, string[]> = {};

export const updateSelectSuggestionKeybindings = (settings: SettingsMap) => {
  selectSuggestionKeybindings = {};
  for (const [key, value] of Object.entries(settings)) {
    if (
      typeof value === "string" &&
      value.startsWith(SELECT_SUGGESTION_ACTION_PREFIX)
    ) {
      const keybinding = key.replace("autocomplete.keybindings.", "");
      if (!selectSuggestionKeybindings[value]) {
        selectSuggestionKeybindings[value] = [keybinding];
      } else {
        selectSuggestionKeybindings[value].push(keybinding);
      }
    }
  }

  selectSuggestionKeybindings = {
    ...defaultSelectSuggestionKeybindings,
    ...selectSuggestionKeybindings,
  };
};

export const getSelectSuggestionKeybinding = (
  suggestionIndex: number /* This is an index so it starts from 0 */,
) => selectSuggestionKeybindings[`selectSuggestion${suggestionIndex + 1}`];

export const getActionKeybindings = () => {};
