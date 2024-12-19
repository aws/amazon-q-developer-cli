import { SETTINGS } from "@aws/amazon-q-developer-cli-api-bindings-wrappers";
import React, {
  useState,
  useEffect,
  useCallback,
  useRef,
  useMemo,
  MutableRefObject,
} from "react";

import logger from "loglevel";
import { getCWDForFilesAndFolders } from "@aws/amazon-q-developer-cli-shared/utils";
import { getMaxHeight, getMaxWidth, POPOUT_WIDTH } from "./window";
import { useParseArgumentsEffect } from "./parser/hooks";
import "./index.css";
import AutoSizedList, { AutoSizedHandleRef } from "./components/AutoSizedList";

import {
  useFigKeypress,
  useFigAutocomplete,
  useFigSettings,
  useFigClearCache,
} from "./fig/hooks";

import { getCommonSuggestionPrefix } from "./suggestions/helpers";

import { useAutocompleteStore } from "./state";
// import { Visibility } from "./state/types";
import {
  useAutocompleteKeypressCallback,
  setInterceptKeystrokes,
  Direction,
  setInterceptKeystrokesBackend,
} from "./hooks/keypress";
import {
  useShake,
  useDynamicResizeObserver,
  useSystemTheme,
} from "./hooks/helpers";

import Suggestion from "./components/Suggestion";
import Description, { DescriptionPosition } from "./components/Description";
import Preview from "./components/Preview";
import { setFontFamily, setFontSize, setTheme } from "./fig/themes";
import LoadingIcon from "./components/LoadingIcon";
import {
  CsWebsocket,
  WebsocketMuxBackend,
} from "@aws/amazon-q-developer-cli-ipc-backend-websocket-mux";
import { IpcBackend } from "@aws/amazon-q-developer-cli-ipc-backend-core";

import "./index.css";

const getIconPath = (cwd: string): string => {
  const home = window?.fig?.constants?.home;
  const path = home && cwd.startsWith("~") ? home + cwd.slice(1) : cwd;
  return path.startsWith("//") ? path.slice(1) : path;
};

export interface IpcBackendProps {
  type: string;
  websocket?: CsWebsocket;
}

export interface AutocompleteProps {
  ipcBackend: IpcBackendProps;
  enableMocks?: boolean;
  onSelect?: (ipcBackend: IpcBackend, item: any) => void;
}

function Autocomplete({
  ipcBackend: ipcBackendProps,
  enableMocks,
  onSelect,
}: AutocompleteProps) {
  const {
    generatorStates,
    suggestions,
    selectedIndex,
    // visibleState,
    figState: { cwd, shellContext },
    parserResult: { searchTerm, currentArg },
    settings,
    setSettings,
    insertTextForItem,

    fuzzySearchEnabled,
    setUserFuzzySearchEnabled,
    setFigState,
  } = useAutocompleteStore();

  useMemo(() => {
    logger.enableAll();
  }, []);

  const ipcBackend = useMemo(
    () => new WebsocketMuxBackend(ipcBackendProps.websocket!),
    [ipcBackendProps.websocket],
  );
  const suggestionsMock: Fig.Suggestion[] = [
    {
      name: "ec2",
      description: "The command line interface for Fig",
      insertValue: 'echo "123"',
    },
    {
      name: "account",
      description: "It's not what you think",
    },
    {
      name: "acm",
      description: "The largest and oldest of the group",
    },
    {
      name: "acm-pca",
      description: "With the circle",
    },
  ];

  const [systemTheme] = useSystemTheme();
  const [isShaking, shake] = useShake();
  const [isLoadingSuggestions, setIsLoadingSuggestions] = useState(false);
  const [windowState, setWindowState] = useState({
    isDescriptionSeparate: false,
    isAboveCursor: true,
    descriptionPosition: "unknown" as DescriptionPosition,
    previewPosition: "right" as DescriptionPosition,
  });
  const {
    [SETTINGS.THEME]: theme,
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

  const isLoading = isLoadingSuggestions;

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
    () =>
      getCommonSuggestionPrefix(
        selectedIndex,
        enableMocks ? suggestionsMock : suggestions,
      ),
    [selectedIndex, suggestions],
  );

  useEffect(() => {
    setWindowState((state) => ({
      isAboveCursor: alwaysShowDescription ? state.isAboveCursor : false,
      isDescriptionSeparate: alwaysShowDescription as boolean,
      descriptionPosition: "unknown",
      previewPosition: state.previewPosition,
    }));
  }, [alwaysShowDescription]);

  // Effect hooks into fig autocomplete events, parser, async generator results, and keypresses.
  const toggleDescriptionPopout = () => {
    setWindowState((state) => ({
      // if we are bringing the description back to the suggestion list, set isAboveCursor to false.
      isAboveCursor: state.isDescriptionSeparate ? false : state.isAboveCursor,
      isDescriptionSeparate: !state.isDescriptionSeparate,
      descriptionPosition: "unknown",
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
    ipcBackend,
  );

  useEffect(() => {
    window.globalCWD = "";
    window.globalTerminalSessionId = "";
    window.globalSSHString = "";
  }, []);

  useFigAutocomplete(setFigState, ipcBackend);
  useParseArgumentsEffect(setIsLoadingSuggestions);
  useFigSettings(setSettings);
  useFigKeypress(keypressCallback, ipcBackend);
  useFigClearCache();

  // useEffect(() => {
  //   Settings.get(SETTINGS.DISABLE_HISTORY_LOADING)
  //     .catch(() => undefined)
  //     .then((res) => {
  //       if (!JSON.parse(res?.jsonBlob ?? "false")) {
  //         loadHistory({});
  //       }
  //     });
  // }, []);

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
  // const isHidden = visibleState !== Visibility.VISIBLE;
  const isHidden = false;
  const interceptKeystrokes = Boolean(
    !isHidden && (enableMocks ? suggestionsMock : suggestions).length > 0,
  );

  useEffect(() => {
    logger.info("Setting intercept keystrokes", {
      suggestions,
      interceptKeystrokes,
    });
    setInterceptKeystrokesBackend(
      ipcBackend,
      interceptKeystrokes,
      (enableMocks ? suggestionsMock : suggestions).length > 0,
      shellContext?.sessionId,
    );
  }, [
    ipcBackend,
    interceptKeystrokes,
    suggestions.length,
    shellContext?.sessionId,
  ]);
  useEffect(() => {
    setTheme(systemTheme, theme as string);
  }, [theme, systemTheme]);
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

  const hasSpecialArgDescription =
    (enableMocks ? suggestionsMock : suggestions).length === 0 &&
    Boolean(currentArg?.name || currentArg?.description);

  const onResize: (size: { height?: number; width?: number }) => void =
    useCallback(
      ({ height, width }) => {
        const onLeft =
          !hasSpecialArgDescription &&
          windowState.descriptionPosition === "unknown";

        const frame = {
          height: height || 1,
          width: width || 1,
          anchorX: onLeft ? -POPOUT_WIDTH : 0,
          offsetFromBaseline: -3,
        };
        logger.debug("Setting window position", { frame });
      },
      // eslint-disable-next-line react-hooks/exhaustive-deps
      [
        windowState.descriptionPosition,
        hasSpecialArgDescription,
        isHidden,
        // eslint-disable-next-line react-hooks/exhaustive-deps
        (enableMocks ? suggestionsMock : suggestions)[selectedIndex]
          ?.previewComponent,
      ],
    );

  useEffect(() => {
    onResize({});
  }, [onResize]);

  const { ref: autocompleteWindowRef } = useDynamicResizeObserver({ onResize });

  if (isHidden) {
    return <div ref={autocompleteWindowRef} id="autocompleteWindow" />;
  }

  const hasSuggestions =
    (enableMocks ? suggestionsMock : suggestions).length > 0;

  const descriptionPosition: DescriptionPosition =
    hasSuggestions && windowState.isDescriptionSeparate
      ? windowState.descriptionPosition
      : "unknown";

  const previewPosition: DescriptionPosition =
    hasSuggestions &&
    (enableMocks ? suggestionsMock : suggestions)[selectedIndex]
      ?.previewComponent
      ? windowState.previewPosition
      : "unknown";

  const description = (
    <Description
      currentArg={currentArg}
      hasSuggestions={hasSuggestions}
      selectedItem={
        (enableMocks ? suggestionsMock : suggestions)[selectedIndex]
      }
      shouldShowHint={!isLoading && !alwaysShowDescription}
      position={descriptionPosition}
      height={size.itemSize}
    />
  );

  const selectedItem = (enableMocks ? suggestionsMock : suggestions)[
    selectedIndex
  ];
  // TODO: re-enable preview
  // eslint-disable-next-line no-constant-binary-expression
  const preview = false && Boolean(selectedItem && !hidePreview) && (
    <Preview selectedItem={selectedItem} position={previewPosition} />
  );

  const hasBottomDescription =
    descriptionPosition === "unknown" && description !== null;
  const listClasses = [
    "rounded",
    isShaking && "shake",
    hasBottomDescription && "rounded-b-none",
  ];

  const devModeWarning = Boolean(devMode) && (
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
          <code className="rounded-sm bg-zinc-700 p-0.5 text-zinc-200">
            Ctrl + C
          </code>{" "}
          in the dev mode process
        </div>
        <div>
          {"• "}
          <button
            type="button"
            className="text-xs underline"
            onClick={() => {}}
          >
            Click to disable
          </button>
        </div>
      </div>
    </div>
  );

  let contents: React.ReactElement;

  // eslint-disable-next-line no-constant-condition
  if (isLoading && false) {
    contents = <LoadingIcon />;
  } else {
    contents = (
      <>
        {windowState.isAboveCursor && devModeWarning}
        <div className="flex flex-row gap-1.5 p-1">
          {descriptionPosition === "left" && description}
          {previewPosition === "left" && preview}
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
              itemCount={Math.round(
                (enableMocks ? suggestionsMock : suggestions).length,
              )}
            >
              {({ index, style }) => (
                <Suggestion
                  style={style}
                  suggestion={
                    (enableMocks ? suggestionsMock : suggestions)[index]
                  }
                  commonPrefix={commonPrefix || ""}
                  // eslint-disable-next-line @typescript-eslint/no-explicit-any
                  onClick={(ipcBackend: IpcBackend, item: any) => {
                    return onSelect
                      ? onSelect(ipcBackend, item)
                      : insertTextForItem(ipcBackend, item);
                  }}
                  isActive={selectedIndex === index}
                  searchTerm={searchTerm}
                  fuzzySearchEnabled={fuzzySearchEnabled}
                  iconPath={iconPath}
                  iconSize={size.itemSize * 0.75}
                  ipcBackend={ipcBackend}
                />
              )}
            </AutoSizedList>
            <div className="scrollbar-none flex flex-shrink-0 flex-row">
              {descriptionPosition === "unknown" && description}
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
          {previewPosition === "right" && preview}
          {descriptionPosition === "right" && description}
        </div>
        {!windowState.isAboveCursor && devModeWarning}
      </>
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

export default Autocomplete;
