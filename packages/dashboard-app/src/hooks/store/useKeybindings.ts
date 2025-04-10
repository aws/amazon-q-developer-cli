import { StoreContext } from "@/context/zustand";
import { useContext } from "react";
import { useStore } from "zustand";

/**
 * @param key The id of the setting
 * @returns An array with the current value of the setting and a callback to set the value
 */
export function useKeybindings(command: string, defaults: string[]) {
  const store = useContext(StoreContext);
  if (!store) throw new Error("Missing StoreContext.Provider in the tree");

  const settings = useStore(store, (state) => state.settings!);

  // Check every entry in settings to see if its value is the command being passed down as the first arg
  // If it is, return the key (format: `autocomplete.keybindings.${keystroke}`).
  const keybindingsByCommand = Object.entries(settings)
    .filter((k) => k[1] === command)
    .map((e) => e[0].split(".")[2]);

  // Check each default to make sure that it hasn't been reassigned to a different command.
  // If it's been reassigned, remove it from the array
  const unchangedDefaults = defaults.filter((d) => {
    // Has the key been assigned to anything?
    const index = Object.keys(settings).indexOf(
      `autocomplete.keybindings.${d}`,
    );
    const value = Object.values(settings)[index];
    const keyIsSafe = index === -1 && value !== "ignore";
    // const keyCommandMatch = value === command

    // return keyIsSafe || keyCommandMatch
    return keyIsSafe;
  });

  return [...keybindingsByCommand, ...unchangedDefaults];
}
