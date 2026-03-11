/**
 * Letter keys a-z
 */
export type Letter = 'a' | 'b' | 'c' | 'd' | 'e' | 'f' | 'g' | 'h' | 'i' | 'j' | 'k' | 'l' | 'm' | 'n' | 'o' | 'p' | 'q' | 'r' | 's' | 't' | 'u' | 'v' | 'w' | 'x' | 'y' | 'z';

/**
 * Symbol keys (printable symbols)
 */
export type SymbolKey = '!' | '@' | '#' | '$' | '%' | '^' | '&' | '*' | '(' | ')' | '-' | '_' | '=' | '+' | '[' | ']' | '{' | '}' | '\\' | '|' | ';' | ':' | '\'' | '"' | ',' | '.' | '<' | '>' | '/' | '?' | '`' | '~';

/**
 * Special keys
 */
export type SpecialKey = 'escape' | 'enter' | 'tab' | 'space' | 'backspace' | 'delete' | 'insert' | 'clear' | 'home' | 'end' | 'pageUp' | 'pageDown' | 'up' | 'down' | 'left' | 'right' | 'f1' | 'f2' | 'f3' | 'f4' | 'f5' | 'f6' | 'f7' | 'f8' | 'f9' | 'f10' | 'f11' | 'f12';

/**
 * Base key types
 */
export type BaseKey = Letter | SymbolKey | SpecialKey;

/**
 * Key event types
 */
export type KeyEventType = 'press' | 'repeat' | 'release';

/**
 * All possible key combinations with modifiers
 */
export type KeyId = 
	| BaseKey
	| `ctrl+${BaseKey}`
	| `shift+${BaseKey}`
	| `alt+${BaseKey}`
	| `ctrl+shift+${BaseKey}`
	| `ctrl+alt+${BaseKey}`
	| `shift+alt+${BaseKey}`
	| `ctrl+shift+alt+${BaseKey}`;

/**
 * Helper object for creating typed key combinations
 */
export const Key = {
	ctrl: <T extends BaseKey>(key: T): `ctrl+${T}` => `ctrl+${key}`,
	shift: <T extends BaseKey>(key: T): `shift+${T}` => `shift+${key}`,
	alt: <T extends BaseKey>(key: T): `alt+${T}` => `alt+${key}`,
	ctrlShift: <T extends BaseKey>(key: T): `ctrl+shift+${T}` => `ctrl+shift+${key}`,
	ctrlAlt: <T extends BaseKey>(key: T): `ctrl+alt+${T}` => `ctrl+alt+${key}`,
	shiftAlt: <T extends BaseKey>(key: T): `shift+alt+${T}` => `shift+alt+${key}`,
	ctrlShiftAlt: <T extends BaseKey>(key: T): `ctrl+shift+alt+${T}` => `ctrl+shift+alt+${key}`,
} as const;