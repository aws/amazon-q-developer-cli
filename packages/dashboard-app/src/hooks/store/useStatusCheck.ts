import { StoreContext } from "@/context/zustand";
import { InstallKey } from "@/types/preferences";
import { useContext } from "react";
import { useStore } from "zustand";

export function useAccessibilityCheck() {
  const store = useContext(StoreContext);
  if (!store) throw new Error("Missing StoreContext.Provider in the tree");
  return [
    useStore(store, (state) => state.accessibilityIsInstalled),
    useStore(store, (state) => state.refreshAccessibilityIsInstalled),
  ] as const;
}

export function useDotfilesCheck() {
  const store = useContext(StoreContext);
  if (!store) throw new Error("Missing StoreContext.Provider in the tree");
  return [
    useStore(store, (state) => state.dotfilesIsInstalled),
    useStore(store, (state) => state.refreshDotfilesIsInstalled),
  ] as const;
}

export function useInputMethodCheck() {
  const store = useContext(StoreContext);
  if (!store) throw new Error("Missing StoreContext.Provider in the tree");
  return [
    useStore(store, (state) => state.accessibilityIsInstalled),
    useStore(store, (state) => state.refreshAccessibilityIsInstalled),
  ] as const;
}

export function useGnomeExtensionCheck() {
  const store = useContext(StoreContext);
  if (!store) throw new Error("Missing StoreContext.Provider in the tree");
  return [
    useStore(store, (state) => state.gnomeExtensionIsInstalled),
    useStore(store, (state) => state.refreshGnomeExtensionIsInstalled),
  ] as const;
}

export function useDesktopEntryCheck() {
  const store = useContext(StoreContext);
  if (!store) throw new Error("Missing StoreContext.Provider in the tree");
  return [
    useStore(store, (state) => state.desktopEntryIsInstalled),
    useStore(store, (state) => state.refreshDesktopEntryIsInstalled),
  ] as const;
}

/**
 * @param check The install method to check
 * @returns The status of the check is installed, if undefined it is is either loading
 * or unable to get a status, the second part is a callback to refresh the status
 */
export function useStatusCheck(check: InstallKey) {
  const [accessibilityIsInstalled, refreshAccessibilityIsInstalled] =
    useAccessibilityCheck();
  const [dotfilesIsInstalled, refreshDotfilesIsInstalled] = useDotfilesCheck();
  const [inputMethodIsInstalled, refreshInputMethodIsInstalled] =
    useInputMethodCheck();
  const [desktopEntryIsInstalled, refreshDesktopEntryIsInstalled] =
    useDesktopEntryCheck();
  const [gnomeExtensionIsInstalled, refreshGnomeExtensionIsInstalled] =
    useGnomeExtensionCheck();

  if (check === "accessibility") {
    return [accessibilityIsInstalled, refreshAccessibilityIsInstalled] as const;
  } else if (check === "dotfiles") {
    return [dotfilesIsInstalled, refreshDotfilesIsInstalled] as const;
  } else if (check === "inputMethod") {
    return [inputMethodIsInstalled, refreshInputMethodIsInstalled] as const;
  } else if (check === "desktopEntry") {
    return [desktopEntryIsInstalled, refreshDesktopEntryIsInstalled] as const;
  } else if (check === "gnomeExtension") {
    return [
      gnomeExtensionIsInstalled,
      refreshGnomeExtensionIsInstalled,
    ] as const;
  } else {
    throw new Error(`Invalid check for install key: ${check}`);
  }
}
