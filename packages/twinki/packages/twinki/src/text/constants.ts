/**
 * Regular expression to match dangerous CSI escape sequences.
 * 
 * Matches CSI sequences that could interfere with terminal rendering
 * by moving the cursor, clearing the screen, or changing terminal modes.
 * Preserves SGR sequences (\x1b[...m) which are safe for styling.
 */
export const DANGEROUS_CSI_RE = /\x1b\[[\d;?]*[A-LN-Za-ln-z]/g;

/**
 * ANSI reset sequence to clear all formatting.
 */
export const ANSI_RESET = '\x1b[0m';

/**
 * ANSI style codes for text formatting.
 */
export const ANSI_CODES = {
	BOLD: '1',
	DIM: '2',
	ITALIC: '3',
	UNDERLINE: '4',
	STRIKETHROUGH: '9',
	INVERSE: '7'
} as const;

/**
 * Component type constants for node identification.
 */
export const NODE_TYPES = {
	TEXT: '#text',
	TWINKI_TEXT: 'twinki-text',
	TWINKI_BOX: 'twinki-box',
	TWINKI_STATIC: 'twinki-static',
	TWINKI_NEWLINE: 'twinki-newline',
	TWINKI_SPACER: 'twinki-spacer',
	TWINKI_TRANSFORM: 'twinki-transform'
} as const;

/**
 * Text wrapping modes for text components.
 */
export enum WrapMode {
	WRAP = 'wrap',
	TRUNCATE = 'truncate',
	TRUNCATE_END = 'truncate-end',
	TRUNCATE_START = 'truncate-start',
	TRUNCATE_MIDDLE = 'truncate-middle'
}

/**
 * Flex direction values for layout.
 */
export enum FlexDirection {
	ROW = 'row',
	COLUMN = 'column',
	ROW_REVERSE = 'row-reverse',
	COLUMN_REVERSE = 'column-reverse'
}

/**
 * Common numeric constants used throughout the codebase.
 */
export const CONSTANTS = {
	/** Default terminal width when width is invalid */
	DEFAULT_TERMINAL_WIDTH: 80,
	/** Default event priority for React reconciler */
	DEFAULT_EVENT_PRIORITY: 16,
	/** Zero-based indexing start */
	ZERO_INDEX: 0,
	/** Single unit increment */
	SINGLE_UNIT: 1,
	/** Double unit increment */
	DOUBLE_UNIT: 2,
	/** Border width in characters */
	BORDER_WIDTH: 1,
	/** Minimum valid width */
	MIN_WIDTH: 1,
	/** No timeout value */
	NO_TIMEOUT: -1,
	/** Invalid timestamp */
	INVALID_TIMESTAMP: -1.1
} as const;

/** Property name constants */
export const PROP_NAMES = {
	TYPE: 'type',
	CHILDREN: 'children',
	HIDDEN: 'hidden'
} as const;