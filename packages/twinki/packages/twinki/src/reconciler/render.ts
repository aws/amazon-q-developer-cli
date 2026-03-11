import React from 'react';
import { reconciler, renderTree, hitTest, findAncestorWithProp } from './host-config.js';
import type { RootContainer, TwinkiNode } from './types.js';
import { createYogaNode } from '../layout/yoga.js';
import { Yoga } from '../layout/yoga.js';
import { ProcessTerminal } from '../terminal/process-terminal.js';
import type { Terminal } from '../terminal/terminal.js';
import { TUI } from '../renderer/tui.js';
import type { Component } from '../renderer/component.js';
import { matchesKey } from '../input/keys.js';
import { TwinkiCtx } from '../hooks/context.js';
import { NODE_TYPES, FlexDirection, CONSTANTS } from '../text/constants.js';

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
	/** Returns a promise that resolves when the application exits */
	waitUntilExit(): Promise<void>;
	/** Clears the display and forces a full redraw */
	clear(): void;
	/** Re-renders the application with a new React element */
	rerender(element: React.ReactElement): void;
	/** Returns current render performance metrics */
	getMetrics(): RenderMetrics;
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
	 * Called on resize. Does NOT reset totalStaticWritten — freed Yoga nodes must
	 * stay skipped. The TUI's accumulatedStaticOutput buffer holds the already-rendered
	 * lines and is re-emitted as-is by the resize handler.
	 */
	resetStatic(): void {
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

	const tui = new TUI(terminal, { targetFps: options.targetFps, fullscreen: options.fullscreen, mouse: options.mouse });

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

	const ctxValue = { tui, exit: exitFn };

	function wrap(el: React.ReactElement): React.ReactElement {
		return React.createElement(TwinkiCtx.Provider, { value: ctxValue }, el);
	}

	reconciler.updateContainer(wrap(element), container, null, noop);

	// Ctrl+C handler
	if (exitOnCtrlC) {
		tui.addInputListener((data) => {
			if (matchesKey(data, 'ctrl+c')) {
				instance.unmount();
				return { consume: true };
			}
		});
	}

	tui.start();

	// Mouse hit-testing: dispatch onClick/onMouseEnter/onMouseLeave to components
	if (options.mouse === true) {
	let hoveredNode: TwinkiNode | null = null;
	tui.addMouseListener((event) => {
		const rootContainer = bridge.getContainer();
		const adjustedY = event.y - tui.getContentYOffset();
		if (adjustedY < 0) return;
		const node = hitTest(rootContainer, event.x, adjustedY);

		// onMouseEnter / onMouseLeave
		const enterNode = findAncestorWithProp(node, 'onMouseEnter') ?? findAncestorWithProp(node, 'onMouseLeave');
		if (enterNode !== hoveredNode) {
			if (hoveredNode?.props.onMouseLeave) hoveredNode.props.onMouseLeave();
			hoveredNode = enterNode;
			if (enterNode?.props.onMouseEnter) enterNode.props.onMouseEnter();
		}

		// onClick on mouseup
		if (event.type === 'mouseup' && event.button === 'left') {
			const clickNode = findAncestorWithProp(node, 'onClick');
			if (clickNode && clickNode.props.onClick) clickNode.props.onClick();
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

	const instance: Instance = {
		unmount() {
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
		waitUntilExit() {
			return exitPromise;
		},
		clear() {
			tui.resetStaticOutput();
			bridge.resetStatic();
			tui.requestRender(true);
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
		rerender(newElement: React.ReactElement) {
			reconciler.updateContainer(wrap(newElement), container, null, noop);
		},
	};

	return instance;
}

/**
 * No-operation function used as a callback placeholder.
 */
function noop(): void {}
