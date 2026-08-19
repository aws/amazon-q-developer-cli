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
import { isHardwareCursorEnabled } from './hardware-cursor.js';
import { uninvertCursorCell } from './cursor-cell.js';
import type { Terminal } from '../terminal/terminal.js';
import { isKeyRelease, isKeyRepeat, matchesKey } from '../input/keys.js';
import { throttle } from 'es-toolkit/compat';
import { parseSGRMouseEvents } from '../input/mouse.js';
import type { MouseEvent } from '../input/mouse.js';
import { visibleWidth } from '../utils/visible-width.js';
import { StaticBuffer } from './static-buffer.js';
import { sliceByColumn, sliceWithWidth } from '../utils/slice.js';
import { extractSegments } from '../utils/extract-segments.js';
import {
  clampTextSelectionPoint,
  extractSelectedText,
  highlightTextSelection,
} from './text-selection.js';
import type {
  TextSelectionBounds,
  TextSelectionPoint,
} from './text-selection.js';
import { osc52ClipboardSequence } from '../terminal/osc52.js';

type TextSelectionScopeResolver = (
  point: TextSelectionPoint
) => TextSelectionBounds | null;

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
  /**
   * Enable renderer-level mouse text selection and clipboard copy on release.
   * Implies mouse tracking. Default: false.
   */
  textSelection?: boolean;
  /**
   * Max lines to keep in the static scrollback buffer (default: 10_000).
   * When exceeded by 10%, the buffer is pruned back to 75% of the cap.
   */
  staticScrollbackCap?: number;
  /**
   * Columns to reserve for the terminal scrollbar (default: 0).
   *
   * When set, the TUI renders at `terminal.columns - scrollbarWidth` so that
   * scrollbar appearance/disappearance does not trigger a width-change reflow.
   * Typical value: 2 (the width most terminals steal for a scrollbar).
   */
  scrollbarWidth?: number;
  /**
   * Enable support for lines wider than terminal width that the terminal
   * soft-wraps into multiple physical rows (e.g. components using
   * `wrap="overflow"`). When true, the renderer tracks physical rows
   * for cursor positioning, viewport math, and differential clearing.
   *
   * Default: false (faster path — assumes every logical line is exactly
   * one physical row). When overflow components are present and this is
   * false, cursor positioning and diff math go wrong for soft-wrapped
   * rows, leaving ghost copies in scrollback during streaming.
   */
  wideLines?: boolean;
  /**
   * Minimum layout width in columns (default: 10).
   * Prevents yoga calculateLayout from entering deep recursion at tiny
   * terminal widths. The render width is clamped to this floor.
   */
  minWidth?: number;
  /**
   * Repaint only the viewport on native-scrollback full redraws instead of
   * clearing scrollback and re-emitting the whole frame (default: false).
   *
   * Only safe when no element spans rows above the viewport. Rows above are
   * left as committed history, so anything drawn as one continuous vertical
   * run — a full-height gutter, border, or status bar — keeps whatever
   * partial state it had when it scrolled off and renders with visible gaps.
   * Enable only for layouts whose off-screen rows are self-contained.
   */
  preserveScrollbackOnRedraw?: boolean;
  /** Whether the terminal supports BSU (DEC 2026). When false, multi-line
   *  writes draw progressively. Default: true unless TWINKI_NO_SYNC=1. */
  synchronizedOutput?: boolean;
}

export type RenderKind = 'full' | 'partial' | 'viewport-tail';

export interface RenderCompletedEvent {
  durationMs: number;
  /**
   * 'full' and 'partial' name the strategy that painted. A 'viewport-tail'
   * paint writes only the trailing window; rows above it keep whatever
   * bytes they last painted with.
   */
  kind: RenderKind;
  /**
   * True only when every live-region row's current bytes are physically on
   * the terminal (static-prefix rows are committed scrollback and cannot be
   * repainted). A viewport-tail paint whose window excludes live rows
   * leaves them stale, and because it commits the whole frame to the shadow
   * buffer, later differential paints inherit that staleness. It clears
   * when a paint covers the whole live region again — a full render, or a
   * tail paint whose window reaches the static prefix.
   */
  frameRowsCurrent: boolean;
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
  /**
   * Per-frame work counters, rewritten on every render.
   *
   * @internal Diagnostics only — shape is unstable and may change without
   * notice. Do not depend on this outside twinki's own tests.
   *
   * These exist to make the static-prefix invariant *deterministically*
   * testable. Wall-clock timings are machine- and load-dependent, so a timing
   * assertion is both flaky and weak evidence. These are exact integers: if
   * frame work is genuinely independent of scrollback size, then for a fixed
   * live region every counter here is identical at 1,000 and 10,000 accumulated
   * static lines. That is an equality assertion, not a threshold.
   *
   * - `diffScanned`     — iterations of the differential comparison loop
   * - `resetsApplied`   — lines the per-frame line-reset pass rewrote
   * - `cursorScanned`   — iterations of the above-viewport cursor-marker scan
   * - `prefixLines`     — accumulated static lines present this frame
   * - `prefixCopied`    — static references copied into a resident frame
   * - `prefixSkipped`   — whether the prefix was excluded from the diff
   */
  public perfLastFrame = {
    diffScanned: 0,
    resetsApplied: 0,
    cursorScanned: 0,
    prefixLines: 0,
    prefixCopied: 0,
    prefixSkipped: false,
  };
  private readonly renderCompleteListeners = new Set<
    (event: RenderCompletedEvent) => void
  >();
  private lastRenderKind: RenderKind = 'partial';
  /**
   * Latched when a viewport-tail paint leaves live rows above its window
   * unwritten while committing the whole frame to the shadow buffer:
   * differential paints then diff against rows that never reached the
   * terminal, so the staleness survives them. Clears when a paint covers
   * the whole live region — a full render, or a tail paint whose window
   * reaches the static prefix (the post-flush repaint at turn boundaries).
   */
  private paintedRowsStale = false;
  /** Number of lines currently held in the static scrollback buffer. */
  get staticBufferLines(): number {
    return this.staticBuffer.length;
  }
  /** Number of rendered lines currently above the visible viewport (unreachable by cursor-up). */
  get linesAboveViewport(): number {
    return this.previousViewportTop;
  }

  private previousLines: string[] = [];
  /**
   * Generation of {@link staticBuffer} that `previousLines` was committed
   * against. `-1` means there is no valid shadow buffer, forcing a full diff.
   */
  private frameGeneration = -1;
  /**
   * Whether `previousLines` contains any line wider than the current
   * terminal width. Cached because checking is O(n) over all lines and
   * `previousLines` can be very large (stress tests: 50k+ lines). Set
   * whenever `previousLines` is assigned a new value.
   */
  private previousHasWide = false;
  /**
   * Cached sum of physical rows in `previousLines`. Updated whenever
   * `previousLines` is assigned (we always know the new value). `-1` means
   * stale — recompute on next access. Avoids O(n) scan every render when
   * `previousLines` can be 50k+ lines.
   */
  private previousPhysRowsCache = 0;
  private previousWidth = 0;
  private focusedComponent: Component | null = null;
  private inputListeners = new Set<InputListener>();
  private renderRequested = false;
  private deferInputRender = false;
  private inputDeferralTimer: ReturnType<typeof setImmediate> | null = null;
  private cursorRow = 0;
  private hardwareCursorRow = 0;
  private inputBuffer = '';
  private cellSizeQueryPending = false;
  private showHardwareCursor = isHardwareCursorEnabled();
  private clearOnShrink = process.env.TWINKI_CLEAR_ON_SHRINK === '1';
  private preserveScrollbackOnRedraw = false;
  /** Emit DEC 2026 markers unless explicitly disabled for debugging. */
  private emitBsu = !process.env['TWINKI_NO_SYNC'];
  /** Whether DEC 2026 makes multi-line writes atomic. */
  private synchronizedOutput = !process.env['TWINKI_NO_SYNC'];
  private maxLinesRendered = 0;
  private previousViewportTop = 0;
  /** Frame physical row of the first row the renderer may touch. Rows above
   *  it are committed history a repaint left painted in place (a straddled
   *  wide line, or rows above a short tail paint): relative moves must not
   *  climb into them and erases must not wipe them. Set by paints, reset
   *  with the frame; consumed as `max(previousViewportTop, ownedTopRow)` so
   *  it expires naturally once the viewport scrolls past it. */
  private ownedTopRow = 0;
  private fullRedrawCount = 0;
  private stopped = false;
  private overlayStack: OverlayEntry[] = [];
  private readonly staticBuffer = new StaticBuffer();
  /**
   * Cached flag: does `accumulatedStaticOutput` contain any line wider than
   * the last terminal width we saw? Recomputed lazily in
   * {@link _doRenderInner} when width changes or on explicit reset; updated
   * incrementally when new static lines are pushed. Lets the hot path avoid
   * re-scanning the full accumulated buffer every frame.
   */
  private staticHasWide = false;
  /** Width at which `staticHasWide` was last computed. -1 means stale. */
  private staticHasWideWidth = -1;
  /**
   * Cached sum of physical rows across `accumulatedStaticOutput` at
   * {@link staticHasWideWidth}. `-1` means stale (not yet computed or
   * invalidated). Lets the hot path compute total physical rows as
   * `staticPhysRowsCache + physRows(liveLines)` instead of walking the
   * full accumulated buffer.
   */
  private staticPhysRowsCache = -1;
  private staticScrollbackCap = 10_000;
  /**
   * When true, this renderer supports lines wider than terminal width
   * (soft-wrapped into multiple physical rows). Set via {@link TUIOptions.wideLines}.
   * When false, physical row math is skipped for performance.
   */
  private wideLinesEnabled = false;
  private onResizeCallbacks: (() => void)[] = [];
  /** Original stdout.write before interception; null when not intercepted. */
  private originalStdoutWrite: typeof process.stdout.write | null = null;
  /** True while twinki is writing to the terminal — suppresses external clear detection. */
  private internalWrite = false;
  private debugLogFd: number | null = null;
  private mouseListeners = new Set<(event: MouseEvent) => void>();
  private mouseEnabled = false;
  private mouseAllowed = false;
  private textSelectionEnabled = false;
  private textSelectionLines: string[] = [];
  private textSelectionRenderWidth = 0;
  private textSelectionAnchor: TextSelectionPoint | null = null;
  private textSelectionFocus: TextSelectionPoint | null = null;
  private textSelectionRawAnchor: { x: number; y: number } | null = null;
  private textSelectionScope: TextSelectionBounds | null = null;
  private textSelectionScopeResolver: TextSelectionScopeResolver | null = null;
  private textSelectionLiveStartRow = 0;
  private textSelectionActive = false;
  private textSelectionDragged = false;
  private textSelectionVisible = false;
  private selectionCopyListeners = new Set<(text: string) => void>();
  private suppressedClickEvents = new WeakSet<MouseEvent>();
  private pasteListeners = new Set<(content: string) => void>();
  private keyReleaseListeners = new Set<(data: string) => void>();
  private keyRepeatListeners = new Set<(data: string) => void>();
  private contentStartRow = -1;
  private liveContentPhysicalRow = 0;
  private dsrPending = false;
  private altScreen = false;
  /**
   * True once any frame has been committed. Full redraws before this point
   * (cold start, resumed-session history) must emit the entire frame; after
   * it, native-scrollback full redraws repaint only the viewport so committed
   * scrollback content is never duplicated or destroyed.
   */
  private hadFirstFrame = false;
  /** Tracks physical ownership while finalized rows replace an overflowing live region. */
  private pendingFlush: {
    /** Rows on screen at flush time (logical lines, marker-free). */
    visible: string[];
    /** Live-region index (old frame) of visible[0]. */
    visibleStartLive: number;
    /** Flushed static lines since capture; trailing run of the static buffer. */
    flushedCount: number;
    /** Ordered byte-identical runs tying finalized rows to prior live rows. */
    runs: Array<{
      flushStart: number;
      liveStart: number;
      len: number;
    }>;
    /** Wide line excluded from the capture because it straddled the viewport
     *  top; its painted rows were left in place as committed history. */
    straddled: string | null;
  } | null = null;
  private frameBudgetMs = 0;
  private lastRenderTime = 0;
  private pacingTimer: ReturnType<typeof setTimeout> | null = null;
  private scrollbarWidth = 0;
  private minWidth = 10;

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
    const opts =
      typeof showHardwareCursor === 'object'
        ? showHardwareCursor
        : { showHardwareCursor };
    if (opts.showHardwareCursor !== undefined) {
      this.showHardwareCursor = opts.showHardwareCursor;
    }
    if (opts.targetFps && opts.targetFps > 0) {
      // Only the derived per-frame budget is used downstream (frame pacing);
      // the raw fps isn't read anywhere else, so we don't retain it.
      this.frameBudgetMs = 1000 / opts.targetFps;
    }
    if (opts.fullscreen) {
      this.altScreen = true;
    }
    if (opts.mouse || opts.textSelection) {
      this.mouseAllowed = true;
    }
    if (opts.textSelection) {
      this.textSelectionEnabled = true;
    }
    if (opts.staticScrollbackCap != null && opts.staticScrollbackCap > 0) {
      this.staticScrollbackCap = opts.staticScrollbackCap;
    }
    if (opts.scrollbarWidth != null && opts.scrollbarWidth > 0) {
      this.scrollbarWidth = opts.scrollbarWidth;
    }
    if (opts.wideLines) {
      this.wideLinesEnabled = true;
    }
    if (opts.preserveScrollbackOnRedraw) {
      this.preserveScrollbackOnRedraw = true;
    }
    if (opts.synchronizedOutput !== undefined) {
      this.synchronizedOutput = opts.synchronizedOutput;
    }
    this.minWidth = Math.max(opts.minWidth ?? 10, 1);
    if (
      process.env.KIRO_RENDER_DEBUG === '1' ||
      process.env.KIRO_RENDER_DEBUG_FILE ||
      process.env.TWINKI_DEBUG_REDRAW === '1'
    ) {
      try {
        const fs = require('fs');
        const os = require('os');
        const path = require('path');
        const logPath =
          process.env.KIRO_RENDER_DEBUG_FILE ??
          path.join(os.tmpdir(), 'kiro-log', 'kiro-render-debug.log');
        const dir = path.dirname(logPath);
        fs.mkdirSync(dir, { recursive: true });
        this.debugLogFd = fs.openSync(logPath, 'a');
      } catch {
        /* ignore */
      }
    }
  }

  private debugLog(msg: string): void {
    if (this.debugLogFd == null) return;
    try {
      require('fs').writeSync(
        this.debugLogFd,
        `[${new Date().toISOString()}] ${msg}\n`
      );
    } catch {
      /* ignore */
    }
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

  onRenderComplete(
    listener: (event: RenderCompletedEvent) => void
  ): () => void {
    this.renderCompleteListeners.add(listener);
    return () => this.renderCompleteListeners.delete(listener);
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
   * Whether the terminal's own cursor is currently visible.
   *
   * The environment helper is only the starting point: this reflects a
   * constructor override or a later setShowHardwareCursor call.
   */
  get hardwareCursorVisible(): boolean {
    return this.showHardwareCursor;
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

  setPreserveScrollbackOnRedraw(enabled: boolean): void {
    this.preserveScrollbackOnRedraw = enabled;
  }

  setWideLinesEnabled(enabled: boolean): void {
    if (this.wideLinesEnabled === enabled) return;
    this.wideLinesEnabled = enabled;
    this.staticHasWideWidth = -1;
    this.staticPhysRowsCache = -1;
    this.requestRender(true);
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
    return () => {
      this.inputListeners.delete(listener);
    };
  }

  /**
   * Removes an input listener from the processing chain.
   *
   * @param listener - Listener function to remove
   */
  removeInputListener(listener: InputListener): void {
    this.inputListeners.delete(listener);
  }

  coalesceInputRenders(): void {
    this.deferInputRender = true;
    if (this.inputDeferralTimer) return;
    this.inputDeferralTimer = setImmediate(() => {
      this.inputDeferralTimer = null;
      this.deferInputRender = false;
    });
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

  /** Returns whether terminal mouse reporting is currently enabled. */
  isMouseEnabled(): boolean {
    return this.mouseEnabled;
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
      if (this.mouseListeners.size === 0 && !this.textSelectionEnabled) {
        this.disableMouse();
      }
    };
  }

  /** Returns whether text selection consumed this mouseup as a drag. */
  isClickSuppressed(event: MouseEvent): boolean {
    return this.suppressedClickEvents.has(event);
  }

  /** Cancels an armed or visible text selection without disabling selection. */
  cancelTextSelection(): void {
    const hadSelection = this.textSelectionVisible;
    this.textSelectionAnchor = null;
    this.textSelectionFocus = null;
    this.textSelectionRawAnchor = null;
    this.textSelectionScope = null;
    this.textSelectionActive = false;
    this.textSelectionDragged = false;
    this.textSelectionVisible = false;
    if (hadSelection) this.requestRender();
  }

  /**
   * Sets the layout resolver for declarative selection scopes. Points and
   * returned bounds are relative to live content, excluding static scrollback.
   */
  setTextSelectionScopeResolver(
    resolver: TextSelectionScopeResolver | null
  ): void {
    this.textSelectionScopeResolver = resolver;
  }

  /** Subscribes to non-empty selections copied on mouse release. */
  addSelectionCopyListener(listener: (text: string) => void): () => void {
    this.selectionCopyListeners.add(listener);
    return () => {
      this.selectionCopyListeners.delete(listener);
    };
  }

  /**
   * Adds a paste event listener.
   * @returns Unsubscribe function
   */
  addPasteListener(listener: (content: string) => void): () => void {
    this.pasteListeners.add(listener);
    return () => {
      this.pasteListeners.delete(listener);
    };
  }

  /**
   * Adds a key release listener. Receives raw data for key release events.
   * @returns Unsubscribe function
   */
  addKeyReleaseListener(listener: (data: string) => void): () => void {
    this.keyReleaseListeners.add(listener);
    return () => {
      this.keyReleaseListeners.delete(listener);
    };
  }

  /**
   * Adds a key repeat listener. Receives raw data for key repeat events.
   * @returns Unsubscribe function
   */
  addKeyRepeatListener(listener: (data: string) => void): () => void {
    this.keyRepeatListeners.add(listener);
    return () => {
      this.keyRepeatListeners.delete(listener);
    };
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

  /** Viewport row at which the live React layout starts. */
  getLiveContentYOffset(): number {
    return this.getContentYOffset() + this.liveContentPhysicalRow;
  }

  private resolveTextSelectionScope(
    point: TextSelectionPoint
  ): TextSelectionBounds | null {
    const resolver = this.textSelectionScopeResolver;
    const liveRow = point.row - this.textSelectionLiveStartRow;
    if (!resolver || liveRow < 0) return null;

    const bounds = resolver({ row: liveRow, column: point.column });
    if (!bounds || bounds.width <= 0 || bounds.height <= 0) return null;
    return {
      ...bounds,
      y: bounds.y + this.textSelectionLiveStartRow,
    };
  }

  private getTextSelectionPoint(event: MouseEvent): TextSelectionPoint | null {
    if (this.textSelectionLines.length === 0) return null;

    let physicalRow = event.y - this.getContentYOffset();
    if (physicalRow < 0) return null;

    const renderWidth = Math.max(this.textSelectionRenderWidth, 1);
    if (!this.wideLinesEnabled) {
      const row = Math.floor(physicalRow);
      const line = this.textSelectionLines[row];
      if (line === undefined) return null;
      return {
        row,
        column: Math.min(
          Math.max(0, Math.min(event.x, renderWidth - 1)),
          visibleWidth(line)
        ),
      };
    }

    for (let row = 0; row < this.textSelectionLines.length; row++) {
      const line = this.textSelectionLines[row] ?? '';
      const lineWidth = visibleWidth(line);
      const physicalRows = Math.max(1, Math.ceil(lineWidth / renderWidth));

      if (physicalRow < physicalRows) {
        const screenColumn = Math.max(0, Math.min(event.x, renderWidth - 1));
        const column = physicalRow * renderWidth + screenColumn;
        return { row, column: Math.min(column, lineWidth) };
      }
      physicalRow -= physicalRows;
    }

    return null;
  }

  private hasTextSelectionRange(): boolean {
    const anchor = this.textSelectionAnchor;
    const focus = this.textSelectionFocus;
    return (
      anchor !== null &&
      focus !== null &&
      (anchor.row !== focus.row || anchor.column !== focus.column)
    );
  }

  private handleTextSelectionMouse(event: MouseEvent): void {
    if (event.button !== 'left') return;

    if (event.type === 'mousedown') {
      const hadVisibleSelection = this.textSelectionVisible;
      const rawPoint = this.getTextSelectionPoint(event);
      const scope = rawPoint ? this.resolveTextSelectionScope(rawPoint) : null;
      const point =
        rawPoint && scope ? clampTextSelectionPoint(rawPoint, scope) : rawPoint;
      this.textSelectionAnchor = point;
      this.textSelectionFocus = point;
      this.textSelectionScope = scope;
      this.textSelectionRawAnchor = point ? { x: event.x, y: event.y } : null;
      this.textSelectionActive = point !== null;
      this.textSelectionDragged = false;
      this.textSelectionVisible = false;
      if (hadVisibleSelection) this.requestRender();
      return;
    }

    if (
      !this.textSelectionActive ||
      (event.type !== 'mousemove' && event.type !== 'mouseup')
    ) {
      return;
    }

    const rawAnchor = this.textSelectionRawAnchor;
    if (rawAnchor && (event.x !== rawAnchor.x || event.y !== rawAnchor.y)) {
      this.textSelectionDragged = true;
    }

    const rawPoint = this.getTextSelectionPoint(event);
    const point =
      rawPoint && this.textSelectionScope
        ? clampTextSelectionPoint(rawPoint, this.textSelectionScope)
        : rawPoint;
    const focusChanged =
      point !== null &&
      (this.textSelectionFocus === null ||
        point.row !== this.textSelectionFocus.row ||
        point.column !== this.textSelectionFocus.column);
    if (point) this.textSelectionFocus = point;

    const wasVisible = this.textSelectionVisible;
    this.textSelectionVisible =
      this.textSelectionDragged && this.hasTextSelectionRange();

    if (event.type === 'mousemove') {
      if (focusChanged || wasVisible !== this.textSelectionVisible) {
        this.requestRender();
      }
      return;
    }

    this.textSelectionActive = false;
    if (this.textSelectionDragged) {
      this.suppressedClickEvents.add(event);
      if (
        this.textSelectionAnchor &&
        this.textSelectionFocus &&
        this.hasTextSelectionRange()
      ) {
        const selectedText = extractSelectedText(
          this.textSelectionLines,
          this.textSelectionAnchor,
          this.textSelectionFocus,
          this.textSelectionScope ?? undefined
        );
        if (selectedText.length > 0) {
          this.terminal.write(osc52ClipboardSequence(selectedText));
          for (const listener of this.selectionCopyListeners) {
            listener(selectedText);
          }
        }
      }
    }

    if (focusChanged || wasVisible !== this.textSelectionVisible) {
      this.requestRender();
    }
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

    // Mouse events may arrive batched by the terminal adapter.
    const mouseEvents = this.mouseEnabled ? parseSGRMouseEvents(data) : null;
    if (mouseEvents) {
      for (const event of mouseEvents) {
        if (this.textSelectionEnabled) {
          this.handleTextSelectionMouse(event);
        }
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
      if (
        this.focusedComponent?.handleInput &&
        this.focusedComponent.wantsKeyRelease
      ) {
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
    const focusedOverlay = this.overlayStack.find(
      (o) => o.component === this.focusedComponent
    );
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

    const doResize = () => {
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
    };

    // Throttle resize to at most once per 100ms (leading + trailing).
    // Leading fires immediately so the screen clears without flash.
    // Trailing fires after drag stops to lock in final dimensions.
    const throttledResize = throttle(doResize, 100, {
      leading: true,
      trailing: true,
    });

    this.terminal.start((data) => this.handleInput(data), throttledResize);
    if (this.textSelectionEnabled) {
      this.enableMouse();
    }
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
    this.invalidateFrame();
    this.previousWidth = -1;
    this.cursorRow = 0;
    this.hardwareCursorRow = 0;
    this.maxLinesRendered = 0;
    this.previousViewportTop = 0;
    this.ownedTopRow = 0;
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
    if (this.inputDeferralTimer) {
      clearImmediate(this.inputDeferralTimer);
      this.inputDeferralTimer = null;
    }
    this.deferInputRender = false;
    if (this.mouseEnabled) this.disableMouse();
    if (this.altScreen) {
      this.exitAltScreen();
    } else if (this.previousLines.length > 0) {
      // Move cursor past the rendered content. Use physical row count when
      // wide lines are enabled (wrap="overflow" soft-wraps into multiple
      // terminal rows); otherwise logical === physical.
      let targetRow: number;
      if (this.wideLinesEnabled) {
        const width = Math.max(this.terminal.columns || 80, this.minWidth);
        let physRows = 0;
        for (const line of this.previousLines) {
          const vw = visibleWidth(line);
          physRows += vw <= width ? 1 : Math.ceil(vw / width);
        }
        targetRow = physRows;
      } else {
        targetRow = this.previousLines.length;
      }
      const diff = targetRow - this.hardwareCursorRow;
      if (diff > 0) this.terminal.write(`\x1b[${diff}B`);
      else if (diff < 0) this.terminal.write(`\x1b[${-diff}A`);
      this.terminal.write('\r\n');
    }
    // Clear internal collections to prevent memory retention after stop
    this.invalidateFrame();
    this.staticBuffer.clear();
    this.staticHasWide = false;
    this.staticHasWideWidth = -1;
    this.staticPhysRowsCache = -1;
    this.overlayStack = [];
    this.onResizeCallbacks.length = 0;
    this.renderCompleteListeners.clear();
    this.inputListeners.clear();
    this.mouseListeners.clear();
    this.textSelectionLines = [];
    this.textSelectionAnchor = null;
    this.textSelectionFocus = null;
    this.textSelectionRawAnchor = null;
    this.textSelectionScope = null;
    this.textSelectionScopeResolver = null;
    this.textSelectionLiveStartRow = 0;
    this.textSelectionActive = false;
    this.textSelectionDragged = false;
    this.textSelectionVisible = false;
    this.textSelectionRenderWidth = 0;
    this.liveContentPhysicalRow = 0;
    this.suppressedClickEvents = new WeakSet<MouseEvent>();
    this.selectionCopyListeners.clear();
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
      this.invalidateFrame();
      this.previousWidth = -1;
      this.cursorRow = 0;
      this.hardwareCursorRow = 0;
      this.maxLinesRendered = 0;
      this.previousViewportTop = 0;
      this.ownedTopRow = 0;
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

    // Input can synchronously update local and external stores; paint them together.
    if (this.frameBudgetMs <= 0) {
      const render = () => {
        this.renderRequested = false;
        this.doRender();
      };
      if (this.deferInputRender) setImmediate(render);
      else process.nextTick(render);
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
    termHeight: number
  ): {
    width: number;
    row: number;
    col: number;
    maxHeight: number | undefined;
  } {
    const opt = options ?? {};
    const margin =
      typeof opt.margin === 'number'
        ? {
            top: opt.margin,
            right: opt.margin,
            bottom: opt.margin,
            left: opt.margin,
          }
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
    if (maxHeight !== undefined)
      maxHeight = Math.max(1, Math.min(maxHeight, availH));

    const effH =
      maxHeight !== undefined
        ? Math.min(overlayHeight, maxHeight)
        : overlayHeight;
    const anchor = opt.anchor ?? 'center';

    let row: number;
    if (opt.row !== undefined) {
      if (typeof opt.row === 'string') {
        const m = opt.row.match(/^(\d+(?:\.\d+)?)%$/);
        if (m) {
          row =
            mT +
            Math.floor((Math.max(0, availH - effH) * parseFloat(m[1]!)) / 100);
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
          col =
            mL +
            Math.floor((Math.max(0, availW - width) * parseFloat(m[1]!)) / 100);
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
  private resolveAnchorRow(
    anchor: OverlayAnchor,
    h: number,
    availH: number,
    mT: number
  ): number {
    switch (anchor) {
      case 'top-left':
      case 'top-center':
      case 'top-right':
        return mT;
      case 'bottom-left':
      case 'bottom-center':
      case 'bottom-right':
        return mT + availH - h;
      default:
        return mT + Math.floor((availH - h) / 2);
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
  private resolveAnchorCol(
    anchor: OverlayAnchor,
    w: number,
    availW: number,
    mL: number
  ): number {
    switch (anchor) {
      case 'top-left':
      case 'left-center':
      case 'bottom-left':
        return mL;
      case 'top-right':
      case 'right-center':
      case 'bottom-right':
        return mL + availW - w;
      default:
        return mL + Math.floor((availW - w) / 2);
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
  private compositeOverlays(
    lines: string[],
    termWidth: number,
    termHeight: number
  ): string[] {
    if (this.overlayStack.length === 0) return lines;
    const result = [...lines];

    const rendered: {
      overlayLines: string[];
      row: number;
      col: number;
      w: number;
    }[] = [];
    let minLinesNeeded = result.length;

    for (const entry of this.overlayStack) {
      if (!this.isOverlayVisible(entry)) continue;
      const { width, maxHeight } = this.resolveOverlayLayout(
        entry.options,
        0,
        termWidth,
        termHeight
      );
      let overlayLines = entry.component.render(width);
      if (maxHeight !== undefined && overlayLines.length > maxHeight) {
        overlayLines = overlayLines.slice(0, maxHeight);
      }
      const { row, col } = this.resolveOverlayLayout(
        entry.options,
        overlayLines.length,
        termWidth,
        termHeight
      );
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
          const truncated =
            visibleWidth(overlayLines[i]!) > w
              ? sliceByColumn(overlayLines[i]!, 0, w, true)
              : overlayLines[i]!;
          result[idx] = this.compositeLineAt(
            result[idx]!,
            truncated,
            col,
            w,
            termWidth
          );
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
    totalWidth: number
  ): string {
    const afterStart = startCol + overlayWidth;
    const base = extractSegments(
      baseLine,
      startCol,
      afterStart,
      totalWidth - afterStart,
      true
    );
    const overlay = sliceWithWidth(overlayLine, 0, overlayWidth, true);

    const beforePad = Math.max(0, startCol - base.beforeWidth);
    const overlayPad = Math.max(0, overlayWidth - overlay.width);
    const actualBeforeW = Math.max(startCol, base.beforeWidth);
    const actualOverlayW = Math.max(overlayWidth, overlay.width);
    const afterTarget = Math.max(
      0,
      totalWidth - actualBeforeW - actualOverlayW
    );
    const afterPad = Math.max(0, afterTarget - base.afterWidth);

    const r = TUI.SEGMENT_RESET;
    const result =
      base.before +
      ' '.repeat(beforePad) +
      r +
      overlay.text +
      ' '.repeat(overlayPad) +
      r +
      base.after +
      ' '.repeat(afterPad);

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
   * @param minRow - Lowest index the above-viewport cleanup scan may reach.
   *   Callers pass the accumulated-static prefix length; those lines are
   *   marker-free by construction (scrubbed in writeStaticLines).
   * @returns Cursor position or null if no cursor found
   */
  private extractCursorPosition(
    lines: string[],
    height: number,
    minRow = 0
  ): { row: number; col: number } | null {
    const viewportTop = Math.max(0, lines.length - height);

    // Fast path: scan viewport (where the cursor almost always is)
    for (let row = lines.length - 1; row >= viewportTop; row--) {
      const line = lines[row]!;
      const idx = line.indexOf(CURSOR_MARKER);
      if (idx !== -1) {
        const col = visibleWidth(line.slice(0, idx));
        let stripped =
          line.slice(0, idx) + line.slice(idx + CURSOR_MARKER.length);
        // The visible hardware cursor inverts the marker cell on its own; a
        // software inverse painted there cancels it and the cursor vanishes.
        if (this.showHardwareCursor) {
          stripped = uninvertCursorCell(stripped, idx);
        }
        lines[row] = stripped;
        return { row, col };
      }
    }

    // Cleanup: strip any marker above viewport so it never leaks to terminal.
    // Bounded below by `minRow`: the accumulated-static prefix is scrubbed of
    // markers once at append time (writeStaticLines), so rescanning it every
    // frame is O(static lines) of pure waste.
    this.perfLastFrame.cursorScanned = Math.max(0, viewportTop - minRow);
    for (let row = viewportTop - 1; row >= minRow; row--) {
      const idx = lines[row]!.indexOf(CURSOR_MARKER);
      if (idx !== -1) {
        lines[row] =
          lines[row]!.slice(0, idx) +
          lines[row]!.slice(idx + CURSOR_MARKER.length);
        break; // only one marker exists
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
   * @param startIndex - First index to process. The accumulated-static prefix
   *   is reset once at append time ({@link writeStaticLines}), so the per-frame
   *   path skips it — rewriting those strings every frame would allocate a new
   *   string per static line and destroy the reference identity the
   *   differential diff relies on.
   * @returns Processed lines with reset sequences
   */
  private applyLineResets(lines: string[], startIndex = 0): string[] {
    const reset = TUI.SEGMENT_RESET;
    this.perfLastFrame.resetsApplied = Math.max(0, lines.length - startIndex);
    for (let i = startIndex; i < lines.length; i++) {
      lines[i] = lines[i]! + reset;
    }
    return lines;
  }

  /**
   * Appends the line-reset suffix to a copy of `lines`, scrubbing any cursor
   * marker. Static lines are committed content: the cursor never belongs to
   * them, and scrubbing here lets the per-frame cursor scan stop at the end of
   * the prefix instead of walking it every frame.
   */
  private finalizeStaticLines(lines: string[]): string[] {
    const reset = TUI.SEGMENT_RESET;
    const out = new Array<string>(lines.length);
    for (let i = 0; i < lines.length; i++) {
      const line = lines[i]!;
      const idx = line.indexOf(CURSOR_MARKER);
      out[i] =
        (idx === -1
          ? line
          : line.slice(0, idx) + line.slice(idx + CURSOR_MARKER.length)) +
        reset;
    }
    return out;
  }

  /**
   * Longest contiguous run of `flushed` lines appearing, in order, inside
   * `live` — evidence those flushed lines were already physically presented
   * in their streamed form. Runs are seeded only on non-blank lines so a
   * coincidental blank/blank match can never anchor one; blanks join a run
   * only by extension from a corroborating non-blank anchor. Seeds and
   * per-line occurrences are capped, keeping the search linear in practice:
   * the true alignment is anchored by the first painted non-blank line.
   */
  private findPresentedRun(
    flushed: string[],
    live: string[]
  ): { flushStart: number; liveStart: number; len: number } {
    const best = { flushStart: 0, liveStart: 0, len: 0 };
    if (flushed.length === 0 || live.length === 0) return best;
    const occurrences = new Map<string, number[]>();
    for (let k = 0; k < live.length; k++) {
      const line = live[k]!;
      if (visibleWidth(line) === 0) continue;
      let arr = occurrences.get(line);
      if (!arr) occurrences.set(line, (arr = []));
      if (arr.length < 8) arr.push(k);
    }
    let seeds = 0;
    for (let j = 0; j < flushed.length && seeds < 8; j++) {
      const line = flushed[j]!;
      if (visibleWidth(line) === 0) continue;
      seeds++;
      const occ = occurrences.get(line);
      if (!occ) continue;
      for (const k of occ) {
        let back = 0;
        while (
          j - back - 1 >= 0 &&
          k - back - 1 >= 0 &&
          flushed[j - back - 1] === live[k - back - 1]
        ) {
          back++;
        }
        let fwd = 1;
        while (
          j + fwd < flushed.length &&
          k + fwd < live.length &&
          flushed[j + fwd] === live[k + fwd]
        ) {
          fwd++;
        }
        const len = back + fwd;
        if (len > best.len) {
          best.len = len;
          best.flushStart = j - back;
          best.liveStart = k - back;
          if (best.len === flushed.length) return best;
        }
      }
    }
    return best;
  }

  /** Finds ordered matching runs on both sides of each divergent boundary. */
  private findPresentedRuns(
    flushed: string[],
    live: string[]
  ): Array<{ flushStart: number; liveStart: number; len: number }> {
    const runs: Array<{
      flushStart: number;
      liveStart: number;
      len: number;
    }> = [];
    const ranges = [
      {
        flushStart: 0,
        flushEnd: flushed.length,
        liveStart: 0,
        liveEnd: live.length,
      },
    ];

    // Bound fragmented divergence work; unmatched fragments use lossless fallback emission.
    while (ranges.length > 0 && runs.length < 32) {
      const range = ranges.pop()!;
      const match = this.findPresentedRun(
        flushed.slice(range.flushStart, range.flushEnd),
        live.slice(range.liveStart, range.liveEnd)
      );
      if (match.len === 0) continue;

      const run = {
        flushStart: range.flushStart + match.flushStart,
        liveStart: range.liveStart + match.liveStart,
        len: match.len,
      };
      runs.push(run);

      const flushAfter = run.flushStart + run.len;
      const liveAfter = run.liveStart + run.len;
      if (flushAfter < range.flushEnd && liveAfter < range.liveEnd) {
        ranges.push({
          flushStart: flushAfter,
          flushEnd: range.flushEnd,
          liveStart: liveAfter,
          liveEnd: range.liveEnd,
        });
      }
      if (
        range.flushStart < run.flushStart &&
        range.liveStart < run.liveStart
      ) {
        ranges.push({
          flushStart: range.flushStart,
          flushEnd: run.flushStart,
          liveStart: range.liveStart,
          liveEnd: run.liveStart,
        });
      }
    }

    runs.sort((a, b) => a.flushStart - b.flushStart);
    return runs;
  }

  /**
   * Records `lines` as the shadow buffer that the next frame diffs against,
   * together with the static-buffer generation it was rendered from.
   */
  private commitFrame(
    lines: string[],
    hasWide: boolean,
    physRows: number,
    width: number,
    staticPrefixPristine: boolean
  ): void {
    this.previousLines = lines;
    this.previousHasWide = hasWide;
    this.previousPhysRowsCache = physRows;
    this.previousWidth = width;
    this.hadFirstFrame = true;
    this.frameGeneration = staticPrefixPristine
      ? this.staticBuffer.generation
      : -1;
  }

  /**
   * Drops the shadow buffer, forcing the next render down a full-repaint path.
   * Used whenever the terminal stops matching what we believe is on screen:
   * clears, resize, suspend, external writes.
   */
  private invalidateFrame(): void {
    this.previousLines = [];
    this.previousHasWide = false;
    this.previousPhysRowsCache = 0;
    this.frameGeneration = -1;
  }

  /**
   * Adds static lines to the accumulated static output.
   * These lines are rendered above live content and scroll into terminal scrollback.
   */
  writeStaticLines(lines: string[]): void {
    if (lines.length > 0 && !this.altScreen) {
      // Apply the line-reset suffix ONCE, here, instead of on every frame.
      // These strings are then final: the per-frame path neither rewrites nor
      // re-diffs them, so the prefix costs nothing per frame.
      // When the fast path is off, store lines verbatim so the per-frame
      // reset pass produces byte-identical output to the unoptimized path.
      const finalized = this.finalizeStaticLines(lines);
      this.staticBuffer.append(finalized);
      // A flush arriving after the capture (screen already erased, nothing
      // new to see) still grows the trailing run the repaint must account
      // for; without accumulating, the frame-index range below drifts onto
      // the wrong lines.
      if (this.pendingFlush) {
        this.pendingFlush.flushedCount += finalized.length;
      }
      // Update cached staticHasWide/staticPhysRowsCache incrementally — only
      // scan the NEW lines, not the whole accumulated buffer. If wide mode
      // is off or our cached width is stale, skip (will be rebuilt next render).
      if (this.wideLinesEnabled && this.staticHasWideWidth > 0) {
        const width = this.staticHasWideWidth;
        for (const l of lines) {
          const vw = visibleWidth(l);
          const rows = vw <= width ? 1 : Math.ceil(vw / width);
          if (rows > 1) this.staticHasWide = true;
          if (this.staticPhysRowsCache >= 0) {
            this.staticPhysRowsCache += rows;
          }
        }
      }
      this.trimStaticOutput();
      // When live content (excluding static) overflowed the viewport, the old
      // active rows above the viewport are stuck in scrollback where they can
      // no longer be updated. With scrollback preservation on, capture the
      // visible rows and erase the viewport; the tail repaint scrolls into
      // scrollback only the captured rows it does not re-present. Otherwise
      // \x1b[3J drops everything, which costs the user their scroll position
      // but is the only way to repaint rows a tall element spans.
      // "Live rows" = physical rows of the active tail (all lines in
      // `previousLines` after the accumulated static prefix). When wide
      // lines aren't enabled, physical === logical (one row per line).
      const staticLogicalCount = this.staticBuffer.length - lines.length;
      let liveRows: number;
      if (this.wideLinesEnabled) {
        const width = Math.max(this.terminal.columns || 80, this.minWidth);
        const rowOf = (line: string) => {
          const vw = visibleWidth(line);
          return vw <= width ? 1 : Math.ceil(vw / width);
        };
        liveRows = 0;
        for (let i = staticLogicalCount; i < this.previousLines.length; i++) {
          liveRows += rowOf(this.previousLines[i] ?? '');
        }
      } else {
        liveRows = this.previousLines.length - staticLogicalCount;
      }
      if (liveRows > this.terminal.rows && !this.altScreen) {
        if (this.preserveScrollbackOnRedraw && !this.synchronizedOutput) {
          // Avoid replaying immutable history without atomic output. The next
          // differential render reconciles the mutable tail.
          return;
        }
        // Rows above `owned` are committed history a previous short paint
        // left painted; the cursor offset and every erase bound below are
        // measured from this origin so neither climbs into them.
        const owned = Math.max(this.previousViewportTop, this.ownedTopRow);
        const screenRow = this.hardwareCursorRow - owned;
        let rowsToErase = Math.min(screenRow + 1, this.terminal.rows);
        if (this.preserveScrollbackOnRedraw && !this.pendingFlush) {
          // Capture the rows on screen and match the flushed lines against
          // the old live region. The repaint emits each piece of content
          // exactly once: erasing without capturing lost it; emitting the
          // flushed lines unconditionally re-emitted everything the user
          // already watched stream (whole-turn duplication).
          const capRowOf = (line: string): number => {
            if (!this.wideLinesEnabled) return 1;
            const w = Math.max(this.terminal.columns || 80, this.minWidth);
            const vw = visibleWidth(line);
            return vw <= w ? 1 : Math.ceil(vw / w);
          };
          const visible: string[] = [];
          let seen = 0;
          let firstVisibleIdx = this.previousLines.length;
          let straddled: string | null = null;
          // Bound the walk by the frame rows physically on screen, measured
          // from the frame end where the walk starts. The erase's final
          // \x1b[J runs to the bottom of the screen regardless of where the
          // cursor is parked (it may sit above trailing chrome), so a
          // cursor-measured bound would leave the oldest on-screen rows
          // erased but uncaptured.
          const prevPhys = this.wideLinesEnabled
            ? this.previousPhysRowsCache
            : this.previousLines.length;
          const onScreenRows = Math.max(
            0,
            Math.min(this.terminal.rows, prevPhys - owned)
          );
          for (
            let i = this.previousLines.length - 1;
            i >= 0 && seen < onScreenRows;
            i--
          ) {
            const line = this.previousLines[i] ?? '';
            const rows = capRowOf(line);
            // A wide line straddling the viewport top stays committed
            // whole: its on-screen rows are left painted (the erase below
            // stops under them) and scroll off as immutable history.
            // Capturing it whole instead would strand its already-scrolled
            // top rows as a cut copy in scrollback and re-emit the full
            // row below them, duplicating and misordering the block.
            if (seen + rows > onScreenRows) {
              straddled = line;
              break;
            }
            seen += rows;
            visible.unshift(line);
            firstVisibleIdx = i;
          }
          // Both sides carry the reset suffix and are marker-free, so the
          // run matcher compares committed strings directly.
          const liveOld = this.previousLines.slice(staticLogicalCount);
          const runs = this.findPresentedRuns(finalized, liveOld);
          this.pendingFlush = {
            visible,
            visibleStartLive: Math.max(0, firstVisibleIdx - staticLogicalCount),
            flushedCount: finalized.length,
            runs,
            straddled,
          };
          // Erase exactly the captured rows. The captured window ends at
          // the frame's last row while the per-row erase walks up from the
          // cursor, which may be parked above trailing chrome: the rows
          // between cursor and frame end are covered by the closing \x1b[J,
          // so only the captured rows at or above the cursor need walking.
          const cursorGap = Math.max(0, prevPhys - 1 - this.hardwareCursorRow);
          rowsToErase = Math.max(
            0,
            Math.min(rowsToErase, seen - cursorGap)
          );
        }
        let buf = this.preserveScrollbackOnRedraw ? '' : '\x1b[3J';
        for (let i = 0; i < rowsToErase; i++) {
          buf += '\x1b[2K' + (i < rowsToErase - 1 ? '\x1b[1A' : '');
        }
        buf += '\r\x1b[J';
        this.terminal.write(buf);
        this.invalidateFrame();
        this.hardwareCursorRow = 0;
        this.cursorRow = 0;
        this.maxLinesRendered = 0;
        this.previousViewportTop = 0;
        this.ownedTopRow = 0;
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
    if (this.staticBuffer.trimTo(this.staticScrollbackCap)) {
      // We may have dropped the only wide lines, so the cached wide flag and
      // physical-row sum no longer describe the buffer.
      this.staticHasWideWidth = -1;
      this.staticPhysRowsCache = -1;
    }
  }

  /**
   * Replaces all accumulated static output. Used on resize to re-render
   * static content at the new width without duplication.
   */
  replaceStaticOutput(lines: string[]): void {
    // Resets applied once here — see writeStaticLines.
    this.staticBuffer.replace(this.finalizeStaticLines(lines));
    this.staticHasWideWidth = -1;
    this.staticPhysRowsCache = -1;
    this.trimStaticOutput();
  }

  /**
   * Clears accumulated static output. Called on resize so static content
   * can be re-rendered at the new width.
   */
  resetStaticOutput(): void {
    this.staticBuffer.clear();
    this.staticHasWide = false;
    this.staticHasWideWidth = -1;
    this.staticPhysRowsCache = -1;
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

    process.stdout.write = function (chunk: any, ...args: any[]): boolean {
      if (
        !self.stopped &&
        !self.internalWrite &&
        typeof chunk === 'string' &&
        (chunk.includes('\x1b[2J') || chunk.includes('\x1b[3J'))
      ) {
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
      process.stdout.write = this
        .originalStdoutWrite as typeof process.stdout.write;
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
    this.invalidateFrame();
    this.maxLinesRendered = 0;
    this.hardwareCursorRow = 0;
    this.cursorRow = 0;
    this.previousViewportTop = 0;
    this.ownedTopRow = 0;
    this.staticBuffer.clear();
    this.staticHasWide = false;
    this.staticHasWideWidth = -1;
    this.staticPhysRowsCache = -1;
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
    let completed = false;
    // Mark as internal write so our stdout interceptor ignores any clear
    // sequences emitted by the render strategies (e.g. Strategy 2 CLEAR_ALL).
    this.internalWrite = true;
    try {
      this._doRenderInner();
      completed = true;
    } finally {
      this.internalWrite = false;
      const elapsed = performance.now() - renderStart;
      this.perfLastRenderMs = elapsed;
      this.perfTotalRenderMs += elapsed;
      this.perfRenderCount++;
      if (elapsed > this.perfMaxRenderMs) this.perfMaxRenderMs = elapsed;
      if (completed && this.renderCompleteListeners.size > 0) {
        const event: RenderCompletedEvent = {
          durationMs: elapsed,
          kind: this.lastRenderKind,
          frameRowsCurrent: !this.paintedRowsStale,
        };
        for (const listener of this.renderCompleteListeners) {
          try {
            listener(event);
          } catch {
            // Observers cannot interrupt terminal rendering.
          }
        }
      }
      this.debugLog(
        `perf: ${elapsed.toFixed(2)}ms render=#${this.perfRenderCount} fullRedraw=${this.fullRedrawCount} max=${this.perfMaxRenderMs.toFixed(2)}ms`
      );
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
    this.lastRenderKind = 'partial';
    this.perfLastFrame.prefixCopied = 0;
    const width = Math.max(
      this.terminal.columns - this.scrollbarWidth,
      this.minWidth
    );
    const height = this.terminal.rows;

    /**
     * Row math helpers.
     *
     * Logical lines (stored in `previousLines`, `newLines`,
     * `accumulatedStaticOutput`) may exceed `width` when a component uses
     * `wrap="overflow"`. The terminal soft-wraps such lines visually into
     * multiple physical rows. Cursor positioning, viewport math, and row
     * counting MUST operate on physical rows to stay aligned with the
     * terminal. These helpers convert between logical indices and physical
     * row counts.
     *
     * Writing to the terminal uses the ORIGINAL logical content (single
     * write per logical line) so the terminal's native soft-wrap preserves
     * the line as one logical unit for copy-paste.
     */
    /**
     * Per-render memoization of `rowOf`. The diff loop, physRowOf helpers,
     * and anyWideChange scan all call `rowOf` on the same line multiple
     * times per render; caching avoids redundant `visibleWidth` work.
     * Scoped to a single _doRenderInner call.
     */
    const rowOfCache = new Map<string, number>();
    const rowOf = (line: string): number => {
      const cached = rowOfCache.get(line);
      if (cached !== undefined) return cached;
      const vw = visibleWidth(line);
      const r = vw <= width ? 1 : Math.ceil(vw / width);
      rowOfCache.set(line, r);
      return r;
    };
    /**
     * Fast check: does any line in `lines` soft-wrap at the current width?
     * Quick byte-length check short-circuits lines that clearly fit.
     * Uses `rowOf` (memoized) for the precise check so later helpers see
     * the cached width.
     */
    const hasAnyWideLine = (lines: string[]): boolean => {
      for (const l of lines) {
        if (l.length > width && rowOf(l) > 1) return true;
      }
      return false;
    };
    /** Total physical rows across a logical line array. */
    const physRows = (lines: string[], knownWide?: boolean): number => {
      if (knownWide === false) return lines.length;
      let total = 0;
      for (const l of lines) total += rowOf(l);
      return total;
    };
    /** Physical row where logical line at index `i` starts. */
    const physRowOf = (
      lines: string[],
      i: number,
      knownWide?: boolean
    ): number => {
      if (knownWide === false) return Math.min(i, lines.length);
      let row = 0;
      for (let k = 0; k < i && k < lines.length; k++) row += rowOf(lines[k]!);
      return row;
    };

    let viewportTop = Math.max(0, this.maxLinesRendered - height);
    let prevViewportTop = this.previousViewportTop;
    let hardwareCursorRow = this.hardwareCursorRow;

    const computeLineDiff = (targetPhysRow: number): number => {
      const currentScreenRow = hardwareCursorRow - prevViewportTop;
      const targetScreenRow = targetPhysRow - viewportTop;
      return targetScreenRow - currentScreenRow;
    };

    let newLines = this.render(width);

    // Track live portion separately — used to compute `newHasWide` cheaply
    // when the static prefix has a cached wide flag.
    const liveLines = newLines;
    const staticPrefixLen =
      this.staticBuffer.length > 0 && !this.altScreen
        ? this.staticBuffer.length
        : 0;
    const hasVisibleOverlay = this.hasOverlay();
    this.textSelectionLiveStartRow = staticPrefixLen;

    // OPTIMIZED: Combine accumulated static output with live content
    // Skip in alt screen — no scrollback buffer to display static content in.
    if (this.staticBuffer.length > 0 && !this.altScreen) {
      const frame = this.staticBuffer.compose(newLines, this.previousLines);
      newLines = frame.lines;
      this.perfLastFrame.prefixCopied = frame.copiedPrefixLines;
    }

    if (hasVisibleOverlay) {
      newLines = this.compositeOverlays(newLines, width, height);
    }

    // Clip to terminal height in alt-screen mode (prevents scroll desync).
    if (this.altScreen && newLines.length > height) {
      newLines = newLines.slice(0, height);
    }

    // Refresh the static-prefix physical-row caches if stale. Done BEFORE
    // cursor physicalization so the cursor fast-path can use them.
    if (
      this.wideLinesEnabled &&
      (this.staticHasWideWidth !== width || this.staticPhysRowsCache < 0)
    ) {
      let total = 0;
      let anyWide = false;
      for (const l of this.staticBuffer.view) {
        const vw = visibleWidth(l);
        const r = vw <= width ? 1 : Math.ceil(vw / width);
        total += r;
        if (r > 1) anyWide = true;
      }
      this.staticPhysRowsCache = total;
      this.staticHasWide = anyWide;
      this.staticHasWideWidth = width;
    }
    this.liveContentPhysicalRow = this.altScreen
      ? 0
      : this.wideLinesEnabled
        ? Math.max(0, this.staticPhysRowsCache)
        : staticPrefixLen;

    const cursorPos = this.extractCursorPosition(
      newLines,
      height,
      hasVisibleOverlay ? 0 : staticPrefixLen
    );
    // `cursorPos.row` is a LOGICAL line index into `newLines`. When lines
    // may soft-wrap (`wideLinesEnabled`), convert it to a physical row so
    // `positionHardwareCursor` — which operates on physical rows — lands
    // the cursor at the correct terminal row. Before this conversion,
    // wide (soft-wrapped) lines caused the hardware cursor to be placed
    // at a row ABOVE where twinki expected, which in turn made subsequent
    // differential writes (`\x1b[{n}A`/`B` + `\r` + `\x1b[2K`) overwrite
    // visible viewport rows belonging to OTHER logical lines — leaving
    // stale streaming content in terminal scrollback.
    //
    // When `wideLinesEnabled=false`, logical and physical rows are
    // always equal so no conversion is needed and this scan is skipped.
    if (cursorPos && this.wideLinesEnabled) {
      // Fast path: if cursor is in the LIVE portion (cursorPos.row >=
      // staticPrefixLen), we compute using the cached static phys rows
      // plus only a walk of the small live suffix up to cursor.row.
      // Otherwise fall back to the full O(n) scan.
      if (
        cursorPos.row >= staticPrefixLen &&
        this.staticPhysRowsCache >= 0 &&
        this.staticHasWideWidth === width
      ) {
        let row = this.staticPhysRowsCache;
        for (
          let k = 0;
          k < cursorPos.row - staticPrefixLen && k < liveLines.length;
          k++
        ) {
          row += rowOf(liveLines[k]!);
        }
        cursorPos.row = row;
      } else {
        cursorPos.row = physRowOf(newLines, cursorPos.row);
      }
    }
    if (this.textSelectionEnabled) {
      this.textSelectionLines = [...newLines];
      this.textSelectionRenderWidth = width;
      if (
        this.textSelectionVisible &&
        this.textSelectionAnchor &&
        this.textSelectionFocus
      ) {
        newLines = highlightTextSelection(
          newLines,
          this.textSelectionAnchor,
          this.textSelectionFocus,
          this.textSelectionScope ?? undefined
        );
      }
    }

    // The accumulated-static prefix already carries its reset suffix (applied
    // in writeStaticLines), so only the live suffix needs resetting. Skipping
    // it keeps the prefix strings reference-identical to `previousLines`,
    // which is what lets the diff below skip them.
    //
    // Overlays and selections that touch static rows rebuild prefix lines, so
    // fall back to resetting and diffing the complete frame.
    const selectionTouchesStaticPrefix =
      this.textSelectionVisible &&
      this.textSelectionAnchor !== null &&
      this.textSelectionFocus !== null &&
      Math.min(this.textSelectionAnchor.row, this.textSelectionFocus.row) <
        staticPrefixLen;
    const prefixIsPristine =
      staticPrefixLen > 0 &&
      !hasVisibleOverlay &&
      !selectionTouchesStaticPrefix;
    newLines = this.applyLineResets(
      newLines,
      prefixIsPristine ? staticPrefixLen : 0
    );

    const widthChanged =
      this.previousWidth !== 0 && this.previousWidth !== width;

    // Physical row totals — these represent what the terminal actually
    // displays once soft-wrap is taken into account. When no lines are
    // wider than `width`, physical == logical and we can avoid O(n) work
    // per render. We only scan for wide lines if the user explicitly
    // enabled `wideLines` — otherwise every line is assumed one physical
    // row (matches baseline semantics and zero perf overhead).
    //
    // The static-prefix caches (`staticHasWide`, `staticPhysRowsCache`)
    // were refreshed above if stale — here we only scan the small LIVE
    // portion.
    let newHasWide = false;
    let newPhysRows: number;
    if (this.wideLinesEnabled) {
      const liveHasWide = hasAnyWideLine(liveLines);
      newHasWide = this.staticHasWide || liveHasWide;
      // Compute live physical rows inline (usually a small array).
      let livePhys = 0;
      if (liveHasWide) {
        for (const l of liveLines) livePhys += rowOf(l);
      } else {
        livePhys = liveLines.length;
      }
      newPhysRows = this.staticPhysRowsCache + livePhys;
    } else {
      newPhysRows = newLines.length;
    }
    const prevHasWide = this.previousHasWide;
    const prevPhysRows = this.previousPhysRowsCache;

    /**
     * Physical row where logical line at index `i` starts, for the
     * `newLines` array specifically. Uses the cached static-prefix
     * physical-row sum when `i` lies past the static prefix, so each
     * query only walks the (small) live suffix instead of the whole
     * accumulated buffer.
     *
     * Only valid for `newLines` — use {@link physRowOf} for other arrays.
     */
    const physRowOfNew = (i: number): number => {
      if (!newHasWide) return Math.min(i, newLines.length);
      if (i >= staticPrefixLen) {
        let row = this.staticPhysRowsCache;
        const liveIdx = Math.min(i - staticPrefixLen, liveLines.length);
        for (let k = 0; k < liveIdx; k++) row += rowOf(liveLines[k]!);
        return row;
      }
      return physRowOf(newLines, i, newHasWide);
    };

    // Debug logging
    this.debugLog(
      `render: lines=${newLines.length} prev=${this.previousLines.length} width=${width} widthChanged=${widthChanged}`
    );

    /**
     * Writes all lines to the terminal as a single synchronized frame.
     *
     * @param clearSeq - ANSI sequence to prepend before content:
     *   - '' (empty): first inline render, no clearing
     *   - '\x1b[3J\x1b[2J\x1b[H': full clear including scrollback (width change, shrink)
     *   - '\x1b[2J\x1b[H': clear visible screen only, preserve scrollback (alt screen)
     */
    const fullRender = (clearSeq: string, reason?: string): void => {
      this.fullRedrawCount++;
      this.lastRenderKind = 'full';
      this.paintedRowsStale = false;
      this.debugLog(
        `fullRedraw #${this.fullRedrawCount}: reason=${reason ?? 'unknown'} lines=${newLines.length}`
      );
      const sync = this.emitBsu;
      let buffer = (sync ? '\x1b[?2026h' : '') + clearSeq;
      // Write each logical line, separated by \r\n. Lines wider than `width`
      // are written as-is — the terminal soft-wraps them into multiple rows
      // visually, preserving single-line semantics for copy-paste.
      for (let i = 0; i < newLines.length; i++) {
        if (i > 0) buffer += '\r\n';
        buffer += newLines[i];
      }
      if (sync) buffer += '\x1b[?2026l';
      this.terminal.write(buffer);
      // cursorRow/hardwareCursorRow track PHYSICAL rows on the terminal.
      this.cursorRow = Math.max(0, newPhysRows - 1);
      this.hardwareCursorRow = this.cursorRow;
      this.maxLinesRendered = clearSeq
        ? newPhysRows
        : Math.max(this.maxLinesRendered, newPhysRows);
      this.previousViewportTop = Math.max(0, this.maxLinesRendered - height);
      // A full paint repaints (or scrolls off) everything: no rows above
      // the viewport are left painted needing protection.
      this.ownedTopRow = 0;
      this.positionHardwareCursor(cursorPos, newPhysRows);
      this.commitFrame(
        newLines,
        newHasWide,
        newPhysRows,
        width,
        prefixIsPristine
      );
    };

    const CLEAR_ALL = TUI.CLEAR_ALL;
    const CLEAR_SCREEN = TUI.CLEAR_SCREEN;

    /**
     * Full redraw that repaints ONLY the visible viewport with the tail of
     * the frame, leaving everything above (terminal scrollback) untouched.
     *
     * Opt-in via `preserveScrollbackOnRedraw`. The alternative — fullRender
     * with CLEAR_ALL — destroys the user's entire scrollback (\x1b[3J) and
     * yanks their scroll position to the top of the session, because
     * re-emitting the whole frame is the only way to avoid duplicating the
     * static prefix that is already physically in scrollback.
     *
     * Rows above the viewport become committed, immutable history: they keep
     * whatever rendering they had when they scrolled off. That is invisible
     * for self-contained text but wrong for an element spanning those rows —
     * a full-height gutter or border shows gaps where the stale rows sit.
     */
    // Captured by the flush-overflow path for exactly the next render; any
    // strategy that full-emits instead may drop them (content is re-emitted).
    const pending = this.pendingFlush;
    this.pendingFlush = null;

    const viewportTailRender = (
      reason: string,
      mustPaintPhysRow?: number
    ): boolean => {
      // Walk back from the frame end until the tail fills the viewport, never
      // exceeding `height` physical rows: surplus rows scroll off into
      // scrollback and are appended again on the next redraw, duplicating the
      // fragment and displacing real history. A logical line taller than the
      // viewport cannot fit at all, so decline and let the caller full-render.
      let startIdx = newLines.length;
      let tailRows = 0;
      while (startIdx > 0 && tailRows < height) {
        const rows = this.wideLinesEnabled
          ? rowOf(newLines[startIdx - 1] ?? '')
          : 1;
        if (tailRows + rows > height) break;
        tailRows += rows;
        startIdx--;
      }
      if (startIdx === newLines.length) {
        this.debugLog(
          `viewport-tail declined: reason=${reason} lines=${newLines.length} (tail line exceeds height)`
        );
        return false;
      }
      // The straddled line's painted rows were left in place above the
      // erased region. If the rows below it shrank across the flush, the
      // tail window walked back over it and would paint a second copy
      // under the painted one — start the tail below it instead. Anchor on
      // the frame-wide first occurrence: when that sits above the tail the
      // straddled line is safely out of the window (an identical row inside
      // the tail is a different logical row and must still be painted).
      if (pending?.straddled != null) {
        const at = newLines.indexOf(pending.straddled);
        if (at >= startIdx) startIdx = at + 1;
        // The skip can empty the tail (the straddled line is the frame's
        // last line); painting nothing while committing the frame as
        // displayed would leave the cleared region permanently blank.
        if (startIdx === newLines.length) {
          this.debugLog(
            `viewport-tail declined: reason=${reason} lines=${newLines.length} (straddled line is the tail)`
          );
          return false;
        }
      }
      // A caller routing a specific change here needs that change painted.
      // A change above the tail window would commit unpainted: the frame
      // then records the new bytes as displayed, and a later flush's run
      // matcher suppresses re-emitting them — the only painted copy stays
      // stale and the change never reaches the terminal. Decline to a full
      // render instead.
      if (
        mustPaintPhysRow !== undefined &&
        mustPaintPhysRow < physRowOfNew(startIdx)
      ) {
        this.debugLog(
          `viewport-tail declined: reason=${reason} changeRow=${mustPaintPhysRow} tailStart=${physRowOfNew(startIdx)} (change above tail)`
        );
        return false;
      }
      this.fullRedrawCount++;
      this.debugLog(
        `fullRedraw #${this.fullRedrawCount}: reason=${reason} lines=${newLines.length} (viewport-tail)`
      );
      // Matched finalized rows replace erased screen rows, but never rows already
      // committed above the viewport; unmatched rows retain the lossless fallback.
      const preserved: string[] = [];
      // Dedup `rows` against the painted tail: drop the longest trailing
      // segment whose rows all reappear, in order, among the tail rows
      // (gaps allowed — streaming across the flush advances the live region
      // and may insert new rows between re-presented ones). At most one row
      // may match as a strict prefix of its tail counterpart, and only at
      // the frame index where that same logical row lands after the flush:
      // the live stream tip keeps growing between capture and repaint, so
      // its captured form is a stale prefix of the row the tail paints in
      // its place. A prefix elsewhere is an unrelated row — preserving it
      // risks bounded duplication, dropping it risks loss, so preserve.
      // Emitting only the prefix before the matched segment keeps preserved
      // rows contiguous and above the tail, so scrollback order holds.
      const pushTailDeduped = (
        rows: string[],
        grownRowIdx?: Array<number | null>
      ): void => {
        if (rows.length === 0) return;
        const reset = TUI.SEGMENT_RESET;
        const bareOf = (s: string): string =>
          s.endsWith(reset) ? s.slice(0, -reset.length) : s;
        const isGrownForm = (captured: string, tail: string): boolean => {
          const bare = bareOf(captured);
          if (visibleWidth(bare.trim()) === 0) return false;
          const bareTail = bareOf(tail);
          return bareTail.length > bare.length && bareTail.startsWith(bare);
        };
        const suffixMatchesTail = (from: number): boolean => {
          let tipUsed = false;
          let j = startIdx;
          for (let i = from; i < rows.length; i++) {
            const row = rows[i] ?? '';
            const grownIdx = grownRowIdx?.[i] ?? null;
            let found = -1;
            for (let k = j; k < newLines.length; k++) {
              const tailRow = newLines[k] ?? '';
              if (row === tailRow) {
                found = k;
                break;
              }
              if (
                !tipUsed &&
                k === grownIdx &&
                isGrownForm(row, tailRow)
              ) {
                tipUsed = true;
                found = k;
                break;
              }
            }
            if (found === -1) return false;
            j = found + 1;
          }
          return true;
        };
        let from = 0;
        while (from < rows.length && !suffixMatchesTail(from)) from++;
        for (let i = 0; i < from; i++) {
          preserved.push(rows[i] ?? '');
        }
      };
      if (pending) {
        const flushBase = Math.max(0, staticPrefixLen - pending.flushedCount);
        const flushEndAboveTail = Math.min(staticPrefixLen, startIdx);
        if (
          pending.runs.length > 0 &&
          staticPrefixLen >= pending.flushedCount
        ) {
          const firstRun = pending.runs[0]!;
          const lastRun = pending.runs[pending.runs.length - 1]!;
          const liveIndexForFlush = (flushIndex: number): number | null => {
            for (const run of pending.runs) {
              if (
                flushIndex >= run.flushStart &&
                flushIndex < run.flushStart + run.len
              ) {
                return run.liveStart + flushIndex - run.flushStart;
              }
            }
            return null;
          };

          for (let v = 0; v < pending.visible.length; v++) {
            if (pending.visibleStartLive + v >= firstRun.liveStart) break;
            preserved.push(pending.visible[v] ?? '');
          }
          // Emit in order, suppressing only a trailing block whose captured owners remain at the same tail coordinates.
          const flushEmit: Array<{
            row: string;
            matchedLiveIndex: number | null;
          }> = [];
          for (let f = 0; f < pending.flushedCount; f++) {
            const frameIdx = flushBase + f;
            if (frameIdx >= flushEndAboveTail) break;
            const matchedLiveIndex = liveIndexForFlush(f);
            if (
              matchedLiveIndex !== null &&
              matchedLiveIndex < pending.visibleStartLive
            ) {
              continue;
            }
            flushEmit.push({
              row: newLines[frameIdx] ?? '',
              matchedLiveIndex,
            });
          }
          let trailingStart = flushEmit.length;
          while (
            trailingStart > 0 &&
            flushEmit[trailingStart - 1]!.matchedLiveIndex !== null
          ) {
            trailingStart--;
          }
          for (let i = 0; i < trailingStart; i++) {
            preserved.push(flushEmit[i]!.row);
          }
          const trailingMatched = flushEmit.slice(trailingStart);
          const tailOwnsTrailing =
            trailingMatched.length > 0 &&
            trailingMatched.every(({ row, matchedLiveIndex }) => {
              if (matchedLiveIndex === null) return false;
              const frameIdx = staticPrefixLen + matchedLiveIndex;
              return (
                frameIdx >= startIdx &&
                frameIdx < newLines.length &&
                (newLines[frameIdx] ?? '') === row
              );
            });
          if (trailingMatched.length > 0 && !tailOwnsTrailing) {
            preserved.push(...trailingMatched.map(({ row }) => row));
          }
          if (startIdx > staticPrefixLen) {
            const afterRuns: string[] = [];
            // A live row surviving the flush keeps its logical position:
            // the flush replaces the live head the run covers, so the row
            // lands at staticPrefixLen + (liveIndex - lastRunLiveEnd). Only
            // there may its captured form prefix-match as a grown tip.
            // The mapping holds only when one run anchored at live 0
            // covers every flushed line: with divergent or gapped runs the
            // absorbed-row count is ambiguous (a gap row may be a flushed
            // line rewritten as it finalized, or a live row that survives),
            // and flushedCount may have grown past the matched batch. Any
            // ambiguity keeps the allowance off — bounded duplication over
            // a mis-slotted prefix match, which risks loss.
            const slotsKnown =
              pending.runs.length === 1 &&
              firstRun.liveStart === 0 &&
              firstRun.len === pending.flushedCount;
            const afterRunsGrownIdx: Array<number | null> = [];
            const lastRunLiveEnd = lastRun.liveStart + lastRun.len;
            // Slots are exact only through the first divergence between the
            // captured remainder and the live rows it maps onto: rows before
            // it are unchanged survivors verified in place, the divergent row
            // is the stream tip (growth and insertions happen at or after
            // it), and rows past it are shifted by an unknowable amount.
            let pastDivergence = false;
            for (let v = 0; v < pending.visible.length; v++) {
              const liveIndex = pending.visibleStartLive + v;
              if (liveIndex >= lastRunLiveEnd) {
                const slot = staticPrefixLen + (liveIndex - lastRunLiveEnd);
                afterRuns.push(pending.visible[v] ?? '');
                afterRunsGrownIdx.push(
                  slotsKnown && !pastDivergence ? slot : null
                );
                if (
                  !slotsKnown ||
                  slot >= newLines.length ||
                  (pending.visible[v] ?? '') !== (newLines[slot] ?? '')
                ) {
                  pastDivergence = true;
                }
              }
            }
            pushTailDeduped(afterRuns, afterRunsGrownIdx);
          }
        } else {
          pushTailDeduped(pending.visible);
          for (
            let frameIdx = flushBase;
            frameIdx < flushEndAboveTail;
            frameIdx++
          ) {
            preserved.push(newLines[frameIdx] ?? '');
          }
        }
      }
      const sync = this.synchronizedOutput;
      const cursorUp = height > 1 ? `\x1b[${height - 1}A` : '';
      // Replaying captured rows is safe only when the write is atomic.
      let preservedSegment = '';
      if (preserved.length > 0 && sync) {
        preservedSegment =
          preserved.join('\r\n') + '\r\n'.repeat(height) + cursorUp;
      }
      const bsu = this.emitBsu;
      let buffer = (bsu ? '\x1b[?2026h' : '') + preservedSegment;
      // Relative moves only — absolute addressing or clears above the
      // viewport would touch committed scrollback. Read the cursor fields,
      // not this function's enclosing locals: a static flush during this
      // same render pass may have erased the screen and reset them, and
      // moving up by the pre-erase offset would climb into rows left
      // painted above the erased region and wipe them. The owned origin
      // caps the climb the same way when a previous short paint left rows
      // above its start.
      const screenRow = Math.max(
        0,
        Math.min(
          height - 1,
          this.hardwareCursorRow -
            Math.max(this.previousViewportTop, this.ownedTopRow)
        )
      );
      if (screenRow > 0) buffer += `\x1b[${screenRow}A`;
      buffer += '\r\x1b[J';
      for (let i = startIdx; i < newLines.length; i++) {
        if (i > startIdx) buffer += '\r\n';
        buffer += newLines[i];
      }
      if (bsu) buffer += '\x1b[?2026l';
      this.terminal.write(buffer);
      this.cursorRow = Math.max(0, newPhysRows - 1);
      this.hardwareCursorRow = this.cursorRow;
      // The viewport anchor keeps its contract (frame row at the top of the
      // visible viewport); what a short paint changes is ownership. When the
      // tail painted fewer rows than the viewport — a straddled-line skip,
      // or a wide line tripping the tail walk — the rows above the paint
      // start were left painted, not repainted, so they are committed
      // history: record the paint start as the ownership boundary that
      // relative moves and erases must not cross.
      let paintedRows = 0;
      for (let i = startIdx; i < newLines.length; i++) {
        paintedRows += this.wideLinesEnabled ? rowOf(newLines[i] ?? '') : 1;
      }
      this.maxLinesRendered = newPhysRows;
      this.ownedTopRow = Math.max(0, newPhysRows - Math.min(height, paintedRows));
      this.previousViewportTop = Math.max(0, this.maxLinesRendered - height);
      this.positionHardwareCursor(cursorPos, newPhysRows);
      this.lastRenderKind = 'viewport-tail';
      // Rows above startIdx keep their old bytes, but static-prefix rows are
      // committed-current by definition: only unpainted LIVE rows go stale.
      // A tail paint reaching into the static prefix rewrote the whole live
      // region, which is the recovery site the scrollback-preserving path
      // actually hits (the post-flush repaint at every turn boundary).
      this.paintedRowsStale = startIdx > staticPrefixLen;
      this.commitFrame(
        newLines,
        newHasWide,
        newPhysRows,
        width,
        prefixIsPristine
      );
      return true;
    };

    // Strategy 1: First render (also after an invalidated frame or external
    // clear). On a true cold start, write the whole frame so history (e.g. a
    // resumed session's static prefix) lands in scrollback; '\x1b[J' clears
    // stale content below (e.g. after Ctrl+L). After the first frame, when
    // scrollback preservation is on, repaint only the viewport — the static
    // prefix is already physically in scrollback and re-emitting it would
    // require destroying the user's scrollback first.
    if (this.previousLines.length === 0 && !widthChanged) {
      if (
        !this.altScreen &&
        this.preserveScrollbackOnRedraw &&
        this.hadFirstFrame &&
        viewportTailRender('first')
      ) {
        return;
      }
      // Match accumulatedStaticOutput to newLines wrt wide-line state:
      // when nothing is wide at all, logical === physical.
      const staticPhysRows = this.wideLinesEnabled
        ? physRows(this.staticBuffer.view as string[])
        : this.staticBuffer.length;
      const liveRows = newPhysRows - staticPhysRows;
      const needsScrollbackClear = liveRows > height;
      fullRender(
        this.altScreen
          ? CLEAR_SCREEN
          : needsScrollbackClear
            ? CLEAR_ALL
            : '\x1b[J',
        'first'
      );
      return;
    }

    // Strategy 2: Width changed
    if (widthChanged) {
      fullRender(this.altScreen ? CLEAR_SCREEN : CLEAR_ALL, 'width-changed');
      return;
    }

    // Strategy 3: Shrink clear
    if (
      this.clearOnShrink &&
      newPhysRows < this.maxLinesRendered &&
      !hasVisibleOverlay
    ) {
      fullRender(this.altScreen ? CLEAR_SCREEN : CLEAR_ALL, 'shrink');
      return;
    }

    // Strategy 4: Differential
    //
    // Indexing into `previousLines`/`newLines` uses LOGICAL indices (position
    // of a logical line in the source array). Cursor positioning, viewport
    // math, and row counts use PHYSICAL rows (terminal rows after soft-wrap).
    // Conversions between the two use `physRowOf(...)` / `rowOf(...)`.
    let firstChanged = -1;
    let lastChanged = -1;
    const maxLen = Math.max(newLines.length, this.previousLines.length);
    // Skip the static prefix when it is provably identical to last frame's.
    // StaticBuffer bumps its generation on every mutation, so an unchanged
    // generation means an unchanged prefix. See StaticBuffer for why comparing
    // line contents instead would be unsound.
    const prefixAligned =
      prefixIsPristine && this.frameGeneration === this.staticBuffer.generation;
    const diffStart = prefixAligned ? staticPrefixLen : 0;
    this.perfLastFrame.diffScanned = Math.max(0, maxLen - diffStart);
    this.perfLastFrame.prefixLines = staticPrefixLen;
    this.perfLastFrame.prefixSkipped = prefixAligned;
    for (let i = diffStart; i < maxLen; i++) {
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
    const appendStart =
      appendedLines &&
      firstChanged === this.previousLines.length &&
      firstChanged > 0;

    // No changes
    if (firstChanged === -1) {
      this.positionHardwareCursor(cursorPos, newPhysRows);
      this.commitFrame(
        newLines,
        newHasWide,
        newPhysRows,
        width,
        prefixIsPristine
      );
      this.previousViewportTop = Math.max(0, this.maxLinesRendered - height);
      return;
    }

    // All changes are tail deletions (new ends before any new content at firstChanged).
    if (firstChanged >= newLines.length) {
      if (this.previousLines.length > newLines.length) {
        const sync = this.emitBsu;
        let buffer = sync ? '\x1b[?2026h' : '';
        // The logical row where the new content ends, expressed as a
        // physical row (cursor will land on the last physical row of the
        // last surviving logical line).
        const lastLogicalIdx = Math.max(0, newLines.length - 1);
        const targetPhysRow = Math.max(
          0,
          physRowOfNew(lastLogicalIdx) +
            rowOf(newLines[lastLogicalIdx] ?? '') -
            1
        );
        // A deletion ending inside rows a short paint left painted cannot
        // be reached by relative moves; repaint the tail from the owned
        // origin instead.
        if (
          this.ownedTopRow > prevViewportTop &&
          targetPhysRow < this.ownedTopRow
        ) {
          if (
            !this.altScreen &&
            this.preserveScrollbackOnRedraw &&
            viewportTailRender('owned-band-shrink')
          ) {
            return;
          }
          fullRender(
            this.altScreen ? CLEAR_SCREEN : CLEAR_ALL,
            'owned-band-shrink'
          );
          return;
        }
        const lineDiff = computeLineDiff(targetPhysRow);
        if (lineDiff > 0) buffer += `\x1b[${lineDiff}B`;
        else if (lineDiff < 0) buffer += `\x1b[${-lineDiff}A`;
        buffer += '\r';
        // Count the number of PHYSICAL rows that used to exist past the
        // new end — those need to be erased one terminal row at a time.
        const extraPhys = prevPhysRows - newPhysRows;
        if (extraPhys > height) {
          if (
            !this.altScreen &&
            this.preserveScrollbackOnRedraw &&
            viewportTailRender('extra>height')
          ) {
            return;
          }
          fullRender(this.altScreen ? CLEAR_SCREEN : CLEAR_ALL, 'extra>height');
          return;
        }
        if (extraPhys > 0) buffer += '\x1b[1B';
        for (let i = 0; i < extraPhys; i++) {
          buffer += '\r\x1b[2K';
          if (i < extraPhys - 1) buffer += '\x1b[1B';
        }
        if (extraPhys > 0) buffer += `\x1b[${extraPhys}A`;
        if (sync) buffer += '\x1b[?2026l';
        this.terminal.write(buffer);
        this.cursorRow = targetPhysRow;
        this.hardwareCursorRow = targetPhysRow;
      }
      this.positionHardwareCursor(cursorPos, newPhysRows);
      this.commitFrame(
        newLines,
        newHasWide,
        newPhysRows,
        width,
        prefixIsPristine
      );
      this.previousViewportTop = Math.max(0, this.maxLinesRendered - height);
      return;
    }

    // Change above previous viewport.
    // `previousContentViewportTop` is the PHYSICAL row where the previous
    // viewport begins. Compare the physical row of `firstChanged` (logical
    // index) against it.
    const previousContentViewportTop = Math.max(0, prevPhysRows - height);
    const firstChangedPhysRow = physRowOfNew(firstChanged);
    // Rows below the ownership boundary were left painted by a short tail
    // paint; relative moves cannot reach them (the up-move clamps at the
    // paint start, landing short by the stale band's height, and the wide
    // branch's clear-to-end would then wipe rows this renderer no longer
    // owns). A change there — a straddled live line growing mid-stream —
    // repaints the tail from the owned origin instead: the stale band
    // scrolls into scrollback as the committed history the frame already
    // considers it.
    if (
      this.ownedTopRow > previousContentViewportTop &&
      firstChangedPhysRow < this.ownedTopRow
    ) {
      if (
        !this.altScreen &&
        this.preserveScrollbackOnRedraw &&
        viewportTailRender('owned-band-change', firstChangedPhysRow)
      ) {
        return;
      }
      fullRender(
        this.altScreen ? CLEAR_SCREEN : CLEAR_ALL,
        'owned-band-change'
      );
      return;
    }
    if (
      firstChangedPhysRow < previousContentViewportTop &&
      newPhysRows >= prevPhysRows
    ) {
      // Re-scan for first change within the visible viewport.
      // We walk logical indices but skip until the logical line starts at
      // or past the viewport's physical top.
      firstChanged = -1;
      lastChanged = -1;
      // Skipped rows are committed below without being written; a changed
      // live row among them is exactly the staleness the latch expresses.
      let unpaintedLiveChange = false;
      for (
        let i = 0;
        i < Math.max(newLines.length, this.previousLines.length);
        i++
      ) {
        const physStartInNew = physRowOfNew(Math.min(i, newLines.length));
        if (physStartInNew < previousContentViewportTop) {
          if (
            i >= staticPrefixLen &&
            (this.previousLines[i] ?? '') !== (newLines[i] ?? '')
          ) {
            unpaintedLiveChange = true;
          }
          continue;
        }
        const oldLine = this.previousLines[i] ?? '';
        const newLine = newLines[i] ?? '';
        if (oldLine !== newLine) {
          if (firstChanged === -1) firstChanged = i;
          lastChanged = i;
        }
      }
      if (unpaintedLiveChange) this.paintedRowsStale = true;
      if (firstChanged === -1) {
        // Only off-screen changes — nothing to render
        this.commitFrame(
          newLines,
          newHasWide,
          newPhysRows,
          width,
          prefixIsPristine
        );
        this.previousViewportTop = Math.max(0, this.maxLinesRendered - height);
        return;
      }
    } else if (firstChangedPhysRow < previousContentViewportTop) {
      if (
        !this.altScreen &&
        this.preserveScrollbackOnRedraw &&
        viewportTailRender('off-screen-change')
      ) {
        return;
      }
      fullRender(
        this.altScreen ? CLEAR_SCREEN : CLEAR_ALL,
        'off-screen-change'
      );
      return;
    }

    // Build differential buffer
    const sync = this.emitBsu;
    let buffer = sync ? '\x1b[?2026h' : '';
    const prevViewportBottom = prevViewportTop + height - 1;
    // `moveTargetPhysRow` is the PHYSICAL row we position the cursor at
    // BEFORE emitting `\r\n` (appendStart) or `\r` (not appendStart).
    // - Not appendStart: cursor lands at the physical row of `firstChanged`,
    //   then `\r` goes to col 0 of that row. We then write `firstChanged`
    //   in place.
    // - appendStart: we're appending NEW lines past the end of previousLines.
    //   We want to land at the LAST physical row of line (firstChanged - 1),
    //   so that `\r\n` advances to a fresh row for the new content. Using
    //   `physRowOf(newLines, firstChanged - 1) + rowOf(...) - 1` gives the
    //   last physical row of that line (handles wide lines correctly).
    const moveTargetPhysRow = appendStart
      ? physRowOfNew(firstChanged - 1) +
        rowOf(newLines[firstChanged - 1] ?? '') -
        1
      : physRowOfNew(firstChanged);

    if (moveTargetPhysRow > prevViewportBottom) {
      const currentScreenRow = Math.max(
        0,
        Math.min(height - 1, hardwareCursorRow - prevViewportTop)
      );
      const moveToBottom = height - 1 - currentScreenRow;
      if (moveToBottom > 0) buffer += `\x1b[${moveToBottom}B`;
      const scroll = moveTargetPhysRow - prevViewportBottom;
      buffer += '\r\n'.repeat(scroll);
      prevViewportTop += scroll;
      viewportTop += scroll;
      hardwareCursorRow = moveTargetPhysRow;
    }

    const lineDiff = computeLineDiff(moveTargetPhysRow);
    if (lineDiff > 0) buffer += `\x1b[${lineDiff}B`;
    else if (lineDiff < 0) buffer += `\x1b[${-lineDiff}A`;
    buffer += appendStart ? '\r\n' : '\r';

    // When any changed logical line has rowOf > 1 (i.e. will soft-wrap),
    // the previous line at that position may have occupied MORE physical
    // rows than the new one, or vice versa. Per-row `\x1b[2K` won't clear
    // stale rows below. Use `\x1b[J` to clear from cursor to end of screen
    // and rewrite the region from firstChanged to end, which guarantees
    // correct output at the cost of re-writing more.
    //
    // Optimization: skip scanning the side (previous or new) that is known
    // to contain no wide lines (via cached `prevHasWide` / `newHasWide`
    // flags). Identical lines that weren't part of the diff also don't
    // need checking — the precomputed firstChanged..lastChanged window
    // already excludes most of those.
    const anyWideChange = (() => {
      if (!newHasWide && !prevHasWide) return false;
      const scanStart = firstChanged;
      const scanEnd = Math.max(lastChanged, this.previousLines.length - 1);
      for (let i = scanStart; i <= scanEnd; i++) {
        if (prevHasWide) {
          const prevLine = this.previousLines[i];
          if (prevLine !== undefined && rowOf(prevLine) > 1) return true;
        }
        if (newHasWide) {
          const newLine = newLines[i];
          if (newLine !== undefined && rowOf(newLine) > 1) return true;
        }
      }
      return false;
    })();

    let renderEnd: number;
    let finalCursorRow: number;

    if (anyWideChange) {
      // Clear to end of screen, then re-write newLines from firstChanged.
      buffer += '\x1b[J';
      renderEnd = newLines.length - 1;
      for (let i = firstChanged; i <= renderEnd; i++) {
        if (i > firstChanged) buffer += '\r\n';
        buffer += newLines[i];
      }
      // Final cursor lands on the last physical row of the last logical
      // line written.
      const lastIdx = renderEnd;
      finalCursorRow =
        lastIdx >= 0
          ? physRowOfNew(lastIdx) + rowOf(newLines[lastIdx] ?? '') - 1
          : 0;
    } else {
      // Narrow-only path: per-row clear + write. Equivalent to pre-existing
      // behavior when all lines fit within terminal width.
      renderEnd = Math.min(lastChanged, newLines.length - 1);
      for (let i = firstChanged; i <= renderEnd; i++) {
        if (i > firstChanged) buffer += '\r\n';
        buffer += '\x1b[2K';
        buffer += newLines[i];
      }
      finalCursorRow = physRowOfNew(renderEnd);

      if (this.previousLines.length > newLines.length) {
        if (renderEnd < newLines.length - 1) {
          const moveDown = newLines.length - 1 - renderEnd;
          buffer += `\x1b[${moveDown}B`;
          finalCursorRow = physRowOfNew(newLines.length - 1);
        }
        const extraPhys = prevPhysRows - newPhysRows;
        for (let i = 0; i < extraPhys; i++) {
          buffer += '\r\n\x1b[2K';
        }
        if (extraPhys > 0) buffer += `\x1b[${extraPhys}A`;
      }
    }

    if (sync) buffer += '\x1b[?2026l';
    this.terminal.write(buffer);

    this.cursorRow = Math.max(0, newPhysRows - 1);
    this.hardwareCursorRow = finalCursorRow;
    this.maxLinesRendered = Math.max(this.maxLinesRendered, newPhysRows);
    this.previousViewportTop = Math.max(0, this.maxLinesRendered - height);
    this.positionHardwareCursor(cursorPos, newPhysRows);
    this.commitFrame(
      newLines,
      newHasWide,
      newPhysRows,
      width,
      prefixIsPristine
    );
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
  private positionHardwareCursor(
    cursorPos: { row: number; col: number } | null,
    totalLines: number
  ): void {
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
