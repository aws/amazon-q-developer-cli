import { StoreContext } from "@/context/zustand";
import { useContext } from "react";
import { useStore } from "zustand";
import z from "zod";

export function useLocalState(
  key: string,
): readonly [unknown, (value: unknown) => void] {
  const store = useContext(StoreContext);
  if (!store) throw new Error("Missing StoreContext.Provider in the tree");
  return [
    useStore(store, (state) => {
      const s = state.state;
      if (!s) throw new Error("Missing state in the store");
      return s[key];
    }),
    useStore(store, (state) => (value: unknown) => state.setState(key, value)),
  ] as const;
}

export function useLocalStateDefault(
  key: string,
  defaultValue: unknown,
): readonly [unknown, (value: unknown) => void] {
  const [state, setState] = useLocalState(key);
  return [state === undefined ? defaultValue : state, setState];
}

export function useLocalStateZod<T>(
  key: string,
  schema: z.Schema<T>,
): readonly [T | undefined, (value: T) => void] {
  const [state, setState] = useLocalState(key);
  return [
    state === undefined ? undefined : schema.parse(state),
    (value: T) => setState(value),
  ];
}

export function useLocalStateZodDefault<T>(
  key: string,
  schema: z.Schema<T>,
  defaultValue: T,
): readonly [T, (value: T) => void] {
  const [state, setState] = useLocalState(key);

  if (state === undefined) {
    return [defaultValue, setState];
  }

  const parsed = schema.safeParse(state);
  return [
    parsed.success ? parsed.data : defaultValue,
    (value: T) => setState(value),
  ];
}

export function useRefreshLocalState() {
  const store = useContext(StoreContext);
  if (!store) throw new Error("Missing StoreContext.Provider in the tree");
  return useStore(store, (state) => state.refreshLocalState);
}
