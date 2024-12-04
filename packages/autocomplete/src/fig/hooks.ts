import React, { useEffect } from "react";
import {
  EditBufferNotifications,
  Event,
  Keybindings,
  Settings,
  Shell,
  Types,
} from "@aws/amazon-q-developer-cli-api-bindings";
import { AliasMap } from "@aws/amazon-q-developer-cli-shell-parser";
import {
  SettingsMap,
  updateSettings,
} from "@aws/amazon-q-developer-cli-api-bindings-wrappers";
import { clearSpecIndex } from "@aws/amazon-q-developer-cli-autocomplete-parser";
import { updateSelectSuggestionKeybindings } from "../actions";
import { generatorCache } from "../generators/helpers";

// TODO: expose Subscription type from API binding library
type Unwrap<T> = T extends Promise<infer U> ? U : T;
type Subscription = Unwrap<
  NonNullable<ReturnType<(typeof EditBufferNotifications)["subscribe"]>>
>;

export type FigState = {
  buffer: string;
  cursorLocation: number;
  cwd: string | null;
  processUserIsIn: string | null;
  sshContextString: string | null;
  aliases: AliasMap;
  environmentVariables: Record<string, string>;
  shellContext?: Types.ShellContext | undefined;
};

export const initialFigState: FigState = {
  buffer: "",
  cursorLocation: 0,
  cwd: null,
  processUserIsIn: null,
  sshContextString: null,
  aliases: {},
  environmentVariables: {},
  shellContext: undefined,
};

export const useFigSubscriptionEffect = (
  getSubscription: () => Promise<Subscription> | undefined,
  deps?: React.DependencyList,
) => {
  useEffect(() => {
    let unsubscribe: () => void;
    let isStale = false;
    // if the component is unmounted before the subscription is awaited we
    // unsubscribe from the event
    getSubscription()?.then((result) => {
      unsubscribe = result.unsubscribe;
      if (isStale) unsubscribe();
    });
    return () => {
      if (unsubscribe) unsubscribe();
      isStale = true;
    };
  }, deps);
};

export const useFigSettings = (
  setSettings: React.Dispatch<React.SetStateAction<Record<string, unknown>>>,
) => {
  useEffect(() => {
    Settings.current().then((settings) => {
      setSettings(settings);
      updateSettings(settings as SettingsMap);
      updateSelectSuggestionKeybindings(settings as SettingsMap);
    });
  }, []);

  useFigSubscriptionEffect(
    () =>
      Settings.didChange.subscribe((notification) => {
        const settings = JSON.parse(notification.jsonBlob ?? "{}");
        setSettings(settings);
        updateSettings(settings);
        updateSelectSuggestionKeybindings(settings as SettingsMap);
        return { unsubscribe: false };
      }),
    [],
  );
};

export const useFigKeypress = (
  keypressCallback: Parameters<typeof Keybindings.pressed>[0],
) => {
  useFigSubscriptionEffect(
    () => Keybindings.pressed(keypressCallback),
    [keypressCallback],
  );
};

export const useFigAutocomplete = (
  setFigState: React.Dispatch<React.SetStateAction<FigState>>,
) => {
  useFigSubscriptionEffect(
    () =>
      EditBufferNotifications.subscribe((notification) => {
        const buffer = notification.buffer ?? "";
        const cursorLocation = notification.cursor ?? buffer.length;

        const cwd = notification.context?.currentWorkingDirectory ?? null;
        const shellContext = notification.context;
        setFigState((figState) => ({
          ...figState,
          buffer,
          cursorLocation,
          cwd,
          shellContext,
        }));
        return { unsubscribe: false };
      }),
    [],
  );

  useFigSubscriptionEffect(
    () =>
      Shell.processDidChange.subscribe((notification) => {
        const { newProcess } = notification;
        setFigState((figState) => ({
          ...figState,
          processUserIsIn: newProcess?.executable ?? null,
          cwd: newProcess?.directory ?? null,
        }));
        return { unsubscribe: false };
      }),
    [],
  );
};

export const useFigClearCache = () => {
  useFigSubscriptionEffect(() =>
    Event.subscribe("clear-cache", () => {
      console.log("clearing cache");
      window.resetCaches?.();
      generatorCache.clear();
      clearSpecIndex();
      return { unsubscribe: false };
    }),
  );
};
