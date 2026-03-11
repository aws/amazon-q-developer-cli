import { createContext, useContext } from 'react';
import type { TUI } from '../renderer/tui.js';

/**
 * Context object providing access to Twinki's core functionality.
 */
export interface TwinkiContext {
	/** The TUI renderer instance */
	tui: TUI;
	/** Function to exit the application */
	exit: (error?: Error) => void;
}

/**
 * React context for sharing Twinki's core functionality across components.
 */
export const TwinkiCtx = createContext<TwinkiContext | null>(null);

/**
 * Hook for accessing Twinki's core context.
 * 
 * The useTwinkiContext hook provides access to the TUI renderer and
 * application control functions. This is a low-level hook that's
 * primarily used by other Twinki hooks and should rarely be used
 * directly in application code.
 * 
 * @returns The Twinki context object
 * @returns returns.tui - The TUI renderer instance
 * @returns returns.exit - Function to exit the application
 * @throws Error if used outside of a Twinki render tree
 * 
 * @example
 * ```tsx
 * function LowLevelComponent() {
 *   const { tui, exit } = useTwinkiContext();
 * 
 *   // Direct access to TUI for advanced use cases
 *   const handleAdvancedOperation = () => {
 *     tui.requestRender();
 *   };
 * 
 *   return <Text>Advanced component</Text>;
 * }
 * ```
 */
export function useTwinkiContext(): TwinkiContext {
	const ctx = useContext(TwinkiCtx);
	if (!ctx) throw new Error('Twinki hooks must be used inside a Twinki render tree');
	return ctx;
}
