/**
 * Hook for accessing stdin functionality.
 * 
 * The useStdin hook provides access to the standard input stream and
 * raw mode controls. Raw mode is essential for terminal applications
 * that need to handle individual keystrokes rather than line-buffered input.
 * 
 * @returns Object containing stdin stream and raw mode controls
 * @returns returns.stdin - The standard input stream
 * @returns returns.isRawModeSupported - Whether raw mode is supported on this platform
 * @returns returns.setRawMode - Function to enable/disable raw mode
 * 
 * @example
 * ```tsx
 * function InputComponent() {
 *   const { stdin, isRawModeSupported, setRawMode } = useStdin();
 * 
 *   useEffect(() => {
 *     if (isRawModeSupported) {
 *       setRawMode(true); // Enable raw mode for immediate key handling
 *       return () => setRawMode(false); // Restore normal mode on cleanup
 *     }
 *   }, [isRawModeSupported, setRawMode]);
 * 
 *   return <Text>Raw mode: {isRawModeSupported ? 'supported' : 'not supported'}</Text>;
 * }
 * ```
 */
export function useStdin(): {
	stdin: NodeJS.ReadStream;
	isRawModeSupported: boolean;
	setRawMode: (value: boolean) => void;
} {
	return {
		stdin: process.stdin,
		isRawModeSupported: typeof process.stdin.setRawMode === 'function',
		setRawMode: (value: boolean) => {
			if (process.stdin.setRawMode) process.stdin.setRawMode(value);
		},
	};
}
