import { useTwinkiContext } from './context.js';

/**
 * Hook for accessing stdout functionality.
 * 
 * The useStdout hook provides access to the standard output stream and
 * a write function that integrates with Twinki's rendering system.
 * The write function should be used for direct terminal output that
 * bypasses the normal React rendering pipeline.
 * 
 * @returns Object containing stdout stream and write function
 * @returns returns.stdout - The standard output stream
 * @returns returns.write - Function to write data directly to the terminal
 * 
 * @example
 * ```tsx
 * function DirectOutputComponent() {
 *   const { stdout, write } = useStdout();
 * 
 *   const handleDirectWrite = () => {
 *     write('\x1b[31mDirect red text\x1b[0m\n'); // Write ANSI-colored text
 *   };
 * 
 *   return (
 *     <Box>
 *       <Text>Click to write directly to terminal</Text>
 *     </Box>
 *   );
 * }
 * ```
 */
export function useStdout(): {
	stdout: NodeJS.WriteStream;
	write: (data: string) => void;
} {
	const { tui } = useTwinkiContext();
	return {
		stdout: process.stdout,
		write: (data: string) => tui.terminal.write(data),
	};
}
