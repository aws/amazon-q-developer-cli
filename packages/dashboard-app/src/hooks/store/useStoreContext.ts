import { StoreContext } from "@/context/zustand";
import { State } from "@/lib/store";
import { useContext } from "react";
import { useStore } from "zustand";

export function useStoreContext<T>(selector: (state: State) => T): T {
  const store = useContext(StoreContext);
  if (!store) throw new Error("Missing StoreContext.Provider in the tree");
  return useStore(store, selector);
}
