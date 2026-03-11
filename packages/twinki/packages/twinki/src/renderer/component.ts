/**
 * Base interface for all renderable components in the TUI system.
 * 
 * Components are the building blocks of the terminal UI, responsible for
 * rendering content and optionally handling input. Each component must
 * implement the render method and invalidation mechanism.
 */
export interface Component {
	/**
	 * Renders the component to an array of strings representing terminal lines.
	 * 
	 * @param width - Available width in terminal columns
	 * @returns Array of strings, each representing a terminal line
	 */
	render(width: number): string[];
	
	/**
	 * Handles input data when the component has focus.
	 * 
	 * @param data - Raw input data from terminal
	 */
	handleInput?(data: string): void;
	
	/**
	 * Marks the component as needing re-render.
	 * 
	 * Called when the component's state changes and it needs to be redrawn.
	 */
	invalidate(): void;
	
	/**
	 * Whether the component wants to receive key release events.
	 * 
	 * Most components only need key press events, but some (like games)
	 * may need release events for proper interaction.
	 */
	wantsKeyRelease?: boolean;
}

/**
 * Interface for components that can receive focus.
 * 
 * Focusable components can receive keyboard input and typically
 * have visual indicators when focused.
 */
export interface Focusable {
	/** Whether the component currently has focus */
	focused: boolean;
}

/**
 * Type guard to check if a component is focusable.
 * 
 * @param component - Component to check
 * @returns Whether the component implements Focusable
 */
export function isFocusable(component: Component | null): component is Component & Focusable {
	return component !== null && 'focused' in component;
}

/**
 * Special marker used to indicate cursor position in rendered output.
 * 
 * This APC sequence is used internally to mark where the cursor should
 * be positioned after rendering. It's stripped from final output.
 */
export const CURSOR_MARKER = '\x1b_twinki:c\x07';

/**
 * Function type for handling input events.
 * 
 * Input listeners can consume input (preventing further processing)
 * or transform it before passing to other handlers.
 */
export type InputListener = (data: string) => { consume?: boolean; data?: string } | void;

/**
 * Anchor positions for overlay positioning.
 * 
 * Determines how overlays are positioned relative to their container
 * or the terminal viewport.
 */
export type OverlayAnchor =
	| 'top-left' | 'top-center' | 'top-right'
	| 'left-center' | 'center' | 'right-center'
	| 'bottom-left' | 'bottom-center' | 'bottom-right';

/**
 * Size value that can be absolute pixels or percentage.
 */
export type SizeValue = number | `${number}%`;

/**
 * Margin specification for overlays.
 * 
 * Allows fine-grained control over spacing around overlay content.
 */
export interface OverlayMargin {
	/** Top margin in terminal rows */
	top?: number;
	/** Right margin in terminal columns */
	right?: number;
	/** Bottom margin in terminal rows */
	bottom?: number;
	/** Left margin in terminal columns */
	left?: number;
}

/**
 * Configuration options for overlay positioning and sizing.
 * 
 * Overlays are floating components that can be positioned over other content.
 * They support various positioning modes, sizing constraints, and visibility rules.
 */
export interface OverlayOptions {
	/** Width of the overlay */
	width?: SizeValue;
	/** Minimum width constraint */
	minWidth?: number;
	/** Maximum height constraint */
	maxHeight?: SizeValue;
	/** Anchor point for positioning */
	anchor?: OverlayAnchor;
	/** Horizontal offset from anchor */
	offsetX?: number;
	/** Vertical offset from anchor */
	offsetY?: number;
	/** Explicit row position */
	row?: number | `${number}%`;
	/** Explicit column position */
	col?: number | `${number}%`;
	/** Margin around the overlay */
	margin?: number | OverlayMargin;
	/** Function to determine if overlay should be visible */
	visible?: (width: number, height: number) => boolean;
}

/**
 * Handle for controlling overlay visibility and state.
 * 
 * Returned when creating overlays to allow dynamic control
 * of their visibility without recreating them.
 */
export interface OverlayHandle {
	/** Hides the overlay */
	hide(): void;
	/** Sets overlay visibility state */
	setHidden(hidden: boolean): void;
	/** Gets current visibility state */
	isHidden(): boolean;
}

/**
 * Parses a size value (number or percentage) into an absolute value.
 * 
 * Converts percentage values to absolute values based on a reference size.
 * Used for responsive sizing in overlays and layout calculations.
 * 
 * @param value - Size value to parse
 * @param referenceSize - Reference size for percentage calculations
 * @returns Absolute size value or undefined if value is undefined
 * 
 * @example
 * ```typescript
 * parseSizeValue(50, 100);     // 50
 * parseSizeValue('50%', 100);  // 50
 * parseSizeValue('25%', 80);   // 20
 * ```
 */
export function parseSizeValue(value: SizeValue | undefined, referenceSize: number): number | undefined {
	if (value === undefined) return undefined;
	if (typeof value === 'number') return value;
	const match = value.match(/^(\d+(?:\.\d+)?)%$/);
	if (match) return Math.floor((parseFloat(match[1]!) / 100) * referenceSize);
	return undefined;
}

/**
 * Basic container component that renders child components sequentially.
 * 
 * The Container class provides a simple way to group multiple components
 * together. It renders all children in order and forwards invalidation
 * calls to all children.
 * 
 * This is useful for creating composite components or managing groups
 * of related UI elements.
 */
export class Container implements Component {
	protected children: Component[] = [];

	/**
	 * Adds a child component to the container.
	 * 
	 * @param child - Component to add
	 */
	addChild(child: Component): void {
		this.children.push(child);
	}

	/**
	 * Removes a child component from the container.
	 * 
	 * @param child - Component to remove
	 */
	removeChild(child: Component): void {
		const index = this.children.indexOf(child);
		if (index !== -1) {
			this.children.splice(index, 1);
		}
	}

	/**
	 * Removes all child components from the container.
	 */
	clear(): void {
		this.children = [];
	}

	/**
	 * Renders all child components sequentially.
	 * 
	 * @param width - Available width in terminal columns
	 * @returns Combined output from all child components
	 */
	render(width: number): string[] {
		const lines: string[] = [];
		for (const child of this.children) {
			lines.push(...child.render(width));
		}
		return lines;
	}

	/**
	 * Invalidates all child components.
	 * 
	 * Forwards the invalidation call to all children, ensuring
	 * they will be re-rendered on the next frame.
	 */
	invalidate(): void {
		for (const child of this.children) {
			child.invalidate();
		}
	}
}
