import React from 'react';

/**
 * Spacer component for flexible spacing in layouts.
 * 
 * The Spacer component automatically grows to fill available space in
 * flex layouts. It's useful for pushing content to opposite ends of
 * a container or creating flexible spacing between elements.
 * 
 * The spacer has flexGrow: 1 by default, meaning it will expand to
 * fill any remaining space in its flex container.
 * 
 * @returns A React element that grows to fill available space
 * 
 * @example
 * ```tsx
 * <Box flexDirection="row">
 *   <Text>Left</Text>
 *   <Spacer />
 *   <Text>Right</Text>
 * </Box>
 * ```
 */
export const Spacer: React.FC = () => {
	return React.createElement('twinki-spacer', { flexGrow: 1 });
};

Spacer.displayName = 'Spacer';
