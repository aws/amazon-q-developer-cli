import { useEffect, useRef } from 'react';
import { useTwinkiContext } from './context.js';
import { parseKey } from '../input/keys.js';

/**
 * Represents the state of special keys and modifiers for keyboard input.
 */
export interface Key {
	/** Whether the up arrow key is pressed */
	upArrow: boolean;
	/** Whether the down arrow key is pressed */
	downArrow: boolean;
	/** Whether the left arrow key is pressed */
	leftArrow: boolean;
	/** Whether the right arrow key is pressed */
	rightArrow: boolean;
	/** Whether the return/enter key is pressed */
	return: boolean;
	/** Whether the escape key is pressed */
	escape: boolean;
	/** Whether the ctrl modifier is active */
	ctrl: boolean;
	/** Whether the shift modifier is active */
	shift: boolean;
	/** Whether the alt modifier is active */
	alt: boolean;
	/** Whether the tab key is pressed */
	tab: boolean;
	/** Whether the backspace key is pressed */
	backspace: boolean;
	/** Whether the delete key is pressed */
	delete: boolean;
	/** Whether the page up key is pressed */
	pageUp: boolean;
	/** Whether the page down key is pressed */
	pageDown: boolean;
	/** Whether the home key is pressed */
	home: boolean;
	/** Whether the end key is pressed */
	end: boolean;
	/** Whether the meta/cmd modifier is active */
	meta: boolean;
}

/**
 * Options for configuring the useInput hook.
 */
export interface UseInputOptions {
	/** Whether input handling is active. Default: true */
	isActive?: boolean;
}

/**
 * Hook for handling keyboard input in Twinki applications.
 * 
 * The useInput hook provides a way to listen for keyboard input events
 * and handle them with a custom handler function. It automatically parses
 * raw input data into readable characters and key states.
 * 
 * The hook handles both printable characters and special keys, providing
 * detailed information about modifier keys and navigation keys.
 * 
 * @param handler - Function called when input is received
 * @param options - Configuration options for the hook
 * @param options.isActive - Whether input handling is active (default: true)
 * 
 * @example
 * ```tsx
 * const [text, setText] = useState('');
 * 
 * useInput((input, key) => {
 *   if (key.return) {
 *     console.log('Enter pressed');
 *   } else if (key.backspace) {
 *     setText(prev => prev.slice(0, -1));
 *   } else if (input) {
 *     setText(prev => prev + input);
 *   }
 * });
 * ```
 */
export function useInput(
	handler: (input: string, key: Key) => void,
	options: UseInputOptions = {},
): void {
	const { tui } = useTwinkiContext();
	const isActive = options.isActive ?? true;
	const handlerRef = useRef(handler);
	handlerRef.current = handler;

	useEffect(() => {
		if (!isActive) return;

		const unsub = tui.addInputListener((data) => {
			const { input, key } = parseInputData(data);
			handlerRef.current(input, key);
		});

		return unsub;
	}, [tui, isActive]);
}

/** Shared helper: parse raw terminal data into input string + Key object */
export function parseInputData(data: string): { input: string; key: Key } {
	const parsed = parseKey(data);
	const keyId = parsed ?? '';
	const key: Key = {
		upArrow: keyId === 'up' || keyId.endsWith('+up'),
		downArrow: keyId === 'down' || keyId.endsWith('+down'),
		leftArrow: keyId === 'left' || keyId.endsWith('+left'),
		rightArrow: keyId === 'right' || keyId.endsWith('+right'),
		return: keyId === 'enter' || keyId.endsWith('+enter'),
		escape: keyId === 'escape',
		ctrl: keyId.includes('ctrl+'),
		shift: keyId.includes('shift+'),
		alt: keyId.includes('alt+'),
		tab: keyId === 'tab' || keyId.endsWith('+tab'),
		backspace: keyId === 'backspace' || keyId.endsWith('+backspace'),
		delete: keyId === 'delete' || keyId.endsWith('+delete'),
		pageUp: keyId === 'pageUp' || keyId.endsWith('+pageUp'),
		pageDown: keyId === 'pageDown' || keyId.endsWith('+pageDown'),
		home: keyId === 'home' || keyId.endsWith('+home'),
		end: keyId === 'end' || keyId.endsWith('+end'),
		meta: keyId.includes('alt+'),
	};
	let input = '';
	if (data.length === 1 && data.charCodeAt(0) >= 0x20) {
		input = data;
	} else if (key.ctrl && data.length === 1) {
		// Ctrl+letter (legacy): expose the letter as input, matching ink's behavior
		const code = data.charCodeAt(0);
		if (code >= 1 && code <= 26) input = String.fromCharCode(code + 96);
	} else if (parsed) {
		// Kitty protocol: CSI u sequences are multi-byte, extract letter from keyId
		const m = parsed.match(/(?:ctrl|alt|shift)\+([a-z])$/);
		if (m) input = m[1]!;
	}
	return { input, key };
}
