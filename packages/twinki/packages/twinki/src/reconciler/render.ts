import React from 'react';
import { reconciler, renderTree, hitTest, findAncestorWithProp } from './host-config.js';
import type { RootContainer, TwinkiNode } from './types.js';
import { createYogaNode } from '../layout/yoga.js';
import { Yoga } from '../layout/yoga.js';
import { ProcessTerminal } from '../terminal/process-terminal.js';
import type { Terminal } from '../terminal/terminal.js';
import { TUI } from '../renderer/tui.js';
import type { RenderCompletedEvent } from '../renderer/tui.js';
import type { Component } from '../renderer/component.js';
import type {
	ComponentMouseEvent,
	MouseEvent,
	MouseTargetBounds,
} from '../input/mouse.js';
import { matchesKey } from '../input/keys.js';
import { TwinkiCtx } from '../hooks/context.js';
import { NODE_TYPES, FlexDirection, CONSTANTS } from '../text/constants.js';

function absoluteBounds(node: TwinkiNode): MouseTargetBounds | null {
	let x = 0;
	let y = 0;
	let current: TwinkiNode | null = node;

	while (current) {
		if (!current.yogaNode) return null;
		x += Math.floor(current.yogaNode.getComputedLeft());
		y += Math.floor(current.yogaNode.getComputedTop());
		current = current.parent;
	}

	return {
		x,
		y,
		width: Math.floor(node.yogaNode!.getComputedWidth()),
		height: Math.floor(node.yogaNode!.getComputedHeight()),
	};
}

function localizeMouseEvent(
	event: MouseEvent,
	node: TwinkiNode,
	contentYOffset: number,
): ComponentMouseEvent | null {
	const bounds = absoluteBounds(node);
	if (!bounds) return null;
	const targetBounds = { ...bounds, y: bounds.y + contentYOffset };
	return {
		...event,
		localX: event.x - targetBounds.x,
		localY: event.y - targetBounds.y,
		targetBounds,
	};
}

/**
 * Configuration options for rendering a Twinki application.
 */
export interface TwinkiRenderOptions {
	/** Output stream (default: process.stdout) */
	stdout?: NodeJS.WriteStream;
	/** Input stream (default: process.stdin) */
	stdin?: NodeJS.ReadStream;
	/** Error stream (default: process.stderr) */
	stderr?: NodeJS.WriteStream;
	/** Enable debug mode */
	debug?: boolean;
	/** Exit application on Ctrl+C (default: true) */
	exitOnCtrlC?: boolean;
	/** Patch console methods for TUI compatibility */
	patchConsole?: boolean;
	/** Custom terminal implementation */
	terminal?: Terminal;
	/** Max renders per second. 0 = unlimited (default). */
	targetFps?: number;
	/** Enter alternate screen buffer (preserves scrollback). */
	fullscreen?: boolean;
	/** Enable mouse event tracking (default: false) */
	mouse?: boolean;
	/**
	 * Enable mouse text selection with automatic clipboard copy.
	 * Implies mouse tracking. Default: false.
	 */
	textSelection?: boolean;
	/** Max lines to keep in static scrollback buffer (default: 10_000). */
	staticScrollbackCap?: number;
	/**
	 * Enable support for lines wider than terminal width (soft-wrapped by
	 * the terminal). Required when any component uses `wrap="overflow"`.
	 * Adds a small per-render cost (O(n) width check) and tracks physical
	 * rows for cursor/viewport math. Default: false.
	 */
	wideLines?: boolean;
	/**
	 * Repaint only the viewport on native-scrollback full redraws instead of
	 * clearing scrollback (default: false). Only safe when no element spans
	 * rows above the viewport — a full-height gutter or border renders with
	 * gaps, because rows above are left as committed history.
	 */
	preserveScrollbackOnRedraw?: boolean;
	synchronizedOutput?: boolean;
}

/**
 * Handle for controlling a rendered Twinki application instance.
 * 
 * Provides methods to manage the lifecycle of a running TUI application,
 * including unmounting, waiting for exit, clearing the display, and re-rendering.
 */
export interface RenderMetrics {
	lastRenderMs: number;
	totalRenderMs: number;
	maxRenderMs: number;
	renderCount: number;
	fullRedrawCount: number;
	/** Total live Yoga nodes in the layout tree (DOM size equivalent). */
	yogaNodeCount: number;
	/** Lines currently held in the static scrollback buffer. Capped at 10,000. */
	staticBufferLines: number;
	/** JS heap used in MB. */
	heapUsedMB: number;
	/** Process RSS in MB (actual physical memory). */
	rssMB: number;
}

export interface Instance {
	/** Unmounts the application and cleans up resources */
	unmount(): void;
	/** Drains stdin to prevent buffered escape sequences from leaking to the parent shell */
	drainInput(maxMs?: number, idleMs?: number): Promise<void>;
	/** Suspends enhanced keyboard reporting (Kitty protocol / modifyOtherKeys) before the process is backgrounded via Ctrl+Z. */
	suspendKeyboard(): void;
	/** Restores enhanced keyboard reporting after the process resumes (SIGCONT). */
	resumeKeyboard(): void;
	/** Synchronously restores legacy keyboard reporting; idempotent, safe from a process 'exit' handler. */
	resetKeyboardModes(): void;
	/** Returns a promise that resolves when the application exits */
	waitUntilExit(): Promise<void>;
	/** Clears the display and forces a full redraw */
	clear(): void;
	/** Enables physical-row tracking for soft-wrapped lines. */
	setWideLines(enabled: boolean): void;
	/** Enables viewport-only repaint on native-scrollback full redraws. */
	setPreserveScrollbackOnRedraw(enabled: boolean): void;
	/** Re-renders the application with a new React element */
	rerender(element: React.ReactElement): void;
	/** Adjusts the static write cursor after items are trimmed from the front of the Static array */
	adjustStaticCursor(removedCount: number): void;
	/** Returns current render performance metrics */
	getMetrics(): RenderMetrics;
	onRenderComplete(cb: (event: RenderCompletedEvent) => void): () => void;
	/** Register a callback invoked after each throttled resize (dimensions already updated). */
	onResize(cb: () => void): void;
	/** Enables or disables terminal mouse reporting at runtime. */
	setMouseEnabled(enabled: boolean): void;
	/** Returns whether terminal mouse reporting is currently enabled. */
	isMouseEnabled(): boolean;
}

/**
 * Bridge component that connects React reconciler to the TUI rendering system.
 * 
 * The ReactBridge acts as an adapter between React's virtual DOM and Twinki's
 * component system. It manages the root container for React elements and
 * handles the conversion from React's render tree to terminal output lines.
 * 
 * Key responsibilities:
 * - Maintains the root Yoga layout container
 * - Caches rendered output for performance
 * - Manages static vs live content separation
 * - Triggers re-renders when React state changes
 */
class ReactBridge implements Component {
	private container: RootContainer;
	private dirty = true;
	private cachedLines: string[] = [];
	private pendingStaticLines: string[] = [];
	private totalStaticWritten = 0; // monotonically increasing write cursor
	private tui: TUI | null = null;
	private staticReset = false;

	/**
	 * Creates a new ReactBridge instance.
	 * 
	 * @param onRender - Callback to trigger when re-render is needed
	 */
	constructor(onRender: () => void) {
		const yogaNode = createYogaNode();
		yogaNode.setFlexDirection(Yoga.FLEX_DIRECTION_COLUMN);
		this.container = {
			yogaNode,
			children: [],
			onRender: () => {
				this.dirty = true;
				onRender();
			},
		};
	}

	/**
	 * Sets the TUI instance for static line management.
	 * 
	 * @param tui - TUI instance to receive static lines
	 */
	setTUI(tui: TUI): void {
		this.tui = tui;
	}

	/**
	 * Gets the root container for React reconciler.
	 * 
	 * @returns Root container instance
	 */
	getContainer(): RootContainer {
		return this.container;
	}

	/**
	 * Renders the React tree to terminal lines.
	 * 
	 * Converts the React component tree to an array of terminal lines,
	 * handling both static content (for scrollback) and live content
	 * (for the interactive area). Uses caching to avoid unnecessary
	 * re-computation when content hasn't changed.
	 * 
	 * @param width - Available width in terminal columns
	 * @returns Array of terminal lines
	 */
	render(width: number): string[] {
		if (this.dirty) {
			const result = renderTree(this.container, width, this.totalStaticWritten);
			if (result.staticLines.length > 0) {
				if (this.tui) {
					if (this.staticReset) {
						this.tui.replaceStaticOutput(result.staticLines);
						this.staticReset = false;
					} else {
						this.tui.writeStaticLines(result.staticLines);
					}
				}
			}
			const findStatic = (node: TwinkiNode | RootContainer): TwinkiNode | null => {
				for (const child of node.children || []) {
					if (child.type === NODE_TYPES.TWINKI_STATIC) return child;
					const found = findStatic(child);
					if (found) return found;
				}
				return null;
			};
			const staticNode = findStatic(this.container);
			if (staticNode) {
				// Monotonically increase: never go backwards even if items are removed from front.
				// This prevents re-writing already-flushed items to scrollback when the app
				// truncates old items from the Static array.
				this.totalStaticWritten = Math.max(this.totalStaticWritten, staticNode.children.length);
			}
			this.cachedLines = result.liveLines;
			this.dirty = false;
		}
		return this.cachedLines;
	}

	/**
	 * Marks the bridge as needing re-render.
	 * 
	 * Called when React state changes or when forced invalidation is needed.
	 */
	invalidate(): void {
		this.dirty = true;
	}

	/**
	 * Adjusts the static write cursor after the app trims items from the
	 * front of the `<Static>` items array.  Without this, the monotonic
	 * cursor would be ahead of the new array length and new items would
	 * be silently skipped until the array grows back past the old cursor.
	 */
	adjustStaticCursor(removedCount: number): void {
		this.totalStaticWritten = Math.max(0, this.totalStaticWritten - removedCount);
	}

	/**
	 * Called on resize. Resets the static write cursor so all static items
	 * are re-rendered at the new width on the next frame. The staticReset
	 * flag tells render() to call replaceStaticOutput instead of append.
	 */
	resetStatic(): void {
		this.staticReset = true;
		this.totalStaticWritten = 0;
		this.dirty = true;
	}

	/** Whether this component wants key release events (always false for ReactBridge) */
	wantsKeyRelease = false;
}

/**
 * Renders a React element as a terminal user interface.
 * 
 * This is the main entry point for Twinki applications. It creates a TUI
 * instance, sets up the React reconciler, and manages the application lifecycle.
 * 
 * The function handles:
 * - Terminal setup and configuration
 * - React reconciler initialization
 * - Input event handling (including Ctrl+C)
 * - Application lifecycle management
 * - Context provider setup for hooks
 * 
 * @param element - React element to render
 * @param options - Configuration options
 * @returns Instance handle for controlling the application
 * 
 * @example
 * ```typescript
 * import React from 'react';
 * import { render, Text } from 'twinki';
 * 
 * const App = () => <Text>Hello, World!</Text>;
 * 
 * const instance = render(<App />);
 * await instance.waitUntilExit();
 * ```
 */
export function render(element: React.ReactElement, options: TwinkiRenderOptions = {}): Instance {
	const exitOnCtrlC = options.exitOnCtrlC ?? true;

	let terminal: Terminal;
	if (options.terminal) {
		terminal = options.terminal;
	} else {
		terminal = new ProcessTerminal();
	}

	const tui = new TUI(terminal, {
		targetFps: options.targetFps,
		fullscreen: options.fullscreen,
		mouse: options.mouse,
		textSelection: options.textSelection,
		staticScrollbackCap: options.staticScrollbackCap,
		wideLines: options.wideLines,
		preserveScrollbackOnRedraw: options.preserveScrollbackOnRedraw,
		synchronizedOutput: options.synchronizedOutput,
	});

	const bridge = new ReactBridge(() => tui.requestRender());
	bridge.setTUI(tui);
	tui.addChild(bridge);
	tui.setFocus(bridge);
	tui.onResize(() => bridge.resetStatic());

	const container = reconciler.createContainer(
		bridge.getContainer(),
		0, // ConcurrentRoot
		null,
		false,
		null,
		'twinki',
		(error: Error) => console.error(error),
		null,
	);

	let exitResolve: ((value?: Error) => void) | null = null;
	const exitPromise = new Promise<void>((resolve, reject) => {
		exitResolve = (error?: Error) => {
			if (error) reject(error);
			else resolve();
		};
	});

	const exitFn = (error?: Error) => {
		instance.unmount();
		if (error && exitResolve) exitResolve(error);
		else if (exitResolve) exitResolve();
	};

	const ctxValue = { tui, exit: exitFn, adjustStaticCursor: (n: number) => bridge.adjustStaticCursor(n) };

	function wrap(el: React.ReactElement): React.ReactElement {
		return React.createElement(TwinkiCtx.Provider, { value: ctxValue }, el);
	}

	if (options.textSelection === true) {
		tui.setTextSelectionScopeResolver((point) => {
			const node = hitTest(
				bridge.getContainer(),
				point.column,
				point.row,
			);
			const scope = findAncestorWithProp(node, 'selectionScope');
			return scope ? absoluteBounds(scope) : null;
		});
	}

	// Ctrl+C handler
	if (exitOnCtrlC) {
		tui.addInputListener((data) => {
			if (matchesKey(data, 'ctrl+c')) {
				instance.unmount();
				return { consume: true };
			}
		});
	}

	// Mouse hit-testing: dispatch onClick/onMouseEnter/onMouseLeave to components
	if (options.mouse === true || options.textSelection === true) {
		let hoveredNode: TwinkiNode | null = null;
		let pressedClickNode: TwinkiNode | null = null;
		tui.addMouseListener((event) => {
			const rootContainer = bridge.getContainer();
			const contentYOffset = tui.getLiveContentYOffset();
			const adjustedY = event.y - contentYOffset;
			if (adjustedY < 0) return;
			const node = hitTest(rootContainer, event.x, adjustedY);

			const enterNode = findAncestorWithProp(node, 'onMouseEnter') ?? findAncestorWithProp(node, 'onMouseLeave');
			if (enterNode !== hoveredNode) {
				if (hoveredNode?.props.onMouseLeave) hoveredNode.props.onMouseLeave();
				hoveredNode = enterNode;
				if (enterNode?.props.onMouseEnter) enterNode.props.onMouseEnter();
			}

			if (event.type === 'mousedown') {
				pressedClickNode = event.button === 'left'
					? findAncestorWithProp(node, 'onClick')
					: null;
				const downNode = findAncestorWithProp(node, 'onMouseDown');
				const localEvent = downNode
					? localizeMouseEvent(event, downNode, contentYOffset)
					: null;
				if (downNode?.props.onMouseDown && localEvent) {
					downNode.props.onMouseDown(localEvent);
				}
			}

			if (event.type === 'mouseup' && event.button === 'left') {
				const pressedNode = pressedClickNode;
				pressedClickNode = null;
				if (tui.isClickSuppressed(event)) return;
				const clickNode = findAncestorWithProp(node, 'onClick');
				if (!pressedNode || clickNode !== pressedNode) return;
				const localEvent = clickNode
					? localizeMouseEvent(event, clickNode, contentYOffset)
					: null;
				if (clickNode?.props.onClick && localEvent) {
					clickNode.props.onClick(localEvent);
				}
			}
		});
	}

	// Patch console methods to route through static lines
	let restoreConsole: (() => void) | null = null;
	if (options.patchConsole) {
		const orig = { log: console.log, warn: console.warn, error: console.error };
		const patch = (stream: 'stdout' | 'stderr') => (...args: unknown[]) => {
			const msg = args.map(a => typeof a === 'string' ? a : String(a)).join(' ');
			tui.writeStaticLines(msg.split('\n'));
			tui.requestRender();
		};
		console.log = patch('stdout');
		console.warn = patch('stderr');
		console.error = patch('stderr');
		restoreConsole = () => { console.log = orig.log; console.warn = orig.warn; console.error = orig.error; };
	}

	let unmounted = false;
	const instance: Instance = {
		unmount() {
			if (unmounted) return;
			unmounted = true;
			restoreConsole?.();
			reconciler.updateContainer(null, container, null, () => {
				// Free root yoga node after React cleanup is complete
				if (bridge.getContainer().yogaNode) {
					bridge.getContainer().yogaNode.free();
				}
			});
			tui.stop();
			if (exitResolve) exitResolve();
		},
		async drainInput(maxMs?: number, idleMs?: number) {
			await tui.terminal.drainInput(maxMs, idleMs);
		},
		suspendKeyboard() {
			tui.terminal.suspendKeyboard?.();
		},
		resumeKeyboard() {
			tui.terminal.resumeKeyboard?.();
		},
		resetKeyboardModes() {
			tui.terminal.resetKeyboardModes?.();
		},
		waitUntilExit() {
			return exitPromise;
		},
		clear() {
			tui.resetStaticOutput();
			bridge.resetStatic();
			tui.requestRender(true);
		},
		setWideLines(enabled: boolean) {
			tui.setWideLinesEnabled(enabled);
		},
		setPreserveScrollbackOnRedraw(enabled: boolean) {
			tui.setPreserveScrollbackOnRedraw(enabled);
		},
		getMetrics() {
			const countNodes = (node: TwinkiNode | RootContainer): number => {
				let n = 'yogaNode' in node && node.yogaNode ? 1 : 0;
				for (const child of node.children) n += countNodes(child);
				return n;
			};
			const mem = process.memoryUsage();
			return {
				lastRenderMs: tui.perfLastRenderMs,
				totalRenderMs: tui.perfTotalRenderMs,
				maxRenderMs: tui.perfMaxRenderMs,
				renderCount: tui.perfRenderCount,
				fullRedrawCount: tui.fullRedraws,
				yogaNodeCount: countNodes(bridge.getContainer()),
				staticBufferLines: tui.staticBufferLines,
				heapUsedMB: Math.round(mem.heapUsed / 1024 / 1024),
				rssMB: Math.round(mem.rss / 1024 / 1024),
			};
		},
		onRenderComplete(cb: (event: RenderCompletedEvent) => void) {
			return tui.onRenderComplete(cb);
		},
		rerender(newElement: React.ReactElement) {
			reconciler.updateContainer(wrap(newElement), container, null, noop);
		},
		adjustStaticCursor(removedCount: number) {
			bridge.adjustStaticCursor(removedCount);
		},
		onResize(cb: () => void) {
			tui.onResize(cb);
		},
		setMouseEnabled(enabled: boolean) {
			if (enabled) tui.enableMouse();
			else tui.disableMouse();
		},
		isMouseEnabled() {
			return tui.isMouseEnabled();
		},
	};

	reconciler.updateContainer(wrap(element), container, null, () => {
		if (!unmounted) tui.start();
	});

	return instance;
}

/**
 * No-operation function used as a callback placeholder.
 */
function noop(): void {}
