import { useEffect, useRef, useCallback } from 'react';
import React from 'react';
import { useTwinkiContext } from './context.js';
import { reconciler, renderTree } from '../reconciler/host-config.js';
import type { RootContainer } from '../reconciler/types.js';
import { createYogaNode, Yoga } from '../layout/yoga.js';
import type { Component } from '../renderer/component.js';
import type { OverlayOptions, OverlayHandle } from '../renderer/component.js';
import { TwinkiCtx } from './context.js';

/**
 * A minimal Component that renders a React element tree via the reconciler.
 * Used internally by useOverlay to bridge React content into tui.showOverlay.
 */
class OverlayBridge implements Component {
	private container: RootContainer;
	private dirty = true;
	private cachedLines: string[] = [];
	private reconcilerContainer: ReturnType<typeof reconciler.createContainer>;

	constructor(element: React.ReactElement, ctxValue: { tui: any; exit: any }, onRender: () => void) {
		const yogaNode = createYogaNode();
		yogaNode.setFlexDirection(Yoga.FLEX_DIRECTION_COLUMN);
		this.container = {
			yogaNode,
			children: [],
			onRender: () => { this.dirty = true; onRender(); },
		};
		this.reconcilerContainer = reconciler.createContainer(
			this.container, 0, null, false, null, 'twinki-overlay',
			(e: Error) => console.error(e), null,
		);
		const wrapped = React.createElement(TwinkiCtx.Provider, { value: ctxValue }, element);
		reconciler.updateContainer(wrapped, this.reconcilerContainer, null, () => {});
	}

	update(element: React.ReactElement, ctxValue: { tui: any; exit: any }) {
		const wrapped = React.createElement(TwinkiCtx.Provider, { value: ctxValue }, element);
		reconciler.updateContainer(wrapped, this.reconcilerContainer, null, () => {});
	}

	render(width: number): string[] {
		if (this.dirty) {
			const result = renderTree(this.container, width);
			this.cachedLines = result.liveLines;
			this.dirty = false;
		}
		return this.cachedLines;
	}

	invalidate(): void { this.dirty = true; }

	destroy() {
		reconciler.updateContainer(null, this.reconcilerContainer, null, () => {});
	}
}

/**
 * Hook for showing a floating overlay over the current TUI content.
 *
 * Returns a `show` function. Call it to display the overlay; it returns
 * an `OverlayHandle` with `.hide()` to dismiss it.
 *
 * @param factory - Function returning the React element to render in the overlay.
 *                  Called each time `show()` is invoked.
 * @param options - Positioning options (anchor, row, col, width, etc.)
 * @returns `show` function that displays the overlay and returns a handle.
 *
 * @example
 * ```tsx
 * function App() {
 *   const showOverlay = useOverlay(
 *     () => <Box borderStyle="round"><Text>Hello!</Text></Box>,
 *     { anchor: 'center' }
 *   );
 *
 *   return <Box onClick={() => showOverlay()}>Click me</Box>;
 * }
 * ```
 */
export function useOverlay(
	factory: () => React.ReactElement,
	options?: OverlayOptions,
): () => OverlayHandle {
	const { tui, exit } = useTwinkiContext();
	const bridgeRef = useRef<OverlayBridge | null>(null);
	const handleRef = useRef<OverlayHandle | null>(null);
	const ctxValue = { tui, exit };

	// Cleanup on unmount
	useEffect(() => {
		return () => {
			handleRef.current?.hide();
			bridgeRef.current?.destroy();
		};
	}, []);

	return useCallback(() => {
		// Dismiss any existing overlay from this hook
		handleRef.current?.hide();
		bridgeRef.current?.destroy();

		const element = factory();
		const bridge = new OverlayBridge(element, ctxValue, () => tui.requestRender());
		bridgeRef.current = bridge;

		const handle = tui.showOverlay(bridge, options);
		handleRef.current = handle;
		return handle;
	}, [tui, options]);
}
