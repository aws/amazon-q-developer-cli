import { StoreContext } from "@/context/zustand";
import { useContext } from "react";
import { useStore } from "zustand";

export function useAuth() {
  const store = useContext(StoreContext);
  if (!store) throw new Error("Missing StoreContext.Provider in the tree");
  return useStore(store, (state) => state.auth!);
}

export function useRefreshAuth() {
  const store = useContext(StoreContext);
  if (!store) throw new Error("Missing StoreContext.Provider in the tree");
  return useStore(store, (state) => state.refreshAuth!);
}

/**
 * Hook to return callbacks for retrieving the current authorization
 * request ID, and for providing the request ID of new authorization
 * attempts.
 */
export function useAuthRequest() {
  const store = useContext(StoreContext);
  if (!store) throw new Error("Missing StoreContext.Provider in the tree");
  return useStore(
    store,
    (state) => [state.currAuthRequestId, state.setAuthRequestId] as const,
  );
}
