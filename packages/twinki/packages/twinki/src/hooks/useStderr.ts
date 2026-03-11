/**
 * Hook for accessing stderr functionality.
 * 
 * The useStderr hook provides access to the standard error stream and
 * a write function for outputting error messages. Unlike stdout, stderr
 * output is not managed by Twinki's rendering system and writes directly
 * to the error stream.
 * 
 * @returns Object containing stderr stream and write function
 * @returns returns.stderr - The standard error stream
 * @returns returns.write - Function to write data directly to stderr
 * 
 * @example
 * ```tsx
 * function ErrorComponent() {
 *   const { stderr, write } = useStderr();
 * 
 *   const handleError = () => {
 *     write('Error: Something went wrong\n'); // Write to stderr
 *   };
 * 
 *   return (
 *     <Box>
 *       <Text>Click to write error message</Text>
 *     </Box>
 *   );
 * }
 * ```
 */
export function useStderr(): {
	stderr: NodeJS.WriteStream;
	write: (data: string) => void;
} {
	return {
		stderr: process.stderr,
		write: (data: string) => process.stderr.write(data),
	};
}
