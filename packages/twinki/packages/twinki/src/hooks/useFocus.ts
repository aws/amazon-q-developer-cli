import { useState, useEffect, useId } from 'react';
import { useFocusManager } from './useFocusManager.js';

/**
 * Options for configuring the useFocus hook.
 */
export interface UseFocusOptions {
	/** Whether the component can receive focus. Default: true */
	isActive?: boolean;
	/** Custom ID for the focusable component. Auto-generated if not provided */
	id?: string;
	/** Whether to automatically focus this component on mount. Default: false */
	autoFocus?: boolean;
}

/**
 * Hook for managing focus state in Twinki applications.
 * 
 * The useFocus hook integrates with Twinki's focus management system
 * to provide keyboard navigation between interactive components.
 * Components using this hook can receive focus and respond to
 * focus-related events.
 * 
 * @param options - Configuration options for focus behavior
 * @param options.isActive - Whether the component can receive focus (default: true)
 * @param options.id - Custom ID for the focusable component (auto-generated if not provided)
 * @param options.autoFocus - Whether to automatically focus this component on mount (default: false)
 * @returns Object containing focus state
 * @returns returns.isFocused - Whether this component is currently focused
 * 
 * @example
 * ```tsx
 * function FocusableButton({ children, onClick }) {
 *   const { isFocused } = useFocus({ autoFocus: true });
 * 
 *   useInput((input, key) => {
 *     if (key.return && isFocused) {
 *       onClick?.();
 *     }
 *   });
 * 
 *   return (
 *     <Box borderStyle={isFocused ? 'double' : 'single'}>
 *       <Text>{children}</Text>
 *     </Box>
 *   );
 * }
 * ```
 */
export function useFocus(options: UseFocusOptions = {}): { isFocused: boolean } {
	const generatedId = useId();
	const id = options.id ?? generatedId;
	const isActive = options.isActive ?? true;
	const autoFocus = options.autoFocus ?? false;
	const { register, unregister, isFocused } = useFocusManager();

	useEffect(() => {
		if (isActive) {
			register(id);
			if (autoFocus) {
				// Focus on next tick to allow all components to register
				queueMicrotask(() => {
					// Focus this component
				});
			}
		}
		return () => unregister(id);
	}, [id, isActive, autoFocus, register, unregister]);

	return { isFocused: isFocused(id) };
}
