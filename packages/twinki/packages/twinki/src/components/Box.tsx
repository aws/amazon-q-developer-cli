import React from 'react';

/**
 * Props for the Box component.
 */
export interface BoxProps {
	/** Child components to render inside the box */
	children?: React.ReactNode;
	
	// Dimensions
	/** Width of the box (number or percentage string) */
	width?: number | string;
	/** Height of the box (number or percentage string) */
	height?: number | string;
	/** Minimum width constraint */
	minWidth?: number;
	/** Minimum height constraint */
	minHeight?: number;
	
	// Flex
	/** Direction of flex layout */
	flexDirection?: 'row' | 'column' | 'row-reverse' | 'column-reverse';
	/** How much the box should grow */
	flexGrow?: number;
	/** How much the box should shrink */
	flexShrink?: number;
	/** Base size before growing/shrinking */
	flexBasis?: number;
	/** Whether flex items should wrap */
	flexWrap?: 'nowrap' | 'wrap' | 'wrap-reverse';
	
	// Alignment
	/** How to align child items */
	alignItems?: 'flex-start' | 'center' | 'flex-end' | 'stretch';
	/** How this box aligns within its parent */
	alignSelf?: 'flex-start' | 'center' | 'flex-end' | 'stretch';
	/** How to distribute space between child items */
	justifyContent?: 'flex-start' | 'center' | 'flex-end' | 'space-between' | 'space-around' | 'space-evenly';
	
	// Padding
	/** Padding on all sides */
	padding?: number;
	/** Top padding */
	paddingTop?: number;
	/** Bottom padding */
	paddingBottom?: number;
	/** Left padding */
	paddingLeft?: number;
	/** Right padding */
	paddingRight?: number;
	/** Horizontal (left and right) padding */
	paddingX?: number;
	/** Vertical (top and bottom) padding */
	paddingY?: number;
	
	// Margin
	/** Margin on all sides */
	margin?: number;
	/** Top margin */
	marginTop?: number;
	/** Bottom margin */
	marginBottom?: number;
	/** Left margin */
	marginLeft?: number;
	/** Right margin */
	marginRight?: number;
	/** Horizontal (left and right) margin */
	marginX?: number;
	/** Vertical (top and bottom) margin */
	marginY?: number;
	
	// Border
	/** Border style using Unicode box-drawing characters */
	borderStyle?: 'single' | 'double' | 'round' | 'bold' | 'singleDouble' | 'doubleSingle' | 'classic';
	/** Border color */
	borderColor?: string;
	
	// Visual
	/** Background color */
	backgroundColor?: string;
	/** How to handle content that exceeds the box size */
	overflow?: 'visible' | 'hidden';
	/** Display mode */
	display?: 'flex' | 'none';

	// Mouse events
	/** Called on mouse click (mousedown + mouseup on same element) */
	onClick?: () => void;
	/** Called when mouse enters the element */
	onMouseEnter?: () => void;
	/** Called when mouse leaves the element */
	onMouseLeave?: () => void;
}

/**
 * Box component for layout and styling containers.
 * 
 * The Box component is the primary layout primitive in Twinki, providing
 * flexbox-based layout with support for borders, padding, margins, and
 * background colors. It uses the Yoga layout engine for consistent
 * cross-platform layout behavior.
 * 
 * Key features:
 * - Full flexbox layout support
 * - Unicode box-drawing borders
 * - Padding and margin spacing
 * - Background colors
 * - Overflow handling
 * - Responsive sizing with percentages
 * 
 * @param props - The component props
 * @param props.children - Child components to render inside the box
 * @param props.width - Width of the box (number or percentage string)
 * @param props.height - Height of the box (number or percentage string)
 * @param props.flexDirection - Direction of flex layout
 * @param props.padding - Padding on all sides
 * @param props.margin - Margin on all sides
 * @param props.borderStyle - Border style using Unicode box-drawing characters
 * @param props.backgroundColor - Background color
 * @returns A React element representing a layout container
 * 
 * @example
 * ```tsx
 * <Box flexDirection="row" padding={1} borderStyle="single">
 *   <Box flexGrow={1}>
 *     <Text>Left content</Text>
 *   </Box>
 *   <Box width={20}>
 *     <Text>Right sidebar</Text>
 *   </Box>
 * </Box>
 * ```
 */
export const Box: React.FC<BoxProps> = (props) => {
	return React.createElement('twinki-box', { flexDirection: 'row', ...props }, props.children);
};

Box.displayName = 'Box';
