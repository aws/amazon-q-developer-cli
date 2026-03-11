import React from 'react';

/**
 * Props for the Newline component.
 */
export interface NewlineProps {
	/** Number of newlines to insert (default: 1) */
	count?: number;
}

/**
 * Newline component for inserting line breaks.
 * 
 * The Newline component creates empty lines in the output, useful for
 * adding vertical spacing between content. Multiple newlines can be
 * created with the count prop.
 * 
 * @param props - The component props
 * @param props.count - Number of newlines to insert (default: 1)
 * @returns A React element representing line breaks
 * 
 * @example
 * ```tsx
 * <Text>First line</Text>
 * <Newline />
 * <Text>Second line with gap</Text>
 * <Newline count={3} />
 * <Text>Line after large gap</Text>
 * ```
 */
export const Newline: React.FC<NewlineProps> = ({ count = 1 }) => {
	return React.createElement('twinki-newline', { count });
};

Newline.displayName = 'Newline';
