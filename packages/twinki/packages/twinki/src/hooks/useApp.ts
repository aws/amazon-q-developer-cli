import { useCallback } from 'react';
import { useTwinkiContext } from './context.js';

/**
 * Hook for accessing application-level functionality.
 * 
 * The useApp hook provides access to core application functions like
 * exiting the application. It's the primary way to control the overall
 * application lifecycle from within components.
 * 
 * @returns Object containing application control functions
 * @returns returns.exit - Function to exit the application, optionally with an error
 * 
 * @example
 * ```tsx
 * function MyComponent() {
 *   const { exit } = useApp();
 * 
 *   const handleQuit = () => {
 *     exit(); // Clean exit
 *   };
 * 
 *   const handleError = () => {
 *     exit(new Error('Something went wrong')); // Exit with error
 *   };
 * 
 *   return (
 *     <Box>
 *       <Text>Press q to quit, e for error</Text>
 *     </Box>
 *   );
 * }
 * ```
 */
export function useApp(): { exit: (error?: Error) => void } {
	const { exit } = useTwinkiContext();
	return { exit: useCallback((error?: Error) => exit(error), [exit]) };
}
