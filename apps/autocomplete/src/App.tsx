import { WindowPosition, Settings, Event } from "@withfig/api-bindings";
import { SETTINGS } from "@amzn/fig-io-api-bindings-wrappers";
import React, {
  useState,
  useEffect,
  useCallback,
  useRef,
  useMemo,
  MutableRefObject,
} from "react";

import logger from "loglevel";
import { getCWDForFilesAndFolders } from "@internal/shared/utils";
import {
  loadPrivateSpecs,
  clearFigCaches,
  reloadClis,
  resetPreloadPromise,
  canLoadFigspec,
} from "@amzn/fig-io-autocomplete-parser";
import { useRefreshTokenExpirationStatus } from "@amzn/fig-io-api-client";
import * as jsxRuntime from "react/jsx-runtime";
import * as figApiBindings from "@withfig/api-bindings";
import { getMaxHeight, getMaxWidth, POPOUT_WIDTH } from "./window";
import { useParseArgumentsEffect } from "./parser/hooks";
import { loadHistory } from "./history";
import "./index.css";
import AutoSizedList, { AutoSizedHandleRef } from "./components/AutoSizedList";

import { captureError } from "./sentry";
import {
  useFigKeypress,
  useFigAutocomplete,
  useLoadAliasEffect,
  useFigSubscriptionEffect,
  useFigSettings,
  useLoadClisEffect,
} from "./fig/hooks";

import { getCommonSuggestionPrefix } from "./suggestions/helpers";

import { useAutocompleteStore } from "./state";
import { Visibility } from "./state/types";
import {
  useAutocompleteKeypressCallback,
  setInterceptKeystrokes,
  Direction,
} from "./hooks/keypress";
import {
  useShake,
  useDynamicResizeObserver,
  useSystemTheme,
} from "./hooks/helpers";

import Suggestion from "./components/Suggestion";
import Description, { DescriptionPosition } from "./components/Description";
import Preview from "./components/Preview";
import {
  setFontFamily,
  setFontSize,
  setTheme,
  setCustomCSS,
} from "./fig/themes";
import { Notifications } from "./components/notifications/Notifications";
import { authClient } from "./auth";
import LoadingIcon from "./components/LoadingIcon";

// eslint-disable-next-line @typescript-eslint/ban-ts-comment
// @ts-ignore
window.jsxRuntime = jsxRuntime;
window.React = React;
// eslint-disable-next-line @typescript-eslint/ban-ts-comment
// @ts-ignore
window.figApiBindings = figApiBindings;

const getIconPath = (cwd: string): string => {
  const { home } = fig.constants ?? {};
  const path = home && cwd.startsWith("~") ? home + cwd.slice(1) : cwd;
  return path.startsWith("//") ? path.slice(1) : path;
};

function App() {
  const {
    generatorStates,
    suggestions,
    selectedIndex,
    visibleState,
    figState: { buffer, cwd, shellContext, processUserIsIn },
    parserResult: { searchTerm, currentArg },
    settings,
    setSettings,
    insertTextForItem,

    fuzzySearchEnabled,
    setUserFuzzySearchEnabled,
    setFigState,
  } = useAutocompleteStore();

  const [systemTheme] = useSystemTheme();
  const [isShaking, shake] = useShake();
  const [isLoadingSuggestions, setIsLoadingSuggestions] = useState(false);
  const [windowState, setWindowState] = useState({
    isDescriptionSeparate: false,
    isAboveCursor: true,
    descriptionPosition: DescriptionPosition.UNKNOWN,
    previewPosition: DescriptionPosition.RIGHT,
  });
  const {
    [SETTINGS.THEME]: theme,
    [SETTINGS.USER_STYLES]: userStyles,
    [SETTINGS.FUZZY_SEARCH]: userFuzzySearch,
    [SETTINGS.FONT_FAMILY]: fontFamily,
    [SETTINGS.FONT_SIZE]: settingsFontSize,
    [SETTINGS.WIDTH]: settingsWidth,
    [SETTINGS.HEIGHT]: settingsHeight,
    [SETTINGS.ALWAYS_SHOW_DESCRIPTION]: alwaysShowDescription,
    [SETTINGS.DEV_MODE_NPM]: devMode,
    [SETTINGS.HIDE_PREVIEW]: hidePreview,
  } = settings;

  const [size, setSize] = useState({
    fontSize: settingsFontSize as number | undefined,
    maxHeight: getMaxHeight(),
    suggestionWidth: getMaxWidth(),
    // 20 is the default height of a suggestion row. Font size is undefined unless set by user
    // in settings. If not set, row height should default to 20px.
    itemSize: 20,
  });

  useEffect(() => {
    setSize((oldSize) => ({
      ...oldSize,
      maxHeight: getMaxHeight(),
      suggestionWidth: getMaxWidth(),
    }));
  }, [settingsHeight, settingsWidth]);

  useEffect(() => {
    loadPrivateSpecs(authClient).catch(captureError);
  }, []);

  useFigSubscriptionEffect(
    () =>
      Event.subscribe("autocomplete.privateSpecsUpdated", () => {
        loadPrivateSpecs(authClient).catch(captureError);
        return { unsubscribe: false };
      }),
    [],
  );

  useFigSubscriptionEffect(
    () =>
      Event.subscribe("autocomplete.mixinsUpdated", () => {
        window.resetCaches?.();
        resetPreloadPromise();
        return { unsubscribe: false };
      }),
    [],
  );

  useFigSubscriptionEffect(
    () =>
      Event.subscribe("autocomplete.commandlineToolUpdated", () => {
        reloadClis(authClient).then((clis) => {
          if (clis) {
            const map = Object.fromEntries(
              clis.map((cli) => [
                cli.name,
                `fig cli @${cli.namespace}/${cli.name}`,
              ]),
            );
            setFigState((state) => ({ ...state, cliAliases: map }));
          }
        });
        return { unsubscribe: false };
      }),
    [],
  );

  useFigSubscriptionEffect(
    () => Event.subscribe("scripts.update", clearFigCaches),
    [],
  );

  useFigSubscriptionEffect(
    () => Event.subscribe("scripts.rename", clearFigCaches),
    [],
  );

  useFigSubscriptionEffect(
    () => Event.subscribe("scripts.delete", clearFigCaches),
    [],
  );

  const expirationStatus = useRefreshTokenExpirationStatus(buffer, authClient);
  const isLoggedIn =
    !expirationStatus.loading && expirationStatus.expired === false;
  const isLoading = isLoadingSuggestions || expirationStatus.loading;

  useEffect(() => {
    // Default font-size is 12.8px (0.8em) and default row size is 20px = 12.8 * 1.5625
    // Row height should scale accordingly with font-size
    console.log("settingsFontSize", settingsFontSize);

    const fontSize =
      typeof settingsFontSize === "number" && settingsFontSize > 0
        ? settingsFontSize
        : 12.8;

    setSize((oldSize) => ({
      ...oldSize,
      fontSize,
      itemSize: fontSize * 1.5625,
    }));
  }, [settingsFontSize]);

  // Info passed down to suggestions to render icons and underline.
  const iconPath = useMemo(
    () => getIconPath(getCWDForFilesAndFolders(cwd, searchTerm)),
    [cwd, searchTerm],
  );

  const commonPrefix = useMemo(
    () => getCommonSuggestionPrefix(selectedIndex, suggestions),
    [selectedIndex, suggestions],
  );

  useEffect(() => {
    setWindowState((state) => ({
      isAboveCursor: alwaysShowDescription ? state.isAboveCursor : false,
      isDescriptionSeparate: alwaysShowDescription as boolean,
      descriptionPosition: DescriptionPosition.UNKNOWN,
      previewPosition: state.previewPosition,
    }));
  }, [alwaysShowDescription]);

  // Effect hooks into fig autocomplete events, parser, async generator results, and keypresses.
  const toggleDescriptionPopout = () => {
    setWindowState((state) => ({
      // if we are bringing the description back to the suggestion list, set isAboveCursor to false.
      isAboveCursor: state.isDescriptionSeparate ? false : state.isAboveCursor,
      isDescriptionSeparate: !state.isDescriptionSeparate,
      descriptionPosition: DescriptionPosition.UNKNOWN,
      previewPosition: state.previewPosition,
    }));
  };

  const changeSize = useCallback((direction: Direction): void => {
    // --font-size is read from the stylesheet as " 12.8px". We just want the number
    const currentFontSize = window
      .getComputedStyle(document.documentElement)
      .getPropertyValue("--font-size")
      .slice(0, -2)
      .trim();
    // Increase or decrease current size by 10%
    const change = (val: number) =>
      direction === Direction.INCREASE ? val * 1.1 : val / 1.1;

    setSize((oldSize) => ({
      fontSize: change(oldSize.fontSize ?? Number(currentFontSize)),
      itemSize: change(oldSize.itemSize),
      maxHeight: change(oldSize.maxHeight),
      suggestionWidth: change(oldSize.suggestionWidth),
    }));
  }, []);

  const keypressCallback = useAutocompleteKeypressCallback(
    toggleDescriptionPopout,
    shake,
    changeSize,
  );

  useFigAutocomplete(setFigState);
  useLoadAliasEffect(setFigState, processUserIsIn ?? shellContext?.processName);
  useLoadClisEffect(setFigState);
  useParseArgumentsEffect(setIsLoadingSuggestions);
  useFigSettings(setSettings);
  useFigKeypress(keypressCallback);

  useEffect(() => {
    // Dont preload if we can load the locally cached figspec
    if (canLoadFigspec()) {
      return;
    }

    Settings.get(SETTINGS.DISABLE_HISTORY_LOADING)
      .catch(() => undefined)
      .then((res) => {
        if (!JSON.parse(res?.jsonBlob ?? "false")) {
          loadHistory({});
        }
      });
  }, []);

  useEffect(() => {
    let isMostRecentEffect = true;
    if (generatorStates.some(({ loading }) => loading)) {
      setTimeout(() => {
        if (isMostRecentEffect) {
          setIsLoadingSuggestions(true);
        }
      }, 200);
    } else {
      setIsLoadingSuggestions(false);
    }
    return () => {
      isMostRecentEffect = false;
    };
  }, [generatorStates]);

  // Make sure fig dimensions align with our desired dimensions.
  const isHidden = visibleState !== Visibility.VISIBLE;
  const interceptKeystrokes =
    isLoggedIn && Boolean(!isHidden && suggestions.length > 0);

  useEffect(() => {
    logger.info("Setting intercept keystrokes", {
      suggestions,
      interceptKeystrokes,
    });
    setInterceptKeystrokes(
      interceptKeystrokes,
      suggestions.length > 0,
      shellContext?.sessionId,
    );
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [interceptKeystrokes, suggestions.length]);
  useEffect(() => {
    setTheme(systemTheme, theme as string);
  }, [theme, systemTheme]);
  useEffect(() => {
    setCustomCSS(userStyles as string);
  }, [userStyles]);
  useEffect(() => {
    setFontSize(size.fontSize);
  }, [size.fontSize]);
  useEffect(() => {
    setUserFuzzySearchEnabled((userFuzzySearch ?? false) as boolean);
  }, [setUserFuzzySearchEnabled, userFuzzySearch]);
  useEffect(() => {
    setFontFamily(fontFamily as string);
  }, [fontFamily]);
  // Scroll when selectedIndex changes.
  const listRef =
    useRef<AutoSizedHandleRef>() as MutableRefObject<AutoSizedHandleRef>;

  const scrollToItemCallback = useCallback(() => {
    logger.info("Scrolling to", { selectedIndex });
    listRef.current?.scrollToItem(selectedIndex);
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [selectedIndex, windowState.descriptionPosition]);

  useEffect(() => {
    let isMostRecentEffect = true;

    if (
      windowState.isDescriptionSeparate ||
      Boolean(suggestions[selectedIndex]?.previewComponent)
    ) {
      WindowPosition.isValidFrame({
        height: size.maxHeight,
        width: size.suggestionWidth + POPOUT_WIDTH,
        anchorX: 0,
      })
        .then(({ isAbove, isClipped }) => {
          if (isMostRecentEffect) {
            setWindowState((state) =>
              state.isDescriptionSeparate ||
              Boolean(suggestions[selectedIndex]?.previewComponent)
                ? {
                    ...state,
                    descriptionPosition: isClipped
                      ? DescriptionPosition.LEFT
                      : DescriptionPosition.RIGHT,
                    isAboveCursor: isAbove ?? false,
                    previewPosition: isClipped
                      ? DescriptionPosition.LEFT
                      : DescriptionPosition.RIGHT,
                  }
                : state,
            );
          }
        })
        .catch((err) => {
          logger.error("Error checking window position", { err });
        });
    }

    return () => {
      isMostRecentEffect = false;
    };
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [
    size.suggestionWidth,
    size.maxHeight,
    buffer,
    windowState.isDescriptionSeparate,
    // eslint-disable-next-line react-hooks/exhaustive-deps
    suggestions[selectedIndex]?.previewComponent,
  ]);

  const hasSpecialArgDescription =
    suggestions.length === 0 &&
    Boolean(currentArg?.name || currentArg?.description);

  const onResize: (size: { height?: number; width?: number }) => void =
    useCallback(
      ({ height, width }) => {
        const onLeft =
          !hasSpecialArgDescription &&
          windowState.descriptionPosition === DescriptionPosition.LEFT;

        const frame = {
          height: height || 1,
          width: width || 1,
          anchorX: onLeft ? -POPOUT_WIDTH : 0,
          offsetFromBaseline: -3,
        };
        logger.debug("Setting window position", { frame });
        WindowPosition.setFrame(frame)
          .then(({ isClipped, isAbove }) => {
            setWindowState((state) =>
              state.isDescriptionSeparate ||
              devMode ||
              Boolean(suggestions[selectedIndex]?.previewComponent)
                ? {
                    ...state,
                    descriptionPosition: isClipped
                      ? DescriptionPosition.LEFT
                      : DescriptionPosition.RIGHT,
                    isAboveCursor: isAbove ?? false,
                    previewPosition: isClipped
                      ? DescriptionPosition.LEFT
                      : DescriptionPosition.RIGHT,
                  }
                : state,
            );
          })
          .catch((err) => {
            logger.error("Error setting window position", { err });
          });
      },
      // eslint-disable-next-line react-hooks/exhaustive-deps
      [
        windowState.descriptionPosition,
        hasSpecialArgDescription,
        isHidden,
        // eslint-disable-next-line react-hooks/exhaustive-deps
        suggestions[selectedIndex]?.previewComponent,
      ],
    );

  useEffect(() => {
    onResize({});
  }, [onResize]);

  const { ref: autocompleteWindowRef } = useDynamicResizeObserver({ onResize });

  if (isHidden) {
    return <div ref={autocompleteWindowRef} id="autocompleteWindow" />;
  }

  const hasSuggestions = suggestions.length > 0;

  const descriptionPosition =
    hasSuggestions && windowState.isDescriptionSeparate
      ? windowState.descriptionPosition
      : DescriptionPosition.UNKNOWN;

  const previewPosition =
    hasSuggestions && suggestions[selectedIndex]?.previewComponent
      ? windowState.previewPosition
      : DescriptionPosition.UNKNOWN;

  const description = (
    <Description
      currentArg={currentArg}
      hasSuggestions={hasSuggestions}
      selectedItem={suggestions[selectedIndex]}
      shouldShowHint={!isLoading && !alwaysShowDescription}
      position={descriptionPosition}
      height={size.itemSize}
    />
  );

  const selectedItem = suggestions[selectedIndex];
  const preview = Boolean(selectedItem && !hidePreview) && (
    <Preview selectedItem={selectedItem} position={previewPosition} />
  );

  const hasBottomDescription =
    descriptionPosition === DescriptionPosition.UNKNOWN && description !== null;
  const listClasses = [
    "rounded",
    isShaking && "shake",
    hasBottomDescription && "rounded-b-none",
  ];

  const devModeWarning = devMode && (
    <div
      style={{
        width: size.suggestionWidth - 20,
      }}
      className="m-1 space-y-1.5 rounded bg-amber-500 px-2.5 py-2 text-black"
    >
      <div className="text-base font-bold">Developer mode enabled!</div>
      <div className="text-sm">
        Loading specs from disk. Disable with either
      </div>
      <div className="ml-2 flex flex-col gap-1 text-xs">
        <div>
          •{" "}
          <code className="rounded-sm bg-neutral-700 p-0.5 text-neutral-200">
            Ctrl + C
          </code>{" "}
          in the dev mode process
        </div>
        <div>
          {"• "}
          <button
            type="button"
            className="text-xs underline"
            onClick={() => {
              Settings.set("autocomplete.developerModeNPM", false);
            }}
          >
            Click to disable
          </button>
        </div>
      </div>
    </div>
  );

  let contents: React.ReactElement;

  if (isLoggedIn) {
    contents = (
      <>
        {windowState.isAboveCursor && devModeWarning}
        {windowState.isAboveCursor && <Notifications />}
        <div className="flex flex-row gap-1.5 p-1">
          {descriptionPosition === DescriptionPosition.LEFT && description}
          {previewPosition === DescriptionPosition.LEFT && preview}
          <div
            className="bg-main-bg relative flex flex-none flex-col items-stretch overflow-hidden rounded shadow-[0px_0px_3px_0px_rgb(85,_85,_85)]"
            style={
              hasSuggestions
                ? {
                    width: size.suggestionWidth,
                    height: "100%",
                    alignSelf: windowState.isAboveCursor
                      ? "flex-end"
                      : "flex-start",
                    maxHeight: size.maxHeight,
                  }
                : {}
            }
          >
            <AutoSizedList
              className={listClasses.filter((x) => !!x).join(" ")}
              onResize={scrollToItemCallback}
              ref={listRef}
              itemSize={size.itemSize}
              width="100%"
              itemCount={Math.round(suggestions.length)}
            >
              {({ index, style }) => (
                <Suggestion
                  style={style}
                  suggestion={suggestions[index]}
                  commonPrefix={commonPrefix || ""}
                  onClick={insertTextForItem}
                  isActive={selectedIndex === index}
                  searchTerm={searchTerm}
                  fuzzySearchEnabled={fuzzySearchEnabled}
                  iconPath={iconPath}
                  iconSize={size.itemSize * 0.75}
                />
              )}
            </AutoSizedList>
            <div className="scrollbar-none flex flex-shrink-0 flex-row">
              {descriptionPosition === DescriptionPosition.UNKNOWN &&
                description}
              {isLoading && (
                <LoadingIcon
                  className={
                    hasSuggestions
                      ? "left-[2px] [&>*]:top-[calc(50%-5px)]"
                      : "left-[2px] [&>*]:top-[calc(50%-3px)]"
                  }
                />
              )}
            </div>
          </div>
          {previewPosition === DescriptionPosition.RIGHT && preview}
          {descriptionPosition === DescriptionPosition.RIGHT && description}
        </div>
        {!windowState.isAboveCursor && <Notifications />}
        {!windowState.isAboveCursor && devModeWarning}
      </>
    );
  } else if (expirationStatus.loading) {
    contents = <LoadingIcon />;
  } else {
    contents = (
      <div className="small-text m-1 w-auto space-y-1.5 whitespace-nowrap rounded bg-amber-500 px-2.5 py-2 text-black">
        <div className="font-bold">Not logged in</div>
        <div className="text-sm">
          Please run{" "}
          <code className="rounded-sm bg-neutral-700 p-0.5 text-neutral-200">
            fig login
          </code>{" "}
          to login/signup
        </div>
      </div>
    );
  }

  return (
    <div
      ref={autocompleteWindowRef}
      id="autocompleteWindow"
      className="relative flex flex-col overflow-hidden"
    >
      {contents}
    </div>
  );
}

export default App;