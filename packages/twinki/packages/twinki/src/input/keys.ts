import type { KeyId, KeyEventType } from './key-types.js';

// Global state for Kitty protocol
let kittyProtocolActive = false;

/**
 * Set whether Kitty keyboard protocol is active
 * @param active - Whether Kitty protocol is active
 */
export function setKittyProtocolActive(active: boolean): void {
	kittyProtocolActive = active;
}

/**
 * Check if Kitty keyboard protocol is active
 * @returns Whether Kitty protocol is active
 */
export function isKittyProtocolActive(): boolean {
	return kittyProtocolActive;
}

/**
 * Check if data represents a key release event
 * @param data - Raw input data
 * @returns Whether this is a key release
 */
/**
 * Check if data represents a key release event.
 *
 * Kitty keyboard protocol (https://sw.kovidgoyal.net/kitty/keyboard-protocol/)
 * reports key events with an event type suffix:
 *   :1 = press, :2 = repeat, :3 = release
 *
 * The suffix appears before the final character that identifies the key type:
 *   CSI codepoint ; modifier :eventtype u    — normal keys (e.g. \x1b[97;1:3u = 'a' release)
 *   CSI 1 ; modifier :eventtype A/B/C/D     — arrow keys  (e.g. \x1b[1;1:3A = up release)
 *   CSI number ; modifier :eventtype ~       — function keys (e.g. \x1b[3;1:3~ = delete release)
 *   CSI 1 ; modifier :eventtype H/F         — home/end    (e.g. \x1b[1;1:3H = home release)
 *
 * We check for `:3` followed by the final byte. This works across all terminals
 * that implement Kitty protocol (Kitty, Ghostty, WezTerm, foot, etc.).
 *
 * Terminals WITHOUT Kitty protocol (xterm, Terminal.app, older Alacritty) never
 * send release events at all — they only send press. So this function returns
 * false for all legacy sequences, which is correct.
 */
export function isKeyRelease(data: string): boolean {
	return data.includes(':3u') || data.includes(':3~') ||
		   data.includes(':3A') || data.includes(':3B') ||
		   data.includes(':3C') || data.includes(':3D') ||
		   data.includes(':3H') || data.includes(':3F');
}

/**
 * Check if data represents a key repeat event
 * @param data - Raw input data
 * @returns Whether this is a key repeat
 */
/**
 * Check if data represents a key repeat event.
 * Same format as release (see isKeyRelease) but with :2 instead of :3.
 */
export function isKeyRepeat(data: string): boolean {
	return data.includes(':2u') || data.includes(':2~') ||
		   data.includes(':2A') || data.includes(':2B') ||
		   data.includes(':2C') || data.includes(':2D') ||
		   data.includes(':2H') || data.includes(':2F');
}

/**
 * Parses Kitty keyboard protocol sequences into structured data.
 * 
 * The Kitty protocol provides enhanced key detection with support for:
 * - Event types (press, repeat, release)
 * - Modifier keys with precise detection
 * - Shifted and base layout key reporting for international keyboards
 * - Disambiguation of keys that produce identical legacy sequences
 * 
 * @param data - Raw input sequence to parse
 * @returns Parsed sequence data or null if not a Kitty sequence
 */
function parseKittySequence(data: string): {
	codepoint: number;
	modifier: number;
	eventType: KeyEventType;
	shiftedKey?: number;
	baseLayoutKey?: number;
} | null {
	// CSI u format with alternate keys (flag 4):
	// \x1b[<codepoint>u
	// \x1b[<codepoint>;<mod>u
	// \x1b[<codepoint>;<mod>:<event>u
	// \x1b[<codepoint>:<shifted>;<mod>u
	// \x1b[<codepoint>:<shifted>:<base>;<mod>u
	// \x1b[<codepoint>::<base>;<mod>u (no shifted key, only base)
	const csiUMatch = data.match(/^\x1b\[(\d+)(?::(\d*))?(?::(\d+))?(?:;(\d+))?(?::(\d+))?u$/);
	if (csiUMatch) {
		const codepoint = parseInt(csiUMatch[1]!, 10);
		const shiftedKey = csiUMatch[2] && csiUMatch[2].length > 0 ? parseInt(csiUMatch[2], 10) : undefined;
		const baseLayoutKey = csiUMatch[3] ? parseInt(csiUMatch[3], 10) : undefined;
		const modValue = csiUMatch[4] ? parseInt(csiUMatch[4], 10) : 1;
		const eventTypeNum = csiUMatch[5] ? parseInt(csiUMatch[5], 10) : 1;
		
		let eventType: KeyEventType = 'press';
		if (eventTypeNum === 2) eventType = 'repeat';
		else if (eventTypeNum === 3) eventType = 'release';
		
		return { codepoint, shiftedKey, baseLayoutKey, modifier: modValue - 1, eventType };
	}

	// Arrow keys with modifier: \x1b[1;<mod>A/B/C/D or \x1b[1;<mod>:<event>A/B/C/D
	const arrowMatch = data.match(/^\x1b\[1;(\d+)(?::(\d+))?([ABCD])$/);
	if (arrowMatch) {
		const modValue = parseInt(arrowMatch[1]!, 10);
		const eventTypeNum = arrowMatch[2] ? parseInt(arrowMatch[2], 10) : 1;
		const arrowCodes: Record<string, number> = { A: -1, B: -2, C: -3, D: -4 };
		
		let eventType: KeyEventType = 'press';
		if (eventTypeNum === 2) eventType = 'repeat';
		else if (eventTypeNum === 3) eventType = 'release';
		
		return { codepoint: arrowCodes[arrowMatch[3]!]!, modifier: modValue - 1, eventType };
	}

	// Functional keys: \x1b[<num>~ or \x1b[<num>;<mod>~ or \x1b[<num>;<mod>:<event>~
	const funcMatch = data.match(/^\x1b\[(\d+)(?:;(\d+))?(?::(\d+))?~$/);
	if (funcMatch) {
		const keyNum = parseInt(funcMatch[1]!, 10);
		const modValue = funcMatch[2] ? parseInt(funcMatch[2], 10) : 1;
		const eventTypeNum = funcMatch[3] ? parseInt(funcMatch[3], 10) : 1;
		const funcCodes: Record<number, number> = {
			2: FUNCTIONAL_CODEPOINTS.insert,
			3: FUNCTIONAL_CODEPOINTS.delete,
			5: FUNCTIONAL_CODEPOINTS.pageUp,
			6: FUNCTIONAL_CODEPOINTS.pageDown,
			7: FUNCTIONAL_CODEPOINTS.home,
			8: FUNCTIONAL_CODEPOINTS.end,
		};
		const codepoint = funcCodes[keyNum];
		if (codepoint !== undefined) {
			let eventType: KeyEventType = 'press';
			if (eventTypeNum === 2) eventType = 'repeat';
			else if (eventTypeNum === 3) eventType = 'release';
			
			return { codepoint, modifier: modValue - 1, eventType };
		}
	}

	// Home/End with modifier: \x1b[1;<mod>H/F or \x1b[1;<mod>:<event>H/F
	const homeEndMatch = data.match(/^\x1b\[1;(\d+)(?::(\d+))?([HF])$/);
	if (homeEndMatch) {
		const modValue = parseInt(homeEndMatch[1]!, 10);
		const eventTypeNum = homeEndMatch[2] ? parseInt(homeEndMatch[2], 10) : 1;
		const codepoint = homeEndMatch[3] === "H" ? FUNCTIONAL_CODEPOINTS.home : FUNCTIONAL_CODEPOINTS.end;
		
		let eventType: KeyEventType = 'press';
		if (eventTypeNum === 2) eventType = 'repeat';
		else if (eventTypeNum === 3) eventType = 'release';
		
		return { codepoint, modifier: modValue - 1, eventType };
	}

	return null;
}

/**
 * Checks if a Kitty protocol sequence matches expected codepoint and modifier.
 * 
 * Handles both direct codepoint matches and base layout key fallbacks
 * for international keyboard layouts. The base layout key is only used
 * when the actual codepoint is not a recognized Latin letter or symbol,
 * preventing false matches in remapped layouts (Dvorak, Colemak, etc.).
 * 
 * @param data - Raw input sequence
 * @param expectedCodepoint - Expected Unicode codepoint
 * @param expectedModifier - Expected modifier bitmask
 * @returns Whether the sequence matches
 */
function matchesKittySequence(data: string, expectedCodepoint: number, expectedModifier: number): boolean {
	const parsed = parseKittySequence(data);
	if (!parsed) return false;
	const actualMod = parsed.modifier & ~LOCK_MASK;
	const expectedMod = expectedModifier & ~LOCK_MASK;

	// Check if modifiers match
	if (actualMod !== expectedMod) return false;

	// Primary match: codepoint matches directly
	if (parsed.codepoint === expectedCodepoint) return true;

	// Alternate match: use base layout key for non-Latin keyboard layouts.
	// This allows Ctrl+С (Cyrillic) to match Ctrl+c (Latin) when terminal reports
	// the base layout key (the key in standard PC-101 layout).
	//
	// Only fall back to base layout key when the codepoint is NOT already a
	// recognized Latin letter (a-z) or symbol (e.g., /, -, [, ;, etc.).
	// When the codepoint is a recognized key, it is authoritative regardless
	// of physical key position. This prevents remapped layouts (Dvorak, Colemak,
	// xremap, etc.) from causing false matches: both letters and symbols move
	// to different physical positions, so Ctrl+K could falsely match Ctrl+V
	// (letter remapping) and Ctrl+/ could falsely match Ctrl+[ (symbol remapping)
	// if the base layout key were always considered.
	if (parsed.baseLayoutKey !== undefined && parsed.baseLayoutKey === expectedCodepoint) {
		const cp = parsed.codepoint;
		const isLatinLetter = cp >= 97 && cp <= 122; // a-z
		const isKnownSymbol = SYMBOL_KEYS.has(String.fromCharCode(cp));
		if (!isLatinLetter && !isKnownSymbol) return true;
	}

	return false;
}

/**
 * Matches xterm modifyOtherKeys format: CSI 27 ; modifiers ; keycode ~
 * 
 * This format is used by terminals when Kitty protocol is not enabled
 * but enhanced key detection is still needed. Modifier values are 1-indexed:
 * 2=shift, 3=alt, 5=ctrl, etc.
 * 
 * @param data - Raw input sequence
 * @param expectedKeycode - Expected keycode
 * @param expectedModifier - Expected modifier bitmask (0-indexed)
 * @returns Whether the sequence matches
 */
function matchesModifyOtherKeys(data: string, expectedKeycode: number, expectedModifier: number): boolean {
	const match = data.match(/^\x1b\[27;(\d+);(\d+)~$/);
	if (!match) return false;
	const modValue = parseInt(match[1]!, 10);
	const keycode = parseInt(match[2]!, 10);
	// Convert from 1-indexed xterm format to our 0-indexed format
	const actualMod = modValue - 1;
	return keycode === expectedKeycode && actualMod === expectedModifier;
}

/**
 * Checks if input data matches any of the provided legacy sequences.
 * 
 * @param data - Raw input sequence
 * @param sequences - Array of possible legacy sequences
 * @returns Whether data matches any sequence
 */
function matchesLegacySequence(data: string, sequences: readonly string[]): boolean {
	return sequences.includes(data);
}

/**
 * Checks if input data matches a legacy modifier sequence for a specific key.
 * 
 * Legacy terminals use different escape sequences for modified keys.
 * This function handles the common patterns for shift and ctrl modifiers.
 * 
 * @param data - Raw input sequence
 * @param key - Base key name
 * @param modifier - Modifier bitmask
 * @returns Whether the sequence matches
 */
function matchesLegacyModifierSequence(data: string, key: string, modifier: number): boolean {
	if (modifier === MODIFIERS.shift) {
		const shiftSeqs = LEGACY_SHIFT_SEQUENCES[key as keyof typeof LEGACY_SHIFT_SEQUENCES];
		return shiftSeqs ? matchesLegacySequence(data, shiftSeqs) : false;
	}
	if (modifier === MODIFIERS.ctrl) {
		const ctrlSeqs = LEGACY_CTRL_SEQUENCES[key as keyof typeof LEGACY_CTRL_SEQUENCES];
		return ctrlSeqs ? matchesLegacySequence(data, ctrlSeqs) : false;
	}
	return false;
}

/**
 * Gets the control character for a key using the universal formula.
 * 
 * Control characters are generated by masking the ASCII code to the lower
 * 5 bits (code & 0x1f). This works for letters a-z and some symbols.
 * Special handling for '-' which maps to the same control code as '_'.
 * 
 * @param key - The key to get control character for
 * @returns Control character string or null if not applicable
 */
function rawCtrlChar(key: string): string | null {
	const char = key.toLowerCase();
	const code = char.charCodeAt(0);
	if ((code >= 97 && code <= 122) || char === "[" || char === "\\" || char === "]" || char === "_") {
		return String.fromCharCode(code & 0x1f);
	}
	// Handle - as _ (same physical key on US keyboards)
	if (char === "-") {
		return String.fromCharCode(31); // Same as Ctrl+_
	}
	return null;
}

/**
 * Parses a KeyId string into its component parts.
 * 
 * Splits modifier+key combinations (e.g., "ctrl+shift+a") into
 * individual modifier flags and the base key.
 * 
 * @param keyId - KeyId string to parse
 * @returns Parsed key components or null if invalid
 */
function parseKeyId(keyId: string): { key: string; ctrl: boolean; shift: boolean; alt: boolean } | null {
	const parts = keyId.toLowerCase().split("+");
	const key = parts[parts.length - 1];
	if (!key) return null;
	return {
		key,
		ctrl: parts.includes("ctrl"),
		shift: parts.includes("shift"),
		alt: parts.includes("alt"),
	};
}

const SYMBOL_KEYS = new Set([
	"`", "-", "=", "[", "]", "\\", ";", "'", ",", ".", "/",
	"!", "@", "#", "$", "%", "^", "&", "*", "(", ")", "_", "+",
	"|", "~", "{", "}", ":", "<", ">", "?"
]);

const MODIFIERS = {
	shift: 1,
	alt: 2,
	ctrl: 4,
} as const;

const LOCK_MASK = 64 + 128; // Caps Lock + Num Lock

const CODEPOINTS = {
	escape: 27,
	tab: 9,
	enter: 13,
	space: 32,
	backspace: 127,
	kpEnter: 57414, // Numpad Enter (Kitty protocol)
} as const;

const ARROW_CODEPOINTS = {
	up: -1,
	down: -2,
	right: -3,
	left: -4,
} as const;

const FUNCTIONAL_CODEPOINTS = {
	delete: -10,
	insert: -11,
	pageUp: -12,
	pageDown: -13,
	home: -14,
	end: -15,
} as const;

const LEGACY_KEY_SEQUENCES = {
	up: ["\x1b[A", "\x1bOA"],
	down: ["\x1b[B", "\x1bOB"],
	right: ["\x1b[C", "\x1bOC"],
	left: ["\x1b[D", "\x1bOD"],
	home: ["\x1b[H", "\x1bOH", "\x1b[1~", "\x1b[7~"],
	end: ["\x1b[F", "\x1bOF", "\x1b[4~", "\x1b[8~"],
	insert: ["\x1b[2~"],
	delete: ["\x1b[3~"],
	pageUp: ["\x1b[5~", "\x1b[[5~"],
	pageDown: ["\x1b[6~", "\x1b[[6~"],
	clear: ["\x1b[E", "\x1bOE"],
	f1: ["\x1bOP", "\x1b[11~", "\x1b[[A"],
	f2: ["\x1bOQ", "\x1b[12~", "\x1b[[B"],
	f3: ["\x1bOR", "\x1b[13~", "\x1b[[C"],
	f4: ["\x1bOS", "\x1b[14~", "\x1b[[D"],
	f5: ["\x1b[15~", "\x1b[[E"],
	f6: ["\x1b[17~"],
	f7: ["\x1b[18~"],
	f8: ["\x1b[19~"],
	f9: ["\x1b[20~"],
	f10: ["\x1b[21~"],
	f11: ["\x1b[23~"],
	f12: ["\x1b[24~"],
} as const;

const LEGACY_SHIFT_SEQUENCES = {
	up: ["\x1b[a"],
	down: ["\x1b[b"],
	right: ["\x1b[c"],
	left: ["\x1b[d"],
	clear: ["\x1b[e"],
	insert: ["\x1b[2$"],
	delete: ["\x1b[3$"],
	pageUp: ["\x1b[5$"],
	pageDown: ["\x1b[6$"],
	home: ["\x1b[7$"],
	end: ["\x1b[8$"],
} as const;

const LEGACY_CTRL_SEQUENCES = {
	up: ["\x1bOa"],
	down: ["\x1bOb"],
	right: ["\x1bOc"],
	left: ["\x1bOd"],
	clear: ["\x1bOe"],
	insert: ["\x1b[2^"],
	delete: ["\x1b[3^"],
	pageUp: ["\x1b[5^"],
	pageDown: ["\x1b[6^"],
	home: ["\x1b[7^"],
	end: ["\x1b[8^"],
} as const;

/**
 * Checks if raw input data matches a specific key identifier.
 * 
 * This is the main key matching function that handles:
 * - Kitty keyboard protocol sequences
 * - Legacy terminal sequences
 * - xterm modifyOtherKeys format
 * - Control characters and modifier combinations
 * - International keyboard layout support
 * 
 * The function automatically adapts to whether Kitty protocol is active
 * and handles the complex mapping between raw terminal input and logical keys.
 * 
 * @param data - Raw input data from terminal
 * @param keyId - Key identifier to match against
 * @returns Whether the data matches the specified key
 * 
 * @example
 * ```typescript
 * // Check for Ctrl+C
 * if (matchesKey(inputData, 'ctrl+c')) {
 *   // Handle interrupt
 * }
 * 
 * // Check for arrow key
 * if (matchesKey(inputData, 'up')) {
 *   // Handle up arrow
 * }
 * ```
 */
export function matchesKey(data: string, keyId: KeyId): boolean {
	const parsed = parseKeyId(keyId);
	if (!parsed) return false;

	const { key, ctrl, shift, alt } = parsed;
	let modifier = 0;
	if (shift) modifier |= MODIFIERS.shift;
	if (alt) modifier |= MODIFIERS.alt;
	if (ctrl) modifier |= MODIFIERS.ctrl;

	switch (key) {
		case "escape":
		case "esc":
			if (modifier !== 0) return false;
			return data === "\x1b" || matchesKittySequence(data, CODEPOINTS.escape, 0);

		case "space":
			if (!kittyProtocolActive) {
				if (ctrl && !alt && !shift && data === "\x00") {
					return true;
				}
				if (alt && !ctrl && !shift && data === "\x1b ") {
					return true;
				}
			}
			if (modifier === 0) {
				return data === " " || matchesKittySequence(data, CODEPOINTS.space, 0);
			}
			return matchesKittySequence(data, CODEPOINTS.space, modifier);

		case "tab":
			if (shift && !ctrl && !alt) {
				return data === "\x1b[Z" || matchesKittySequence(data, CODEPOINTS.tab, MODIFIERS.shift);
			}
			if (modifier === 0) {
				return data === "\t" || matchesKittySequence(data, CODEPOINTS.tab, 0);
			}
			return matchesKittySequence(data, CODEPOINTS.tab, modifier);

		case "enter":
		case "return":
			if (shift && !ctrl && !alt) {
				// CSI u sequences (standard Kitty protocol)
				if (
					matchesKittySequence(data, CODEPOINTS.enter, MODIFIERS.shift) ||
					matchesKittySequence(data, CODEPOINTS.kpEnter, MODIFIERS.shift)
				) {
					return true;
				}
				// xterm modifyOtherKeys format (fallback when Kitty protocol not enabled)
				if (matchesModifyOtherKeys(data, CODEPOINTS.enter, MODIFIERS.shift)) {
					return true;
				}
				// When Kitty protocol is active, legacy sequences are custom terminal mappings
				// \x1b\r = Kitty's "map shift+enter send_text all \e\r"
				// \n = Ghostty's "keybind = shift+enter=text:\n"
				if (kittyProtocolActive) {
					return data === "\x1b\r" || data === "\n";
				}
				return false;
			}
			if (alt && !ctrl && !shift) {
				// CSI u sequences (standard Kitty protocol)
				if (
					matchesKittySequence(data, CODEPOINTS.enter, MODIFIERS.alt) ||
					matchesKittySequence(data, CODEPOINTS.kpEnter, MODIFIERS.alt)
				) {
					return true;
				}
				// xterm modifyOtherKeys format (fallback when Kitty protocol not enabled)
				if (matchesModifyOtherKeys(data, CODEPOINTS.enter, MODIFIERS.alt)) {
					return true;
				}
				// \x1b\r is alt+enter only in legacy mode (no Kitty protocol)
				// When Kitty protocol is active, alt+enter comes as CSI u sequence
				if (!kittyProtocolActive) {
					return data === "\x1b\r";
				}
				return false;
			}
			if (modifier === 0) {
				return (
					data === "\r" ||
					(!kittyProtocolActive && data === "\n") ||
					data === "\x1bOM" || // SS3 M (numpad enter in some terminals)
					matchesKittySequence(data, CODEPOINTS.enter, 0) ||
					matchesKittySequence(data, CODEPOINTS.kpEnter, 0)
				);
			}
			return (
				matchesKittySequence(data, CODEPOINTS.enter, modifier) ||
				matchesKittySequence(data, CODEPOINTS.kpEnter, modifier)
			);

		case "backspace":
			if (alt && !ctrl && !shift) {
				if (data === "\x1b\x7f" || data === "\x1b\b") {
					return true;
				}
				return matchesKittySequence(data, CODEPOINTS.backspace, MODIFIERS.alt);
			}
			if (modifier === 0) {
				return data === "\x7f" || data === "\x08" || matchesKittySequence(data, CODEPOINTS.backspace, 0);
			}
			return matchesKittySequence(data, CODEPOINTS.backspace, modifier);

		case "insert":
			if (modifier === 0) {
				return (
					matchesLegacySequence(data, LEGACY_KEY_SEQUENCES.insert) ||
					matchesKittySequence(data, FUNCTIONAL_CODEPOINTS.insert, 0)
				);
			}
			if (matchesLegacyModifierSequence(data, "insert", modifier)) {
				return true;
			}
			return matchesKittySequence(data, FUNCTIONAL_CODEPOINTS.insert, modifier);

		case "delete":
			if (modifier === 0) {
				return (
					matchesLegacySequence(data, LEGACY_KEY_SEQUENCES.delete) ||
					matchesKittySequence(data, FUNCTIONAL_CODEPOINTS.delete, 0)
				);
			}
			if (matchesLegacyModifierSequence(data, "delete", modifier)) {
				return true;
			}
			return matchesKittySequence(data, FUNCTIONAL_CODEPOINTS.delete, modifier);

		case "clear":
			if (modifier === 0) {
				return matchesLegacySequence(data, LEGACY_KEY_SEQUENCES.clear);
			}
			return matchesLegacyModifierSequence(data, "clear", modifier);

		case "home":
			if (modifier === 0) {
				return (
					matchesLegacySequence(data, LEGACY_KEY_SEQUENCES.home) ||
					matchesKittySequence(data, FUNCTIONAL_CODEPOINTS.home, 0)
				);
			}
			if (matchesLegacyModifierSequence(data, "home", modifier)) {
				return true;
			}
			return matchesKittySequence(data, FUNCTIONAL_CODEPOINTS.home, modifier);

		case "end":
			if (modifier === 0) {
				return (
					matchesLegacySequence(data, LEGACY_KEY_SEQUENCES.end) ||
					matchesKittySequence(data, FUNCTIONAL_CODEPOINTS.end, 0)
				);
			}
			if (matchesLegacyModifierSequence(data, "end", modifier)) {
				return true;
			}
			return matchesKittySequence(data, FUNCTIONAL_CODEPOINTS.end, modifier);

		case "pageup":
			if (modifier === 0) {
				return (
					matchesLegacySequence(data, LEGACY_KEY_SEQUENCES.pageUp) ||
					matchesKittySequence(data, FUNCTIONAL_CODEPOINTS.pageUp, 0)
				);
			}
			if (matchesLegacyModifierSequence(data, "pageUp", modifier)) {
				return true;
			}
			return matchesKittySequence(data, FUNCTIONAL_CODEPOINTS.pageUp, modifier);

		case "pagedown":
			if (modifier === 0) {
				return (
					matchesLegacySequence(data, LEGACY_KEY_SEQUENCES.pageDown) ||
					matchesKittySequence(data, FUNCTIONAL_CODEPOINTS.pageDown, 0)
				);
			}
			if (matchesLegacyModifierSequence(data, "pageDown", modifier)) {
				return true;
			}
			return matchesKittySequence(data, FUNCTIONAL_CODEPOINTS.pageDown, modifier);

		case "up":
			if (alt && !ctrl && !shift) {
				return data === "\x1bp" || matchesKittySequence(data, ARROW_CODEPOINTS.up, MODIFIERS.alt);
			}
			if (modifier === 0) {
				return (
					matchesLegacySequence(data, LEGACY_KEY_SEQUENCES.up) ||
					matchesKittySequence(data, ARROW_CODEPOINTS.up, 0)
				);
			}
			if (matchesLegacyModifierSequence(data, "up", modifier)) {
				return true;
			}
			return matchesKittySequence(data, ARROW_CODEPOINTS.up, modifier);

		case "down":
			if (alt && !ctrl && !shift) {
				return data === "\x1bn" || matchesKittySequence(data, ARROW_CODEPOINTS.down, MODIFIERS.alt);
			}
			if (modifier === 0) {
				return (
					matchesLegacySequence(data, LEGACY_KEY_SEQUENCES.down) ||
					matchesKittySequence(data, ARROW_CODEPOINTS.down, 0)
				);
			}
			if (matchesLegacyModifierSequence(data, "down", modifier)) {
				return true;
			}
			return matchesKittySequence(data, ARROW_CODEPOINTS.down, modifier);

		case "left":
			if (alt && !ctrl && !shift) {
				return (
					data === "\x1b[1;3D" ||
					(!kittyProtocolActive && data === "\x1bB") ||
					data === "\x1bb" ||
					matchesKittySequence(data, ARROW_CODEPOINTS.left, MODIFIERS.alt)
				);
			}
			if (ctrl && !alt && !shift) {
				return (
					data === "\x1b[1;5D" ||
					matchesLegacyModifierSequence(data, "left", MODIFIERS.ctrl) ||
					matchesKittySequence(data, ARROW_CODEPOINTS.left, MODIFIERS.ctrl)
				);
			}
			if (modifier === 0) {
				return (
					matchesLegacySequence(data, LEGACY_KEY_SEQUENCES.left) ||
					matchesKittySequence(data, ARROW_CODEPOINTS.left, 0)
				);
			}
			if (matchesLegacyModifierSequence(data, "left", modifier)) {
				return true;
			}
			return matchesKittySequence(data, ARROW_CODEPOINTS.left, modifier);

		case "right":
			if (alt && !ctrl && !shift) {
				return (
					data === "\x1b[1;3C" ||
					(!kittyProtocolActive && data === "\x1bF") ||
					data === "\x1bf" ||
					matchesKittySequence(data, ARROW_CODEPOINTS.right, MODIFIERS.alt)
				);
			}
			if (ctrl && !alt && !shift) {
				return (
					data === "\x1b[1;5C" ||
					matchesLegacyModifierSequence(data, "right", MODIFIERS.ctrl) ||
					matchesKittySequence(data, ARROW_CODEPOINTS.right, MODIFIERS.ctrl)
				);
			}
			if (modifier === 0) {
				return (
					matchesLegacySequence(data, LEGACY_KEY_SEQUENCES.right) ||
					matchesKittySequence(data, ARROW_CODEPOINTS.right, 0)
				);
			}
			if (matchesLegacyModifierSequence(data, "right", modifier)) {
				return true;
			}
			return matchesKittySequence(data, ARROW_CODEPOINTS.right, modifier);

		case "f1":
		case "f2":
		case "f3":
		case "f4":
		case "f5":
		case "f6":
		case "f7":
		case "f8":
		case "f9":
		case "f10":
		case "f11":
		case "f12": {
			if (modifier !== 0) {
				return false;
			}
			const functionKey = key as keyof typeof LEGACY_KEY_SEQUENCES;
			return matchesLegacySequence(data, LEGACY_KEY_SEQUENCES[functionKey]);
		}
	}

	// Handle single letter keys (a-z) and some symbols
	if (key.length === 1 && ((key >= "a" && key <= "z") || SYMBOL_KEYS.has(key))) {
		const codepoint = key.charCodeAt(0);
		const rawCtrl = rawCtrlChar(key);

		if (ctrl && alt && !shift && !kittyProtocolActive && rawCtrl) {
			// Legacy: ctrl+alt+key is ESC followed by the control character
			return data === `\x1b${rawCtrl}`;
		}

		if (alt && !ctrl && !shift && !kittyProtocolActive && key >= "a" && key <= "z") {
			// Legacy: alt+letter is ESC followed by the letter
			if (data === `\x1b${key}`) return true;
		}

		if (ctrl && !shift && !alt) {
			// Legacy: ctrl+key sends the control character
			if (rawCtrl && data === rawCtrl) return true;
			return matchesKittySequence(data, codepoint, MODIFIERS.ctrl);
		}

		if (ctrl && shift && !alt) {
			return matchesKittySequence(data, codepoint, MODIFIERS.shift + MODIFIERS.ctrl);
		}

		if (shift && !ctrl && !alt) {
			// Legacy: shift+letter produces uppercase
			if (data === key.toUpperCase()) return true;
			return matchesKittySequence(data, codepoint, MODIFIERS.shift);
		}

		if (modifier !== 0) {
			return matchesKittySequence(data, codepoint, modifier);
		}

		// Check both raw char and Kitty sequence (needed for release events)
		return data === key || matchesKittySequence(data, codepoint, 0);
	}

	return false;
}



/**
 * Parses raw input data into a KeyId string.
 * 
 * Attempts to identify the logical key from raw terminal input by:
 * 1. First trying Kitty protocol parsing if active
 * 2. Falling back to legacy sequence recognition
 * 3. Handling control characters and Alt+key combinations
 * 4. Processing single character input
 * 
 * This function is the inverse of matchesKey - it converts raw input
 * into the standardized KeyId format used throughout the application.
 * 
 * @param data - Raw input data from terminal
 * @returns KeyId string or undefined if not recognized
 * 
 * @example
 * ```typescript
 * const keyId = parseKey('\x1b[A'); // Returns 'up'
 * const keyId = parseKey('\x03');   // Returns 'ctrl+c'
 * const keyId = parseKey('a');      // Returns 'a'
 * ```
 */
export function parseKey(data: string): KeyId | undefined {
	// Try Kitty protocol first
	if (kittyProtocolActive) {
		const parsed = parseKittySequence(data);
		if (parsed) {
			const { codepoint, baseLayoutKey, modifier } = parsed;
			const mods: string[] = [];
			const effectiveMod = modifier & ~LOCK_MASK;
			if (effectiveMod & MODIFIERS.shift) mods.push("shift");
			if (effectiveMod & MODIFIERS.ctrl) mods.push("ctrl");
			if (effectiveMod & MODIFIERS.alt) mods.push("alt");

			// Use base layout key only when codepoint is not a recognized Latin
			// letter (a-z) or symbol (/, -, [, ;, etc.). For those, the codepoint
			// is authoritative regardless of physical key position.
			const isLatinLetter = codepoint >= 97 && codepoint <= 122; // a-z
			const isKnownSymbol = SYMBOL_KEYS.has(String.fromCharCode(codepoint));
			const effectiveCodepoint = isLatinLetter || isKnownSymbol ? codepoint : (baseLayoutKey ?? codepoint);

			let keyName: string | undefined;
			if (effectiveCodepoint === CODEPOINTS.escape) keyName = "escape";
			else if (effectiveCodepoint === CODEPOINTS.tab) keyName = "tab";
			else if (effectiveCodepoint === CODEPOINTS.enter || effectiveCodepoint === CODEPOINTS.kpEnter) keyName = "enter";
			else if (effectiveCodepoint === CODEPOINTS.space) keyName = "space";
			else if (effectiveCodepoint === CODEPOINTS.backspace) keyName = "backspace";
			else if (effectiveCodepoint === FUNCTIONAL_CODEPOINTS.delete) keyName = "delete";
			else if (effectiveCodepoint === FUNCTIONAL_CODEPOINTS.insert) keyName = "insert";
			else if (effectiveCodepoint === FUNCTIONAL_CODEPOINTS.home) keyName = "home";
			else if (effectiveCodepoint === FUNCTIONAL_CODEPOINTS.end) keyName = "end";
			else if (effectiveCodepoint === FUNCTIONAL_CODEPOINTS.pageUp) keyName = "pageUp";
			else if (effectiveCodepoint === FUNCTIONAL_CODEPOINTS.pageDown) keyName = "pageDown";
			else if (effectiveCodepoint === ARROW_CODEPOINTS.up) keyName = "up";
			else if (effectiveCodepoint === ARROW_CODEPOINTS.down) keyName = "down";
			else if (effectiveCodepoint === ARROW_CODEPOINTS.left) keyName = "left";
			else if (effectiveCodepoint === ARROW_CODEPOINTS.right) keyName = "right";
			else if (effectiveCodepoint >= 97 && effectiveCodepoint <= 122) keyName = String.fromCharCode(effectiveCodepoint);
			else if (SYMBOL_KEYS.has(String.fromCharCode(effectiveCodepoint)))
				keyName = String.fromCharCode(effectiveCodepoint);

			if (keyName) {
				return (mods.length > 0 ? `${mods.join("+")}+${keyName}` : keyName) as KeyId;
			}
		}
	}

	// Mode-aware legacy sequences
	if (kittyProtocolActive) {
		if (data === "\x1b\r" || data === "\n") return "shift+enter";
	}

	// Try common legacy sequences
	if (data === "\x1b") return "escape";
	if (data === "\r") return "enter";
	if (data === "\t") return "tab";
	if (data === " ") return "space";
	if (data === "\x7f" || data === "\x08") return "backspace";
	if (data === "\x1b[Z") return "shift+tab";

	// Arrow keys
	if (data === "\x1b[A" || data === "\x1bOA") return "up";
	if (data === "\x1b[B" || data === "\x1bOB") return "down";
	if (data === "\x1b[C" || data === "\x1bOC") return "right";
	if (data === "\x1b[D" || data === "\x1bOD") return "left";

	// Function keys
	if (data === "\x1bOP") return "f1";
	if (data === "\x1bOQ") return "f2";
	if (data === "\x1bOR") return "f3";
	if (data === "\x1bOS") return "f4";

	// Control characters (Ctrl+a through Ctrl+z)
	if (data.length === 1) {
		const code = data.charCodeAt(0);
		if (code >= 1 && code <= 26) {
			return `ctrl+${String.fromCharCode(code + 96)}` as KeyId; // Convert to a-z
		}
		// Single character (letter or symbol)
		return data as KeyId;
	}

	// Alt+key sequences (ESC + key)
	if (data.length === 2 && data[0] === '\x1b') {
		return `alt+${data[1]}` as KeyId;
	}

	return undefined;
}