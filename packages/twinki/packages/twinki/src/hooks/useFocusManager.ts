import { createContext, useContext, useCallback, useRef } from 'react';

/**
 * Internal state for the focus management system.
 */
interface FocusManagerState {
	/** Array of registered focusable component IDs */
	ids: string[];
	/** ID of the currently focused component */
	activeId: string | null;
	/** Whether focus management is enabled */
	enabled: boolean;
}

/**
 * API for managing focus across components.
 */
interface FocusManagerAPI {
	/** Register a component as focusable */
	register(id: string): void;
	/** Unregister a component from focus management */
	unregister(id: string): void;
	/** Check if a component is currently focused */
	isFocused(id: string): boolean;
	/** Enable focus management */
	enableFocus(): void;
	/** Disable focus management */
	disableFocus(): void;
	/** Move focus to the next component */
	focusNext(): void;
	/** Move focus to the previous component */
	focusPrevious(): void;
	/** Focus a specific component by ID */
	focus(id: string): void;
}

const FocusManagerCtx = createContext<FocusManagerState | null>(null);

/**
 * Hook for managing focus state across multiple components.
 * 
 * The useFocusManager hook provides a centralized focus management system
 * that allows components to register themselves as focusable and provides
 * methods for navigating between them. This is typically used internally
 * by the useFocus hook.
 * 
 * The focus manager maintains a list of focusable components and provides
 * methods for cycling through them, enabling keyboard navigation in
 * terminal applications.
 * 
 * @returns API object for focus management operations
 * @returns returns.register - Register a component as focusable
 * @returns returns.unregister - Unregister a component from focus management
 * @returns returns.isFocused - Check if a component is currently focused
 * @returns returns.enableFocus - Enable focus management
 * @returns returns.disableFocus - Disable focus management
 * @returns returns.focusNext - Move focus to the next component
 * @returns returns.focusPrevious - Move focus to the previous component
 * @returns returns.focus - Focus a specific component by ID
 * 
 * @example
 * ```tsx
 * function FocusableList({ items }) {
 *   const { register, unregister, isFocused, focusNext, focusPrevious } = useFocusManager();
 * 
 *   useInput((input, key) => {
 *     if (key.tab) {
 *       focusNext();
 *     } else if (key.shift && key.tab) {
 *       focusPrevious();
 *     }
 *   });
 * 
 *   return (
 *     <Box flexDirection="column">
 *       {items.map((item, index) => (
 *         <FocusableItem key={index} id={`item-${index}`} />
 *       ))}
 *     </Box>
 *   );
 * }
 * ```
 */
export function useFocusManager(): FocusManagerAPI {
	const stateRef = useRef<FocusManagerState>({ ids: [], activeId: null, enabled: true });
	const state = stateRef.current;

	const register = useCallback((id: string) => {
		if (!state.ids.includes(id)) state.ids.push(id);
	}, [state]);

	const unregister = useCallback((id: string) => {
		const idx = state.ids.indexOf(id);
		if (idx !== -1) state.ids.splice(idx, 1);
		if (state.activeId === id) state.activeId = null;
	}, [state]);

	const isFocused = useCallback((id: string) => {
		return state.enabled && state.activeId === id;
	}, [state]);

	const focusNext = useCallback(() => {
		if (!state.enabled || state.ids.length === 0) return;
		const idx = state.activeId ? state.ids.indexOf(state.activeId) : -1;
		state.activeId = state.ids[(idx + 1) % state.ids.length]!;
	}, [state]);

	const focusPrevious = useCallback(() => {
		if (!state.enabled || state.ids.length === 0) return;
		const idx = state.activeId ? state.ids.indexOf(state.activeId) : 0;
		state.activeId = state.ids[(idx - 1 + state.ids.length) % state.ids.length]!;
	}, [state]);

	const focus = useCallback((id: string) => {
		if (state.ids.includes(id)) state.activeId = id;
	}, [state]);

	return {
		register,
		unregister,
		isFocused,
		enableFocus: useCallback(() => { state.enabled = true; }, [state]),
		disableFocus: useCallback(() => { state.enabled = false; }, [state]),
		focusNext,
		focusPrevious,
		focus,
	};
}
