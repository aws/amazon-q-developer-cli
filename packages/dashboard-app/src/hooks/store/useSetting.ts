import { StoreContext } from "@/context/zustand";
import { useContext } from "react";
import { useStore } from "zustand";

/**
 * @param key The id of the setting
 * @returns An array with the current value of the setting and a callback to set the value
 */
export function useSetting(key: string) {
  const store = useContext(StoreContext);
  if (!store) throw new Error("Missing StoreContext.Provider in the tree");

  return [
    useStore(store, (state) => state.settings![key]),
    useStore(
      store,
      (state) => (value: unknown) => state.setSetting(key, value),
    ),
  ] as const;
}
