import type { WrapMode, FlexDirection } from '../text/constants.js';

/**
 * Base props that all components can accept.
 */
export interface BaseProps {
	/** Component children */
	children?: React.ReactNode;
}

/**
 * Layout-related props for Yoga layout system.
 */
export interface LayoutProps {
	/** Width in terminal columns */
	width?: number | string;
	/** Height in terminal rows */
	height?: number | string;
	/** Minimum width */
	minWidth?: number;
	/** Maximum width */
	maxWidth?: number;
	/** Minimum height */
	minHeight?: number;
	/** Maximum height */
	maxHeight?: number;
	/** Flex direction */
	flexDirection?: FlexDirection | keyof typeof FlexDirection;
	/** Flex grow factor */
	flexGrow?: number;
	/** Flex shrink factor */
	flexShrink?: number;
	/** Flex basis */
	flexBasis?: number | string;
	/** Flex wrap */
	flexWrap?: 'nowrap' | 'wrap' | 'wrap-reverse';
	/** Justify content */
	justifyContent?: 'flex-start' | 'flex-end' | 'center' | 'space-between' | 'space-around' | 'space-evenly';
	/** Align items */
	alignItems?: 'flex-start' | 'flex-end' | 'center' | 'stretch' | 'baseline';
	/** Align self */
	alignSelf?: 'auto' | 'flex-start' | 'flex-end' | 'center' | 'stretch' | 'baseline';
	/** Padding */
	padding?: number;
	/** Padding top */
	paddingTop?: number;
	/** Padding right */
	paddingRight?: number;
	/** Padding bottom */
	paddingBottom?: number;
	/** Padding left */
	paddingLeft?: number;
	/** Margin */
	margin?: number;
	/** Margin top */
	marginTop?: number;
	/** Margin right */
	marginRight?: number;
	/** Margin bottom */
	marginBottom?: number;
	/** Margin left */
	marginLeft?: number;
	/** Position type */
	position?: 'relative' | 'absolute';
	/** Top position */
	top?: number;
	/** Right position */
	right?: number;
	/** Bottom position */
	bottom?: number;
	/** Left position */
	left?: number;
	/** Display mode */
	display?: 'flex' | 'none';
	/** Row gap */
	rowGap?: number;
	/** Column gap */
	columnGap?: number;
	/** Gap (shorthand) */
	gap?: number;
}

/**
 * Style-related props for visual appearance.
 */
export interface StyleProps {
	/** Text color */
	color?: string;
	/** Background color */
	backgroundColor?: string;
	/** Border style */
	borderStyle?: 'single' | 'double' | 'round' | 'bold' | 'singleDouble' | 'doubleSingle' | 'classic';
	/** Border color */
	borderColor?: string;
	/** Text decoration */
	bold?: boolean;
	/** Italic text */
	italic?: boolean;
	/** Underlined text */
	underline?: boolean;
	/** Strikethrough text */
	strikethrough?: boolean;
	/** Dimmed text */
	dim?: boolean;
	/** Dimmed color (alias for dim) */
	dimColor?: boolean;
	/** Inverse colors */
	inverse?: boolean;
}

/**
 * Text-specific props.
 */
export interface TextProps extends BaseProps, LayoutProps, StyleProps {
	/** Text wrapping mode */
	wrap?: WrapMode | keyof typeof WrapMode;
}

/**
 * Box-specific props.
 */
export interface BoxProps extends BaseProps, LayoutProps, StyleProps {
	/** Overflow behavior */
	overflow?: 'visible' | 'hidden';
}

/**
 * Static component props.
 */
export interface StaticProps extends BaseProps {
	/** Static content items */
	items?: React.ReactNode[];
}

/**
 * Newline component props.
 */
export interface NewlineProps {
	/** Number of newlines */
	count?: number;
}

/**
 * Transform component props.
 */
export interface TransformProps extends BaseProps {
	/** Transform function */
	transform?: (line: string) => string;
}

/**
 * Mouse event props.
 */
export interface MouseProps {
	/** Click handler */
	onClick?: () => void;
	/** Mouse enter handler */
	onMouseEnter?: () => void;
	/** Mouse leave handler */
	onMouseLeave?: () => void;
}

/**
 * Union type of all possible component props.
 */
export type ComponentProps = TextProps & BoxProps & StaticProps & NewlineProps & TransformProps & MouseProps;

/**
 * Type-safe props for specific component types.
 */
export type PropsForType<T extends string> = 
	T extends '#text' ? { textContent?: string } :
	T extends 'twinki-text' ? TextProps :
	T extends 'twinki-box' ? BoxProps :
	T extends 'twinki-static' ? StaticProps :
	T extends 'twinki-newline' ? NewlineProps :
	T extends 'twinki-spacer' ? BaseProps :
	T extends 'twinki-transform' ? TransformProps :
	ComponentProps;