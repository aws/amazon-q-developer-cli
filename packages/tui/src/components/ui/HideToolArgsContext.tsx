import React from 'react';

/**
 * When true, tool-use entries collapse to a minimal title + status line,
 * hiding all args/diff/command/params/output. Spec mode enables this to keep
 * the conversation uncluttered; every other mode leaves it at the default
 * (`false`), preserving the full tool renderers byte-for-byte.
 */
export const HideToolArgsContext = React.createContext(false);

/** Whether tool args should be hidden for the current render scope. */
export function useHideToolArgs(): boolean {
  return React.useContext(HideToolArgsContext);
}
