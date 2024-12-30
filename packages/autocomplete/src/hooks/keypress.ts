import { useCallback } from "react";
import logger from "loglevel";
import { Keybindings } from "@aws/amazon-q-developer-cli-api-bindings";
import {
  SETTINGS,
  getSetting,
} from "@aws/amazon-q-developer-cli-api-bindings-wrappers";
import { ACTIONS, AutocompleteAction } from "../actions";
import { useAutocompleteStore } from "../state";
import { Visibility } from "../state/types";
import { IpcBackend } from "@aws/amazon-q-developer-cli-ipc-backend-core";
import { create } from "@bufbuild/protobuf";
import {
  InsertTextRequestSchema,
  InterceptRequest_SetFigjsInterceptsSchema,
  InterceptRequestSchema,
} from "@aws/amazon-q-developer-cli-proto/figterm";

export const setInterceptKeystrokes = (
  intercept: boolean,
  globalIntercept?: boolean,
  currentTerminalSessionId?: string,
) =>
  Keybindings.setInterceptKeystrokes(
    ACTIONS,
    intercept,
    globalIntercept,
    currentTerminalSessionId,
  );

export const setInterceptKeystrokesBackend = (
  ipcBackend: IpcBackend,
  interceptBoundKeystrokes: boolean,
  interceptGlobalKeystrokes: boolean = false,
  currentTerminalSessionId?: string,
) =>
  ipcBackend?.intercept(
    currentTerminalSessionId ?? "",
    create(InterceptRequestSchema, {
      interceptCommand: {
        case: "setFigjsIntercepts",
        value: create(InterceptRequest_SetFigjsInterceptsSchema, {
          // actions: ACTIONS,
          actions: [],
          interceptBoundKeystrokes,
          interceptGlobalKeystrokes,
          overrideActions: false,
        }),
      },
    }),
  );
// Keybindings.setInterceptKeystrokes(
//   ACTIONS,
//   intercept,
//   globalIntercept,
//   currentTerminalSessionId,
// );

enum KeyCode {
  TAB = 48,
}

export enum Direction {
  INCREASE = "increase",
  DECREASE = "decrease",
}

export const useAutocompleteKeypressCallback = (
  toggleDescriptionPopout: () => void,
  shake: () => void,
  changeSize: (direction: Direction) => void,
  ipcBackend: IpcBackend,
): Parameters<typeof Keybindings.pressed>[0] => {
  const {
    suggestions,
    visibleState,
    selectedIndex,

    setHistoryModeEnabled,
    setUserFuzzySearchEnabled,
    settings,

    insertTextForItem,
    insertCommonPrefix,
    scroll: scrollToIndex,
    setVisibleState,

    figState,
    setFigState,
  } = useAutocompleteStore();

  const selectedItem = suggestions[selectedIndex];
  const scrollWrapAround = settings[SETTINGS.SCROLL_WRAP_AROUND];

  const navigate = useCallback(
    (delta: number, hideOrShowHistoryOnNegative = false) => {
      let newIndex = selectedIndex + delta;
      let newVisibleState = visibleState;
      if (scrollWrapAround) {
        newIndex = (newIndex + suggestions.length) % suggestions.length;
      } else if (newIndex === -1 && hideOrShowHistoryOnNegative) {
        if (getSetting<boolean>(SETTINGS.NAVIGATE_TO_HISTORY)) {
          setHistoryModeEnabled((enabled) => !enabled);
          return;
        }
        newVisibleState = Visibility.HIDDEN_UNTIL_KEYPRESS;
      }
      scrollToIndex(newIndex, newVisibleState);
    },
    [
      selectedIndex,
      visibleState,
      suggestions.length,
      scrollWrapAround,
      setHistoryModeEnabled,
    ],
  );

  return useCallback(
    (notification) => {
      const { action, keypress, context: shellContext } = notification;
      logger.debug("Handling keypress", { notification, keypress, action });

      setFigState({ ...figState, shellContext });

      if (
        getSetting<boolean>(SETTINGS.ONLY_SHOW_ON_TAB) &&
        keypress?.appleKeyCode === KeyCode.TAB &&
        visibleState !== Visibility.VISIBLE
      ) {
        if (suggestions.length === 1) {
          insertTextForItem(ipcBackend, suggestions[0]);
          return undefined;
        }
        setVisibleState(Visibility.VISIBLE);
        return undefined;
      }
      if (!selectedItem) {
        return undefined;
      }
      switch (action) {
        case AutocompleteAction.NAVIGATE_UP:
          navigate(-1, true);
          break;
        case AutocompleteAction.NAVIGATE_DOWN:
          navigate(1);
          break;
        case AutocompleteAction.INSERT_SELECTED:
          insertTextForItem(ipcBackend, selectedItem);
          break;
        case AutocompleteAction.INSERT_COMMON_PREFIX:
          try {
            insertCommonPrefix(ipcBackend);
          } catch (err) {
            shake();
          }
          break;
        case AutocompleteAction.INSERT_COMMON_PREFIX_OR_NAVIGATE_DOWN:
          try {
            insertCommonPrefix(ipcBackend);
          } catch (err) {
            navigate(1);
          }
          break;
        case AutocompleteAction.INSERT_COMMON_PREFIX_OR_INSERT_SELECTED:
          try {
            insertCommonPrefix(ipcBackend);
          } catch (err) {
            insertTextForItem(ipcBackend, selectedItem);
          }
          break;
        case AutocompleteAction.INSERT_SELECTED_AND_EXECUTE:
          insertTextForItem(ipcBackend, selectedItem, true);
          break;
        case AutocompleteAction.EXECUTE:
          ipcBackend?.insertText(
            figState.shellContext?.sessionId ?? "",
            create(InsertTextRequestSchema, {
              insertion: "\n",
            }),
          );
          break;
        case AutocompleteAction.HIDE_AUTOCOMPLETE:
          setVisibleState(Visibility.HIDDEN_UNTIL_SHOWN);
          break;
        case AutocompleteAction.SHOW_AUTOCOMPLETE:
          setVisibleState(Visibility.VISIBLE);
          break;
        case AutocompleteAction.TOGGLE_AUTOCOMPLETE:
          setVisibleState(
            visibleState === Visibility.VISIBLE
              ? Visibility.HIDDEN_UNTIL_SHOWN
              : Visibility.VISIBLE,
          );
          break;
        case AutocompleteAction.TOGGLE_HISTORY_MODE:
          setHistoryModeEnabled((enabled) => !enabled);
          break;
        case AutocompleteAction.TOGGLE_DESCRIPTION:
          if (!getSetting<boolean>(SETTINGS.ALWAYS_SHOW_DESCRIPTION)) {
            toggleDescriptionPopout();
          }
          break;
        case AutocompleteAction.TOGGLE_FUZZY_SEARCH:
          setUserFuzzySearchEnabled((enabled) => !enabled);
          break;
        case AutocompleteAction.INCREASE_SIZE:
          changeSize(Direction.INCREASE);
          break;
        case AutocompleteAction.DECREASE_SIZE:
          changeSize(Direction.DECREASE);
          break;
        case AutocompleteAction.SELECT_SUGGESTION_1:
        case AutocompleteAction.SELECT_SUGGESTION_2:
        case AutocompleteAction.SELECT_SUGGESTION_3:
        case AutocompleteAction.SELECT_SUGGESTION_4:
        case AutocompleteAction.SELECT_SUGGESTION_5:
        case AutocompleteAction.SELECT_SUGGESTION_6:
        case AutocompleteAction.SELECT_SUGGESTION_7:
        case AutocompleteAction.SELECT_SUGGESTION_8:
        case AutocompleteAction.SELECT_SUGGESTION_9:
        case AutocompleteAction.SELECT_SUGGESTION_10: {
          const suggestionIndex = parseInt(action, 10) - 1;
          if (
            suggestionIndex <= suggestions.length - 1 &&
            suggestionIndex <= 0 &&
            suggestionIndex <= 9
          ) {
            scrollToIndex(suggestionIndex, Visibility.VISIBLE);
          }
          break;
        }
        default:
          logger.error("Unrecognized action");
          break;
      }
      return undefined;
    },
    [
      ipcBackend,
      navigate,
      insertCommonPrefix,
      insertTextForItem,
      selectedItem,
      toggleDescriptionPopout,
      setHistoryModeEnabled,
      visibleState,
      setVisibleState,
      changeSize,
      figState,
      setFigState,
    ],
  );
};
