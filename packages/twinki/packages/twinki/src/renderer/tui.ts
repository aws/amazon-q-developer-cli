import {
	Container,
	CURSOR_MARKER,
	isFocusable,
	parseSizeValue,
} from './component.js';
import type {
	Component,
	InputListener,
	OverlayAnchor,
	OverlayHandle,
	OverlayOptions,
} from './component.js';
import type { Terminal } from '../terminal/terminal.js';
import { isKeyRelease, isKeyRepeat, matchesKey } from '../input/keys.js';
import { parseSGRMouse, isSGRMouse } from '../input/mouse.js';
import type { MouseEvent } from '../input/mouse.js';
import { visibleWidth } from '../utils/visible-width.js';
import { sliceByColumn, sliceWithWidth } from '../utils/slice.js';
import { extractSegments } from '../utils/extract-segments.js';

/**
 * Internal overlay entry structure for managing overlay stack.
 */
interface OverlayEntry {
	/** The overlay component */
	component: Component;
	/** Positioning and sizing options */
	options?: OverlayOptions;
	/** Component that had focus before this overlay */
	preFocus: Component | null;
	/** Whether the overlay is currently hidden */
	hidden: boolean;
}

/**
 * Options for TUI constructor.
 */
export interface TUIOptions {
	showHardwareCursor?: boolean;
	/** Max renders per second. 0 = unlimited (default). */
	targetFps?: number;
	/** Enter alternate screen buffer on start. */
	fullscreen?: boolean;
	/** Allow mouse tracking to be enabled (default: false). */
	mouse?: boolean;
}

/**
 * Terminal User Interface (TUI) - The core rendering engine.
 * 
 * The TUI class is the heart of Twinki's rendering system, providing:
 * - Differential rendering with 4 distinct strategies
 * - Overlay management and compositing
 * - Focus management and input routing
 * - Performance monitoring and optimization
 * - Static content handling for scrollback
 * - Hardware cursor positioning
 * 
 * The TUI uses a sophisticated rendering pipeline that minimizes terminal
 * writes by only updating changed content. It supports complex layouts
 * with overlays, proper ANSI code handling, and maintains compatibility
 * with various terminal types.
 * 
 * @example
 * ```typescript
 * const tui = new TUI(terminal);
 * tui.addChild(myComponent);
 * tui.start();
 * ```
 */
export class TUI extends Container {
	/** Terminal interface for output and input */
	public terminal: Terminal;
	/** Optional debug callback */
	public onDebug?: () => void;

	// Performance counters (public for testing)
	/** Duration of last render in milliseconds */
	public perfLastRenderMs = 0;
	/** Total time spent rendering in milliseconds */
	public perfTotalRenderMs = 0;
	/** Maximum single render time in milliseconds */
	public perfMaxRenderMs = 0;
	/** Total number of renders performed */
	public perfRenderCount = 0;
	/** Number of lines currently held in the static scrollback buffer. */
	get staticBufferLines(): number { return this.accumulatedStaticOutput.length; }

	private previousLines: string[] = [];
	private previousWidth = 0;
	private focusedComponent: Component | null = null;
	private inputListeners = new Set<InputListener>();
	private renderRequested = false;
	private cursorRow = 0;
	private hardwareCursorRow = 0;
	private inputBuffer = '';
	private cellSizeQueryPending = false;
	private showHardwareCursor = process.env.TWINKI_HARDWARE_CURSOR === '1';
	private clearOnShrink = process.env.TWINKI_CLEAR_ON_SHRINK === '1';
	private maxLinesRendered = 0;
	private previousViewportTop = 0;
	private fullRedrawCount = 0;
	private stopped = false;
	private overlayStack: OverlayEntry[] = [];
	private accumulatedStaticOutput: string[] = [];
	private onResizeCallbacks: (() => void)[] = [];
	/** Original stdout.write before interception; null when not intercepted. */
	private originalStdoutWrite: typeof process.stdout.write | null = null;
	/** True while twinki is writing to the terminal — suppresses external clear detection. */
	private internalWrite = false;
	private debugLogFd: number | null = null;
	private mouseListeners = new Set<(event: MouseEvent) => void>();
	private mouseEnabled = false;
	private mouseAllowed = false;
	private pasteListeners = new Set<(content: string) => void>();
	private keyReleaseListeners = new Set<(data: string) => void>();
	private keyRepeatListeners = new Set<(data: string) => void>();
	private contentStartRow = -1;
	private dsrPending = false;
	private altScreen = false;
	private targetFps = 0;
	private frameBudgetMs = 0;
	private lastRenderTime = 0;
	private pacingTimer: ReturnType<typeof setTimeout> | null = null;

	/** ANSI reset sequence used between overlay segments */
	private static readonly SEGMENT_RESET = '\x1b[0m';
	/** Full clear: wipe scrollback + screen + cursor home */
	private static readonly CLEAR_ALL = '\x1b[3J\x1b[2J\x1b[H';
	/** Screen clear only: preserve scrollback */
	private static readonly CLEAR_SCREEN = '\x1b[2J\x1b[H';

	/**
	 * Creates a new TUI instance.
	 * 
	 * @param terminal - Terminal interface for I/O
	 * @param showHardwareCursor - Whether to show hardware cursor (optional)
	 */
	constructor(terminal: Terminal, showHardwareCursor?: boolean | TUIOptions) {
		super();
		this.terminal = terminal;
		const opts = typeof showHardwareCursor === 'object' ? showHardwareCursor : { showHardwareCursor };
		if (opts.showHardwareCursor !== undefined) {
			this.showHardwareCursor = opts.showHardwareCursor;
		}
		if (opts.targetFps && opts.targetFps > 0) {
			this.targetFps = opts.targetFps;
			this.frameBudgetMs = 1000 / opts.targetFps;
		}
		if (opts.fullscreen) {
			this.altScreen = true;
		}
		if (opts.mouse) {
			this.mouseAllowed = true;
		}
		if (process.env.TWINKI_DEBUG_REDRAW === '1') {
			try {
				const fs = require('fs');
				const os = require('os');
				const path = require('path');
				const dir = path.join(os.homedir(), '.twinki');
				fs.mkdirSync(dir, { recursive: true });
				this.debugLogFd = fs.openSync(path.join(dir, 'debug.log'), 'a');
			} catch { /* ignore */ }
		}
	}

	private debugLog(msg: string): void {
		if (this.debugLogFd == null) return;
		try { require('fs').writeSync(this.debugLogFd, `[${new Date().toISOString()}] ${msg}\n`); } catch { /* ignore */ }
	}

	/**
	 * Gets the number of full redraws performed.
	 * 
	 * Full redraws are expensive operations that clear and redraw the entire
	 * screen. This counter helps monitor rendering efficiency.
	 * 
	 * @returns Number of full redraws
	 */
	get fullRedraws(): number {
		return this.fullRedrawCount;
	}

	/**
	 * Sets whether the hardware cursor should be visible.
	 * 
	 * The hardware cursor is the blinking cursor shown by the terminal.
	 * Most TUI applications hide it, but some components may want it visible.
	 * 
	 * @param enabled - Whether to show the hardware cursor
	 */
	setShowHardwareCursor(enabled: boolean): void {
		if (this.showHardwareCursor === enabled) return;
		this.showHardwareCursor = enabled;
		if (!enabled) this.terminal.hideCursor();
		this.requestRender();
	}

	/**
	 * Sets whether to clear screen when content shrinks.
	 * 
	 * When enabled, the TUI will perform a full clear and redraw when
	 * the content becomes smaller. This prevents visual artifacts but
	 * is more expensive.
	 * 
	 * @param enabled - Whether to clear on shrink
	 */
	setClearOnShrink(enabled: boolean): void {
		this.clearOnShrink = enabled;
	}

	// --- Focus ---

	/**
	 * Sets focus to a specific component.
	 * 
	 * Only one component can have focus at a time. The focused component
	 * receives keyboard input and typically shows visual focus indicators.
	 * 
	 * @param component - Component to focus, or null to clear focus
	 */
	setFocus(component: Component | null): void {
		if (isFocusable(this.focusedComponent)) {
			this.focusedComponent.focused = false;
		}
		this.focusedComponent = component;
		if (isFocusable(component)) {
			component.focused = true;
		}
	}

	// --- Overlays ---

	/**
	 * Shows an overlay component with specified positioning options.
	 * 
	 * Overlays are floating components that appear above the main content.
	 * They automatically receive focus and can be positioned using various
	 * anchor points and sizing constraints.
	 * 
	 * @param component - Component to show as overlay
	 * @param options - Positioning and sizing options
	 * @returns Handle for controlling overlay visibility
	 * 
	 * @example
	 * ```typescript
	 * const handle = tui.showOverlay(dialog, {
	 *   anchor: 'center',
	 *   width: '50%',
	 *   maxHeight: 20
	 * });
	 * ```
	 */
	showOverlay(component: Component, options?: OverlayOptions): OverlayHandle {
		const entry: OverlayEntry = {
			component,
			options,
			preFocus: this.focusedComponent,
			hidden: false,
		};
		this.overlayStack.push(entry);
		if (this.isOverlayVisible(entry)) {
			this.setFocus(component);
		}
		this.terminal.hideCursor();
		this.requestRender();

		return {
			hide: () => {
				const idx = this.overlayStack.indexOf(entry);
				if (idx === -1) return;
				this.overlayStack.splice(idx, 1);
				if (this.focusedComponent === component) {
					const top = this.getTopmostVisibleOverlay();
					this.setFocus(top?.component ?? entry.preFocus);
				}
				if (this.overlayStack.length === 0) this.terminal.hideCursor();
				this.requestRender();
			},
			setHidden: (hidden: boolean) => {
				if (entry.hidden === hidden) return;
				entry.hidden = hidden;
				if (hidden && this.focusedComponent === component) {
					const top = this.getTopmostVisibleOverlay();
					this.setFocus(top?.component ?? entry.preFocus);
				} else if (!hidden && this.isOverlayVisible(entry)) {
					this.setFocus(component);
				}
				this.requestRender();
			},
			isHidden: () => entry.hidden,
		};
	}

	/**
	 * Hides the topmost overlay.
	 * 
	 * Removes the most recently shown overlay from the stack and
	 * restores focus to the appropriate component.
	 */
	hideOverlay(): void {
		const overlay = this.overlayStack.pop();
		if (!overlay) return;
		const top = this.getTopmostVisibleOverlay();
		this.setFocus(top?.component ?? overlay.preFocus);
		if (this.overlayStack.length === 0) this.terminal.hideCursor();
		this.requestRender();
	}

	/**
	 * Checks if any overlays are currently visible.
	 * 
	 * @returns Whether any overlays are visible
	 */
	hasOverlay(): boolean {
		return this.overlayStack.some((o) => this.isOverlayVisible(o));
	}

	/**
	 * Checks if an overlay entry is currently visible.
	 * 
	 * @param entry - Overlay entry to check
	 * @returns Whether the overlay is visible
	 */
	private isOverlayVisible(entry: OverlayEntry): boolean {
		if (entry.hidden) return false;
		if (entry.options?.visible) {
			return entry.options.visible(this.terminal.columns, this.terminal.rows);
		}
		return true;
	}

	/**
	 * Gets the topmost visible overlay from the stack.
	 * 
	 * @returns Topmost visible overlay or undefined if none
	 */
	private getTopmostVisibleOverlay(): OverlayEntry | undefined {
		for (let i = this.overlayStack.length - 1; i >= 0; i--) {
			if (this.isOverlayVisible(this.overlayStack[i]!)) {
				return this.overlayStack[i];
			}
		}
		return undefined;
	}

	// --- Input ---

	/**
	 * Adds an input listener to the processing chain.
	 * 
	 * Input listeners can intercept, transform, or consume input before
	 * it reaches the focused component. They're processed in order of
	 * addition and can modify the input data.
	 * 
	 * @param listener - Function to handle input
	 * @returns Function to remove the listener
	 */
	addInputListener(listener: InputListener): () => void {
		this.inputListeners.add(listener);
		return () => { this.inputListeners.delete(listener); };
	}

	/**
	 * Removes an input listener from the processing chain.
	 * 
	 * @param listener - Listener function to remove
	 */
	removeInputListener(listener: InputListener): void {
		this.inputListeners.delete(listener);
	}

	/**
	 * Enables mouse tracking. Automatically called when first mouse listener is added.
	 */
	enableMouse(): void {
		if (!this.mouseAllowed) return;
		if (!this.mouseEnabled) {
			this.mouseEnabled = true;
			this.terminal.enableMouse();
		}
	}

	/**
	 * Disables mouse tracking. Automatically called when last mouse listener is removed.
	 */
	disableMouse(): void {
		if (this.mouseEnabled) {
			this.mouseEnabled = false;
			this.terminal.disableMouse();
		}
	}

	/**
	 * Adds a mouse event listener. Enables mouse tracking on first listener.
	 * @returns Unsubscribe function
	 */
	addMouseListener(listener: (event: MouseEvent) => void): () => void {
		this.mouseListeners.add(listener);
		if (this.mouseListeners.size === 1) this.enableMouse();
		return () => {
			this.mouseListeners.delete(listener);
			if (this.mouseListeners.size === 0) this.disableMouse();
		};
	}

	/**
	 * Adds a paste event listener.
	 * @returns Unsubscribe function
	 */
	addPasteListener(listener: (content: string) => void): () => void {
		this.pasteListeners.add(listener);
		return () => { this.pasteListeners.delete(listener); };
	}

	/**
	 * Adds a key release listener. Receives raw data for key release events.
	 * @returns Unsubscribe function
	 */
	addKeyReleaseListener(listener: (data: string) => void): () => void {
		this.keyReleaseListeners.add(listener);
		return () => { this.keyReleaseListeners.delete(listener); };
	}

	/**
	 * Adds a key repeat listener. Receives raw data for key repeat events.
	 * @returns Unsubscribe function
	 */
	addKeyRepeatListener(listener: (data: string) => void): () => void {
		this.keyRepeatListeners.add(listener);
		return () => { this.keyRepeatListeners.delete(listener); };
	}

	/**
	 * Returns the Y offset to convert viewport mouse coordinates to content-relative coordinates.
	 * Accounts for the initial cursor position and any scrolling that has occurred.
	 */
	getContentYOffset(): number {
		if (this.contentStartRow < 0) return 0;
		const height = this.terminal.rows;
		// How many lines the terminal scrolled since we started
		const totalRendered = this.contentStartRow + this.maxLinesRendered;
		const scrolled = Math.max(0, totalRendered - height);
		return this.contentStartRow - scrolled;
	}

	/**
	 * Handles raw input data from the terminal.
	 * 
	 * This method processes input through several stages:
	 * 1. Filter key release events (unless component opts in)
	 * 2. Run through input listener chain
	 * 3. Handle special sequences (cell size responses, debug keys)
	 * 4. Forward to focused component
	 * 
	 * @param data - Raw input data from terminal
	 */
	private handleInput(data: string): void {
		// DSR cursor position response: \x1b[row;colR
		if (this.dsrPending) {
			const m = data.match(/\x1b\[(\d+);(\d+)R/);
			if (m) {
				this.contentStartRow = parseInt(m[1], 10) - 1; // DSR is 1-based
				this.dsrPending = false;
				const rest = data.replace(/\x1b\[\d+;\d+R/, '');
				if (rest.length === 0) return;
				data = rest;
			}
		}

		// Mouse events: parse and dispatch to mouse listeners
		if (this.mouseEnabled && isSGRMouse(data)) {
			const event = parseSGRMouse(data);
			if (event) {
				for (const listener of this.mouseListeners) listener(event);
			}
			return;
		}

		// Bracketed paste: extract content and dispatch to paste listeners
		if (data.startsWith('\x1b[200~') && data.endsWith('\x1b[201~')) {
			if (this.pasteListeners.size > 0) {
				const content = data.slice(6, -6);
				for (const listener of this.pasteListeners) listener(content);
				return;
			}
		}

		// Filter key release events — dispatch to release listeners, then to focused component if it opts in
		if (isKeyRelease(data)) {
			for (const listener of this.keyReleaseListeners) listener(data);
			if (this.focusedComponent?.handleInput && this.focusedComponent.wantsKeyRelease) {
				this.focusedComponent.handleInput(data);
				this.requestRender();
			}
			return;
		}

		// Key repeat events — dispatch to repeat listeners, then continue normal processing
		if (isKeyRepeat(data)) {
			for (const listener of this.keyRepeatListeners) listener(data);
		}

		// Run through input listeners (consume/transform chain)
		if (this.inputListeners.size > 0) {
			let current = data;
			for (const listener of this.inputListeners) {
				const result = listener(current);
				if (result?.consume) return;
				if (result?.data !== undefined) current = result.data;
			}
			if (current.length === 0) return;
			data = current;
		}

		// Cell size response buffering
		if (this.cellSizeQueryPending) {
			this.inputBuffer += data;
			const filtered = this.parseCellSizeResponse();
			if (filtered.length === 0) return;
			data = filtered;
		}

		// Debug key
		if (matchesKey(data, 'ctrl+shift+d') && this.onDebug) {
			this.onDebug();
			return;
		}

		// Verify focused overlay visibility
		const focusedOverlay = this.overlayStack.find((o) => o.component === this.focusedComponent);
		if (focusedOverlay && !this.isOverlayVisible(focusedOverlay)) {
			const top = this.getTopmostVisibleOverlay();
			this.setFocus(top ? top.component : focusedOverlay.preFocus);
		}

		// Forward to focused component
		if (this.focusedComponent?.handleInput) {
			this.focusedComponent.handleInput(data);
			this.requestRender();
		}
	}

	/**
	 * Parses cell size response from terminal and filters it from input.
	 * 
	 * Some terminals send cell size information in response to queries.
	 * This method extracts that information and prevents it from being
	 * processed as regular input.
	 * 
	 * @returns Filtered input data with cell size responses removed
	 */
	private parseCellSizeResponse(): string {
		const pattern = /\x1b\[6;(\d+);(\d+)t/;
		const match = this.inputBuffer.match(pattern);
		if (match) {
			this.inputBuffer = this.inputBuffer.replace(pattern, '');
			this.cellSizeQueryPending = false;
			this.invalidate();
			this.requestRender();
		}

		// Check for partial response
		if (/\x1b(\[6?;?[\d;]*)?$/.test(this.inputBuffer)) {
			const last = this.inputBuffer[this.inputBuffer.length - 1];
			if (last && !/[a-zA-Z~]/.test(last)) return '';
		}

		const result = this.inputBuffer;
		this.inputBuffer = '';
		this.cellSizeQueryPending = false;
		return result;
	}

	// --- Lifecycle ---

	/**
	 * Invalidates the TUI and all its components.
	 * 
	 * Marks all components (including overlays) as needing re-render.
	 * This is called when the terminal size changes or when forced refresh is needed.
	 */
	override invalidate(): void {
		super.invalidate();
		for (const overlay of this.overlayStack) overlay.component.invalidate?.();
	}

	/**
	 * Starts the TUI and begins processing input/output.
	 * 
	 * Initializes the terminal, sets up event handlers, and performs
	 * the initial render. The TUI will continue running until stop() is called.
	 */
	start(): void {
		this.stopped = false;
		this.installStdoutInterceptor();
		this.terminal.start(
			(data) => this.handleInput(data),
			() => {
				// Resize handler: synchronous clear to prevent reflowed content flash.
				this.internalWrite = true;
				this.terminal.write(TUI.CLEAR_ALL);
				this.internalWrite = false;
				this.terminal.hideCursor();
				// Do NOT clear accumulatedStaticOutput — it is prepended to every frame
				// so static content survives resize automatically.
				if (!this.altScreen) {
					this.contentStartRow = 0;
				}
				for (const cb of this.onResizeCallbacks) cb();
				this.requestRender(true);
			},
		);
		if (this.altScreen) {
			this.terminal.write('\x1b[?1049h');
		}
		this.terminal.hideCursor();
		if (!this.altScreen) {
			this.dsrPending = true;
			this.terminal.write('\x1b[6n');
		}
		this.requestRender();
	}

	/**
	 * Stops the TUI and cleans up resources.
	 * 
	 * Positions the cursor at the end of content, shows the hardware cursor,
	 * and stops the terminal. After calling this, the TUI should not be used.
	 */
	/**
	 * Clears all render tracking state. Used when switching screen buffers
	 * so stop() doesn't write stale cursor movement into the wrong buffer.
	 */
	clearRenderState(): void {
		this.previousLines = [];
		this.previousWidth = -1;
		this.cursorRow = 0;
		this.hardwareCursorRow = 0;
		this.maxLinesRendered = 0;
		this.previousViewportTop = 0;
	}

	/**
	 * Enters alternate screen buffer.
	 */
	enterAltScreen(): void {
		this.altScreen = true;
		this.terminal.write('\x1b[?1049h');
		this.requestRender(true);
	}

	/**
	 * Exits alternate screen buffer and clears render state.
	 */
	exitAltScreen(): void {
		this.altScreen = false;
		this.clearRenderState();
		this.terminal.write('\x1b[?1049l');
	}

	isAltScreen(): boolean {
		return this.altScreen;
	}

	stop(): void {
		if (this.stopped) return;
		this.stopped = true;
		this.removeStdoutInterceptor();
		if (this.pacingTimer) {
			clearTimeout(this.pacingTimer);
			this.pacingTimer = null;
		}
		if (this.mouseEnabled) this.disableMouse();
		if (this.altScreen) {
			this.exitAltScreen();
		} else if (this.previousLines.length > 0) {
			const targetRow = this.previousLines.length;
			const diff = targetRow - this.hardwareCursorRow;
			if (diff > 0) this.terminal.write(`\x1b[${diff}B`);
			else if (diff < 0) this.terminal.write(`\x1b[${-diff}A`);
			this.terminal.write('\r\n');
		}
		// Clear internal collections to prevent memory retention after stop
		this.previousLines = [];
		this.accumulatedStaticOutput = [];
		this.overlayStack = [];
		this.onResizeCallbacks.length = 0;
		this.inputListeners.clear();
		this.mouseListeners.clear();
		this.pasteListeners.clear();
		this.keyReleaseListeners.clear();
		this.keyRepeatListeners.clear();
		this.terminal.showCursor();
		this.terminal.stop();
	}

	/**
	 * Requests a render on the next tick.
	 * 
	 * Renders are debounced using process.nextTick to avoid excessive
	 * redraws when multiple changes occur in the same tick.
	 * 
	 * @param force - If true, forces a full redraw by clearing state
	 */
	requestRender(force = false): void {
		if (this.stopped) return;
		if (force) {
			this.previousLines = [];
			this.previousWidth = -1;
			this.cursorRow = 0;
			this.hardwareCursorRow = 0;
			this.maxLinesRendered = 0;
			this.previousViewportTop = 0;
			if (this.pacingTimer) {
				clearTimeout(this.pacingTimer);
				this.pacingTimer = null;
			}
			this.renderRequested = true;
			process.nextTick(() => {
				this.renderRequested = false;
				this.lastRenderTime = performance.now();
				this.doRender();
			});
			return;
		}
		if (this.renderRequested) return;
		this.renderRequested = true;

		// No frame pacing — render on next tick
		if (this.frameBudgetMs <= 0) {
			process.nextTick(() => {
				this.renderRequested = false;
				this.doRender();
			});
			return;
		}

		// Frame pacing — respect budget
		const now = performance.now();
		const elapsed = now - this.lastRenderTime;
		if (elapsed >= this.frameBudgetMs) {
			process.nextTick(() => {
				this.renderRequested = false;
				this.lastRenderTime = performance.now();
				this.doRender();
			});
		} else if (!this.pacingTimer) {
			const remaining = this.frameBudgetMs - elapsed;
			this.pacingTimer = setTimeout(() => {
				this.pacingTimer = null;
				this.renderRequested = false;
				this.lastRenderTime = performance.now();
				this.doRender();
			}, remaining);
		}
	}

	// --- Overlay layout ---

	/**
	 * Resolves overlay positioning and sizing based on options and constraints.
	 * 
	 * Calculates the final position and dimensions for an overlay considering:
	 * - Terminal dimensions and available space
	 * - Margin constraints
	 * - Anchor positioning
	 * - Size constraints (min/max width/height)
	 * - Explicit positioning overrides
	 * 
	 * @param options - Overlay positioning options
	 * @param overlayHeight - Actual height of rendered overlay content
	 * @param termWidth - Terminal width in columns
	 * @param termHeight - Terminal height in rows
	 * @returns Resolved layout with width, position, and constraints
	 */
	private resolveOverlayLayout(
		options: OverlayOptions | undefined,
		overlayHeight: number,
		termWidth: number,
		termHeight: number,
	): { width: number; row: number; col: number; maxHeight: number | undefined } {
		const opt = options ?? {};
		const margin = typeof opt.margin === 'number'
			? { top: opt.margin, right: opt.margin, bottom: opt.margin, left: opt.margin }
			: (opt.margin ?? {});
		const mT = Math.max(0, margin.top ?? 0);
		const mR = Math.max(0, margin.right ?? 0);
		const mB = Math.max(0, margin.bottom ?? 0);
		const mL = Math.max(0, margin.left ?? 0);
		const availW = Math.max(1, termWidth - mL - mR);
		const availH = Math.max(1, termHeight - mT - mB);

		let width = parseSizeValue(opt.width, termWidth) ?? Math.min(80, availW);
		if (opt.minWidth !== undefined) width = Math.max(width, opt.minWidth);
		width = Math.max(1, Math.min(width, availW));

		let maxHeight = parseSizeValue(opt.maxHeight, termHeight);
		if (maxHeight !== undefined) maxHeight = Math.max(1, Math.min(maxHeight, availH));

		const effH = maxHeight !== undefined ? Math.min(overlayHeight, maxHeight) : overlayHeight;
		const anchor = opt.anchor ?? 'center';

		let row: number;
		if (opt.row !== undefined) {
			if (typeof opt.row === 'string') {
				const m = opt.row.match(/^(\d+(?:\.\d+)?)%$/);
				if (m) {
					row = mT + Math.floor(Math.max(0, availH - effH) * parseFloat(m[1]!) / 100);
				} else {
					row = this.resolveAnchorRow(anchor, effH, availH, mT);
				}
			} else {
				row = opt.row;
			}
		} else {
			row = this.resolveAnchorRow(anchor, effH, availH, mT);
		}

		let col: number;
		if (opt.col !== undefined) {
			if (typeof opt.col === 'string') {
				const m = opt.col.match(/^(\d+(?:\.\d+)?)%$/);
				if (m) {
					col = mL + Math.floor(Math.max(0, availW - width) * parseFloat(m[1]!) / 100);
				} else {
					col = this.resolveAnchorCol(anchor, width, availW, mL);
				}
			} else {
				col = opt.col;
			}
		} else {
			col = this.resolveAnchorCol(anchor, width, availW, mL);
		}

		if (opt.offsetY !== undefined) row += opt.offsetY;
		if (opt.offsetX !== undefined) col += opt.offsetX;
		row = Math.max(mT, Math.min(row, termHeight - mB - effH));
		col = Math.max(mL, Math.min(col, termWidth - mR - width));

		return { width, row, col, maxHeight };
	}

	/**
	 * Resolves the row position based on anchor point.
	 * 
	 * @param anchor - Anchor position
	 * @param h - Overlay height
	 * @param availH - Available height
	 * @param mT - Top margin
	 * @returns Resolved row position
	 */
	private resolveAnchorRow(anchor: OverlayAnchor, h: number, availH: number, mT: number): number {
		switch (anchor) {
			case 'top-left': case 'top-center': case 'top-right': return mT;
			case 'bottom-left': case 'bottom-center': case 'bottom-right': return mT + availH - h;
			default: return mT + Math.floor((availH - h) / 2);
		}
	}

	/**
	 * Resolves the column position based on anchor point.
	 * 
	 * @param anchor - Anchor position
	 * @param w - Overlay width
	 * @param availW - Available width
	 * @param mL - Left margin
	 * @returns Resolved column position
	 */
	private resolveAnchorCol(anchor: OverlayAnchor, w: number, availW: number, mL: number): number {
		switch (anchor) {
			case 'top-left': case 'left-center': case 'bottom-left': return mL;
			case 'top-right': case 'right-center': case 'bottom-right': return mL + availW - w;
			default: return mL + Math.floor((availW - w) / 2);
		}
	}

	// --- Overlay compositing ---

	/**
	 * Composites overlays onto the base content.
	 * 
	 * This method renders all visible overlays and composites them onto
	 * the base content, handling:
	 * - Overlay positioning and clipping
	 * - Viewport scrolling for tall content
	 * - Z-order (overlay stack order)
	 * - Size constraints and truncation
	 * 
	 * @param lines - Base content lines
	 * @param termWidth - Terminal width
	 * @param termHeight - Terminal height
	 * @returns Composited lines with overlays applied
	 */
	private compositeOverlays(lines: string[], termWidth: number, termHeight: number): string[] {
		if (this.overlayStack.length === 0) return lines;
		const result = [...lines];

		const rendered: { overlayLines: string[]; row: number; col: number; w: number }[] = [];
		let minLinesNeeded = result.length;

		for (const entry of this.overlayStack) {
			if (!this.isOverlayVisible(entry)) continue;
			const { width, maxHeight } = this.resolveOverlayLayout(entry.options, 0, termWidth, termHeight);
			let overlayLines = entry.component.render(width);
			if (maxHeight !== undefined && overlayLines.length > maxHeight) {
				overlayLines = overlayLines.slice(0, maxHeight);
			}
			const { row, col } = this.resolveOverlayLayout(entry.options, overlayLines.length, termWidth, termHeight);
			rendered.push({ overlayLines, row, col, w: width });
			minLinesNeeded = Math.max(minLinesNeeded, row + overlayLines.length);
		}

		const workingHeight = Math.max(this.maxLinesRendered, minLinesNeeded);
		while (result.length < workingHeight) result.push('');

		const viewportStart = Math.max(0, workingHeight - termHeight);

		for (const { overlayLines, row, col, w } of rendered) {
			for (let i = 0; i < overlayLines.length; i++) {
				const idx = viewportStart + row + i;
				if (idx >= 0 && idx < result.length) {
					const truncated = visibleWidth(overlayLines[i]!) > w
						? sliceByColumn(overlayLines[i]!, 0, w, true)
						: overlayLines[i]!;
					result[idx] = this.compositeLineAt(result[idx]!, truncated, col, w, termWidth);
				}
			}
		}

		return result;
	}

	/**
	 * Composites an overlay line onto a base line at a specific position.
	 * 
	 * This method handles the complex task of merging overlay content with
	 * base content while preserving ANSI formatting and handling padding.
	 * It extracts segments before and after the overlay region and properly
	 * composites them with reset sequences to prevent style bleeding.
	 * 
	 * @param baseLine - Base line content
	 * @param overlayLine - Overlay line content
	 * @param startCol - Starting column for overlay
	 * @param overlayWidth - Width of overlay region
	 * @param totalWidth - Total line width
	 * @returns Composited line
	 */
	private compositeLineAt(
		baseLine: string,
		overlayLine: string,
		startCol: number,
		overlayWidth: number,
		totalWidth: number,
	): string {
		const afterStart = startCol + overlayWidth;
		const base = extractSegments(baseLine, startCol, afterStart, totalWidth - afterStart, true);
		const overlay = sliceWithWidth(overlayLine, 0, overlayWidth, true);

		const beforePad = Math.max(0, startCol - base.beforeWidth);
		const overlayPad = Math.max(0, overlayWidth - overlay.width);
		const actualBeforeW = Math.max(startCol, base.beforeWidth);
		const actualOverlayW = Math.max(overlayWidth, overlay.width);
		const afterTarget = Math.max(0, totalWidth - actualBeforeW - actualOverlayW);
		const afterPad = Math.max(0, afterTarget - base.afterWidth);

		const r = TUI.SEGMENT_RESET;
		const result = base.before + ' '.repeat(beforePad) + r +
			overlay.text + ' '.repeat(overlayPad) + r +
			base.after + ' '.repeat(afterPad);

		if (visibleWidth(result) <= totalWidth) return result;
		return sliceByColumn(result, 0, totalWidth, true);
	}

	// --- Cursor extraction ---

	/**
	 * Extracts cursor position from rendered lines and removes cursor markers.
	 * 
	 * Scans the rendered output for cursor markers and determines the
	 * visual position where the cursor should be placed. The markers
	 * are removed from the output to prevent them from being displayed.
	 * 
	 * @param lines - Rendered lines (modified in place)
	 * @param height - Terminal height for viewport calculation
	 * @returns Cursor position or null if no cursor found
	 */
	private extractCursorPosition(lines: string[], height: number): { row: number; col: number } | null {
		const viewportTop = Math.max(0, lines.length - height);
		for (let row = lines.length - 1; row >= viewportTop; row--) {
			const line = lines[row]!;
			const idx = line.indexOf(CURSOR_MARKER);
			if (idx !== -1) {
				const col = visibleWidth(line.slice(0, idx));
				lines[row] = line.slice(0, idx) + line.slice(idx + CURSOR_MARKER.length);
				return { row, col };
			}
		}
		return null;
	}

	/**
	 * Applies reset sequences to the end of each line.
	 * 
	 * Ensures that ANSI formatting doesn't bleed between lines by
	 * adding reset sequences. This is crucial for proper rendering
	 * and prevents visual artifacts.
	 * 
	 * @param lines - Lines to process (modified in place)
	 * @returns Processed lines with reset sequences
	 */
	private applyLineResets(lines: string[]): string[] {
		const reset = TUI.SEGMENT_RESET;
		for (let i = 0; i < lines.length; i++) {
			lines[i] = lines[i]! + reset;
		}
		return lines;
	}

	/**
	 * Adds static lines to the accumulated static output.
	 * These lines are rendered above live content and scroll into terminal scrollback.
	 */
	writeStaticLines(lines: string[]): void {
		if (lines.length > 0) {
			this.accumulatedStaticOutput.push(...lines);
			this.trimStaticOutput();
			// When content overflowed the viewport, old active content is stuck
			// in scrollback where cursor-up can't reach. Clear everything and
			// let the next render do a clean full redraw.
			if (this.previousLines.length > this.terminal.rows && !this.altScreen) {
				const screenRow = this.hardwareCursorRow - this.previousViewportTop;
				const linesToErase = Math.min(screenRow + 1, this.terminal.rows);
				let buf = '\x1b[3J';
				for (let i = 0; i < linesToErase; i++) {
					buf += '\x1b[2K' + (i < linesToErase - 1 ? '\x1b[1A' : '');
				}
				buf += '\r\x1b[J';
				this.terminal.write(buf);
				this.previousLines = [];
				this.hardwareCursorRow = 0;
				this.cursorRow = 0;
				this.maxLinesRendered = 0;
				this.previousViewportTop = 0;
			}
		}
	}

	/**
	 * Caps the accumulated static output buffer to the last {@link STATIC_OUTPUT_CAP} lines.
	 *
	 * ## Why this cap exists
	 *
	 * `accumulatedStaticOutput` is prepended to every rendered frame so that static
	 * content (chat history, log lines, completed tasks) survives terminal redraws and
	 * resizes. Without a cap it grows without bound — a 1,000-turn conversation with
	 * 500-line responses would accumulate 500,000 strings in memory indefinitely.
	 *
	 * ## Why 10,000 lines
	 *
	 * Most terminals default to a 10,000-line scrollback buffer (iTerm2, GNOME Terminal,
	 * Terminal.app). Lines beyond that limit have already been evicted from the terminal's
	 * own buffer, so re-emitting them on resize would produce content the user can never
	 * scroll back to see. Keeping more than 10,000 lines in memory is therefore pure waste.
	 *
	 * ## App-owner responsibility
	 *
	 * This cap bounds the *string buffer* inside TUI. The React fiber tree is a separate
	 * concern: each item passed to `<Static items={...}>` creates a fiber that lives until
	 * the item is removed from the array. For long-running conversations, the app should
	 * also cap the `items` array (e.g. `messages.slice(-50)`) to bound fiber memory.
	 * Twinki's `totalStaticWritten` cursor is monotonically increasing, so removing
	 * already-flushed items from the front of `items` is safe — they will not be
	 * re-written to scrollback.
	 */
	private trimStaticOutput(): void {
		const cap = 10_000;
		if (this.accumulatedStaticOutput.length > cap) {
			this.accumulatedStaticOutput = this.accumulatedStaticOutput.slice(-cap);
		}
	}

	/**
	 * Replaces all accumulated static output. Used on resize to re-render
	 * static content at the new width without duplication.
	 */
	replaceStaticOutput(lines: string[]): void {
		this.accumulatedStaticOutput = lines;
		this.trimStaticOutput();
	}

	/**
	 * Clears accumulated static output. Called on resize so static content
	 * can be re-rendered at the new width.
	 */
	resetStaticOutput(): void {
		this.accumulatedStaticOutput = [];
	}

	/**
	 * Intercepts `process.stdout.write` to detect external clear sequences.
	 *
	 * A differential renderer maintains a shadow buffer (`previousLines`) that
	 * must mirror the terminal screen. When external code (e.g. Ctrl+L handler)
	 * writes clear sequences (`\x1b[2J` or `\x1b[3J`) directly to stdout, the
	 * terminal is wiped but the shadow buffer becomes stale. Without detection,
	 * the next differential render would skip unchanged lines — leaving the
	 * screen blank or corrupted.
	 *
	 * This intercept detects those sequences and calls `handleExternalClear()`
	 * to reset the shadow buffer and force a full redraw.
	 *
	 * The `internalWrite` flag prevents twinki's own clear sequences (resize,
	 * Strategy 2) from triggering the intercept.
	 */
	private installStdoutInterceptor(): void {
		if (this.originalStdoutWrite) return;
		if (!process.stdout || typeof process.stdout.write !== 'function') return;

		this.originalStdoutWrite = process.stdout.write.bind(process.stdout);
		const originalWrite = this.originalStdoutWrite;
		const self = this;

		process.stdout.write = function(chunk: any, ...args: any[]): boolean {
			if (!self.stopped && !self.internalWrite
				&& typeof chunk === 'string'
				&& (chunk.includes('\x1b[2J') || chunk.includes('\x1b[3J'))) {
				self.handleExternalClear();
			}
			return originalWrite(chunk, ...args);
		} as typeof process.stdout.write;
	}

	/**
	 * Restores the original `process.stdout.write` if it was intercepted.
	 */
	private removeStdoutInterceptor(): void {
		if (this.originalStdoutWrite) {
			process.stdout.write = this.originalStdoutWrite as typeof process.stdout.write;
			this.originalStdoutWrite = null;
		}
	}

	/**
	 * Resets all render state after an external clear.
	 *
	 * When the terminal is cleared externally (e.g. Ctrl+L), the shadow buffer
	 * (`previousLines`) no longer matches the screen. This method invalidates
	 * all cached state and forces a full redraw on the next render cycle.
	 *
	 * Also clears `accumulatedStaticOutput` since that content was wiped from
	 * the terminal along with everything else.
	 */
	private handleExternalClear(): void {
		this.previousLines = [];
		this.maxLinesRendered = 0;
		this.hardwareCursorRow = 0;
		this.cursorRow = 0;
		this.previousViewportTop = 0;
		this.accumulatedStaticOutput = [];
		this.requestRender(true);
	}

	/**
	 * Register a callback to be called on terminal resize.
	 */
	onResize(cb: () => void): void {
		this.onResizeCallbacks.push(cb);
	}

	// --- Render ---

	/**
	 * Main render method with performance tracking.
	 * 
	 * Wraps the actual render implementation with performance monitoring
	 * to track render times and update performance counters.
	 */
	private doRender(): void {
		if (this.stopped) return;

		// Write pressure: defer if stdout buffer is saturated
		if (this.frameBudgetMs > 0) {
			const stdout = (process as any).stdout;
			if (stdout?.writableNeedDrain) {
				this.renderRequested = true;
				stdout.once('drain', () => {
					this.renderRequested = false;
					this.lastRenderTime = performance.now();
					this.doRender();
				});
				return;
			}
		}

		const renderStart = performance.now();
		// Mark as internal write so our stdout interceptor ignores any clear
		// sequences emitted by the render strategies (e.g. Strategy 2 CLEAR_ALL).
		this.internalWrite = true;
		try {
		this._doRenderInner();
		} finally {
		this.internalWrite = false;
		const elapsed = performance.now() - renderStart;
		this.perfLastRenderMs = elapsed;
		this.perfTotalRenderMs += elapsed;
		this.perfRenderCount++;
		if (elapsed > this.perfMaxRenderMs) this.perfMaxRenderMs = elapsed;
		}
	}

	/**
	 * Core render implementation with 4-strategy differential rendering.
	 * 
	 * This is the heart of Twinki's rendering system, implementing four
	 * distinct rendering strategies:
	 * 
	 * 1. **First render**: Write all content without cursor movement
	 * 2. **Width changed**: Clear screen and full redraw
	 * 3. **Shrink clear**: Full clear when content shrinks (optional)
	 * 4. **Differential**: Only update changed lines (most common)
	 * 
	 * The method handles:
	 * - Static line insertion into scrollback
	 * - Overlay compositing
	 * - Cursor position extraction and management
	 * - Viewport scrolling for tall content
	 * - Synchronized output to prevent tearing
	 * - Line-based diffing for minimal terminal writes
	 */
	private _doRenderInner(): void {
		const width = this.terminal.columns;
		const height = this.terminal.rows;
		let viewportTop = Math.max(0, this.maxLinesRendered - height);
		let prevViewportTop = this.previousViewportTop;
		let hardwareCursorRow = this.hardwareCursorRow;

		const computeLineDiff = (targetRow: number): number => {
			const currentScreenRow = hardwareCursorRow - prevViewportTop;
			const targetScreenRow = targetRow - viewportTop;
			return targetScreenRow - currentScreenRow;
		};

		let newLines = this.render(width);

		// OPTIMIZED: Combine accumulated static output with live content
		// Use concat instead of spread operator for better performance with large arrays
		if (this.accumulatedStaticOutput.length > 0) {
			newLines = this.accumulatedStaticOutput.concat(newLines);
		}

		if (this.overlayStack.length > 0) {
			newLines = this.compositeOverlays(newLines, width, height);
		}

		const cursorPos = this.extractCursorPosition(newLines, height);
		newLines = this.applyLineResets(newLines);

		const widthChanged = this.previousWidth !== 0 && this.previousWidth !== width;

		// Crash guard: detect lines exceeding terminal width (only when debug enabled)
		if (this.debugLogFd != null) {
			for (let i = 0; i < newLines.length; i++) {
				const vw = visibleWidth(newLines[i]);
				if (vw > width) {
					const msg = `Line ${i} visible width (${vw}) exceeds terminal width (${width}). Content: ${newLines[i].slice(0, 120)}...`;
					this.debugLog(`CRASH GUARD: ${msg}`);
					try {
						const fs = require('fs'), os = require('os'), path = require('path');
						const dir = path.join(os.homedir(), '.twinki');
						fs.mkdirSync(dir, { recursive: true });
						fs.writeFileSync(path.join(dir, 'crash.log'), `[${new Date().toISOString()}] ${msg}\n`, { flag: 'a' });
					} catch { /* ignore */ }
				}
			}
		}

		// Debug logging
		this.debugLog(`render: lines=${newLines.length} prev=${this.previousLines.length} width=${width} widthChanged=${widthChanged}`);

		/**
		 * Writes all lines to the terminal as a single synchronized frame.
		 *
		 * @param clearSeq - ANSI sequence to prepend before content:
		 *   - '' (empty): first inline render, no clearing
		 *   - '\x1b[3J\x1b[2J\x1b[H': full clear including scrollback (width change, shrink)
		 *   - '\x1b[2J\x1b[H': clear visible screen only, preserve scrollback (alt screen)
		 */
		const fullRender = (clearSeq: string): void => {
			this.fullRedrawCount++;
			const sync = !process.env['TWINKI_NO_SYNC'];
			let buffer = (sync ? '\x1b[?2026h' : '') + clearSeq;
			for (let i = 0; i < newLines.length; i++) {
				if (i > 0) buffer += '\r\n';
				buffer += newLines[i];
			}
			if (sync) buffer += '\x1b[?2026l';
			this.terminal.write(buffer);
			this.cursorRow = Math.max(0, newLines.length - 1);
			this.hardwareCursorRow = this.cursorRow;
			this.maxLinesRendered = clearSeq ? newLines.length : Math.max(this.maxLinesRendered, newLines.length);
			this.previousViewportTop = Math.max(0, this.maxLinesRendered - height);
			this.positionHardwareCursor(cursorPos, newLines.length);
			this.previousLines = newLines;
			this.previousWidth = width;
		};

		const CLEAR_ALL = TUI.CLEAR_ALL;
		const CLEAR_SCREEN = TUI.CLEAR_SCREEN;

		// Strategy 1: First render (also after external clear)
		// \x1b[J clears from cursor to end of screen, preventing stale content
		// below the new output (e.g. after Ctrl+L cleared the screen).
		if (this.previousLines.length === 0 && !widthChanged) {
			fullRender(this.altScreen ? CLEAR_SCREEN : '\x1b[J');
			return;
		}

		// Strategy 2: Width changed
		if (widthChanged) {
			fullRender(this.altScreen ? CLEAR_SCREEN : CLEAR_ALL);
			return;
		}

		// Strategy 3: Shrink clear
		if (this.clearOnShrink && newLines.length < this.maxLinesRendered && this.overlayStack.length === 0) {
			fullRender(this.altScreen ? CLEAR_SCREEN : CLEAR_ALL);
			return;
		}

		// Strategy 4: Differential
		let firstChanged = -1;
		let lastChanged = -1;
		const maxLen = Math.max(newLines.length, this.previousLines.length);
		for (let i = 0; i < maxLen; i++) {
			const oldLine = this.previousLines[i] ?? '';
			const newLine = newLines[i] ?? '';
			if (oldLine !== newLine) {
				if (firstChanged === -1) firstChanged = i;
				lastChanged = i;
			}
		}

		const appendedLines = newLines.length > this.previousLines.length;
		if (appendedLines) {
			if (firstChanged === -1) firstChanged = this.previousLines.length;
			lastChanged = newLines.length - 1;
		}
		const appendStart = appendedLines && firstChanged === this.previousLines.length && firstChanged > 0;

		// No changes
		if (firstChanged === -1) {
			this.positionHardwareCursor(cursorPos, newLines.length);
			this.previousViewportTop = Math.max(0, this.maxLinesRendered - height);
			return;
		}

		// All changes in deleted lines
		if (firstChanged >= newLines.length) {
			if (this.previousLines.length > newLines.length) {
				const sync = !process.env['TWINKI_NO_SYNC'];
				let buffer = sync ? '\x1b[?2026h' : '';
				const targetRow = Math.max(0, newLines.length - 1);
				const lineDiff = computeLineDiff(targetRow);
				if (lineDiff > 0) buffer += `\x1b[${lineDiff}B`;
				else if (lineDiff < 0) buffer += `\x1b[${-lineDiff}A`;
				buffer += '\r';
				const extra = this.previousLines.length - newLines.length;
				if (extra > height) { fullRender(this.altScreen ? CLEAR_SCREEN : CLEAR_ALL); return; }
				if (extra > 0) buffer += '\x1b[1B';
				for (let i = 0; i < extra; i++) {
					buffer += '\r\x1b[2K';
					if (i < extra - 1) buffer += '\x1b[1B';
				}
				if (extra > 0) buffer += `\x1b[${extra}A`;
				if (sync) buffer += '\x1b[?2026l';
				this.terminal.write(buffer);
				this.cursorRow = targetRow;
				this.hardwareCursorRow = targetRow;
			}
			this.positionHardwareCursor(cursorPos, newLines.length);
			this.previousLines = newLines;
			this.previousWidth = width;
			this.previousViewportTop = Math.max(0, this.maxLinesRendered - height);
			return;
		}

		// Change above previous viewport
		const previousContentViewportTop = Math.max(0, this.previousLines.length - height);
		if (firstChanged < previousContentViewportTop) {
			fullRender(this.altScreen ? CLEAR_SCREEN : CLEAR_ALL);
			return;
		}

		// Build differential buffer
		const sync = !process.env['TWINKI_NO_SYNC'];
		let buffer = sync ? '\x1b[?2026h' : '';
		const prevViewportBottom = prevViewportTop + height - 1;
		const moveTargetRow = appendStart ? firstChanged - 1 : firstChanged;

		if (moveTargetRow > prevViewportBottom) {
			const currentScreenRow = Math.max(0, Math.min(height - 1, hardwareCursorRow - prevViewportTop));
			const moveToBottom = height - 1 - currentScreenRow;
			if (moveToBottom > 0) buffer += `\x1b[${moveToBottom}B`;
			const scroll = moveTargetRow - prevViewportBottom;
			buffer += '\r\n'.repeat(scroll);
			prevViewportTop += scroll;
			viewportTop += scroll;
			hardwareCursorRow = moveTargetRow;
		}

		const lineDiff = computeLineDiff(moveTargetRow);
		if (lineDiff > 0) buffer += `\x1b[${lineDiff}B`;
		else if (lineDiff < 0) buffer += `\x1b[${-lineDiff}A`;
		buffer += appendStart ? '\r\n' : '\r';

		const renderEnd = Math.min(lastChanged, newLines.length - 1);
		for (let i = firstChanged; i <= renderEnd; i++) {
			if (i > firstChanged) buffer += '\r\n';
			buffer += '\x1b[2K';
			buffer += newLines[i];
		}

		let finalCursorRow = renderEnd;

		if (this.previousLines.length > newLines.length) {
			if (renderEnd < newLines.length - 1) {
				const moveDown = newLines.length - 1 - renderEnd;
				buffer += `\x1b[${moveDown}B`;
				finalCursorRow = newLines.length - 1;
			}
			const extra = this.previousLines.length - newLines.length;
			for (let i = 0; i < extra; i++) {
				buffer += '\r\n\x1b[2K';
			}
			buffer += `\x1b[${extra}A`;
		}

		if (sync) buffer += '\x1b[?2026l';
		this.terminal.write(buffer);

		this.cursorRow = Math.max(0, newLines.length - 1);
		this.hardwareCursorRow = finalCursorRow;
		this.maxLinesRendered = Math.max(this.maxLinesRendered, newLines.length);
		this.previousViewportTop = Math.max(0, this.maxLinesRendered - height);
		this.positionHardwareCursor(cursorPos, newLines.length);
		this.previousLines = newLines;
		this.previousWidth = width;
	}

	/**
	 * Positions the hardware cursor at the specified location.
	 * 
	 * Moves the terminal's hardware cursor to the given position using
	 * ANSI escape sequences. Handles cursor visibility based on the
	 * showHardwareCursor setting.
	 * 
	 * @param cursorPos - Target cursor position or null to hide
	 * @param totalLines - Total number of lines in content
	 */
	private positionHardwareCursor(cursorPos: { row: number; col: number } | null, totalLines: number): void {
		if (!cursorPos || totalLines <= 0) {
			this.terminal.hideCursor();
			return;
		}
		const targetRow = Math.max(0, Math.min(cursorPos.row, totalLines - 1));
		const targetCol = Math.max(0, cursorPos.col);
		const rowDelta = targetRow - this.hardwareCursorRow;
		let buf = '';
		if (rowDelta > 0) buf += `\x1b[${rowDelta}B`;
		else if (rowDelta < 0) buf += `\x1b[${-rowDelta}A`;
		buf += `\x1b[${targetCol + 1}G`;
		if (buf) this.terminal.write(buf);
		this.hardwareCursorRow = targetRow;
		if (this.showHardwareCursor) this.terminal.showCursor();
		else this.terminal.hideCursor();
	}
}
