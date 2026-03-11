import React, { useEffect, useRef, useState } from 'react';
import { useTwinkiContext } from '../hooks/context.js';
import { Input } from './Input.js';

export interface TextInputProps {
	/** Current value (controlled) */
	value?: string;
	/** Placeholder text shown when empty and unfocused */
	placeholder?: string;
	/** Called when user presses Enter */
	onSubmit?: (value: string) => void;
	/** Called when user presses Escape */
	onEscape?: () => void;
	/** Called when value changes */
	onChange?: (value: string) => void;
	/** Whether input is active (default: true) */
	isActive?: boolean;
}

/**
 * React wrapper for the Input component.
 * Single-line text input with horizontal scrolling, undo/redo, kill ring.
 */
export const TextInput: React.FC<TextInputProps> = ({
	value,
	placeholder,
	onSubmit,
	onEscape,
	onChange,
	isActive = true,
}) => {
	const { tui } = useTwinkiContext();
	const inputRef = useRef<Input>(null!);
	const [renderedLine, setRenderedLine] = useState('');

	// Create Input instance once
	if (!inputRef.current) {
		inputRef.current = new Input();
	}

	const input = inputRef.current;

	// Sync props
	useEffect(() => {
		input.onSubmit = onSubmit;
		input.onEscape = onEscape;
		input.onChange = (val: string) => {
			onChange?.(val);
			rerender();
		};
		input.placeholder = placeholder;
	});

	// Sync controlled value
	useEffect(() => {
		if (value !== undefined && value !== input.getValue()) {
			input.setValue(value);
			rerender();
		}
	}, [value]);

	// Focus and input handling
	useEffect(() => {
		if (!isActive) {
			input.focused = false;
			rerender();
			return;
		}

		input.focused = true;
		rerender();

		const unsub = tui.addInputListener((data) => {
			input.handleInput(data);
			rerender();
		});

		return () => {
			unsub();
			input.focused = false;
		};
	}, [tui, isActive]);

	function rerender() {
		const width = tui.terminal.columns;
		const lines = input.render(width);
		setRenderedLine(lines[0] || '');
	}

	return React.createElement('twinki-text', { wrap: 'truncate' }, renderedLine);
};

TextInput.displayName = 'TextInput';
