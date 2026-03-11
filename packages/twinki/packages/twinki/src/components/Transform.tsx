import React from 'react';

/**
 * Props for the Transform component.
 */
export interface TransformProps {
	/** Child components to transform */
	children?: React.ReactNode;
	/** Function to transform each line of output */
	transform: (output: string) => string;
}

/**
 * Transform component for applying custom transformations to rendered output.
 * 
 * The Transform component allows you to apply custom transformations to
 * the rendered output of its children. The transform function is called
 * for each line of output, allowing you to modify, filter, or enhance
 * the content before it's displayed.
 * 
 * This is useful for:
 * - Adding prefixes or suffixes to lines
 * - Applying custom styling or formatting
 * - Filtering or modifying content
 * - Adding line numbers or timestamps
 * 
 * @param props - The component props
 * @param props.children - Child components to transform
 * @param props.transform - Function to transform each line of output
 * @returns A React element that applies transformations to its children
 * 
 * @example
 * ```tsx
 * <Transform transform={(line) => `> ${line}`}>
 *   <Text>This will be prefixed</Text>
 *   <Text>So will this</Text>
 * </Transform>
 * 
 * <Transform transform={(line) => line.toUpperCase()}>
 *   <Text>this will be uppercase</Text>
 * </Transform>
 * ```
 */
export const Transform: React.FC<TransformProps> = (props) => {
	return React.createElement('twinki-transform', { transform: props.transform }, props.children);
};

Transform.displayName = 'Transform';
