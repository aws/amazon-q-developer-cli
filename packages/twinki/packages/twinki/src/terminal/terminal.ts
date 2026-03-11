/**
 * Minimal terminal interface for TUI rendering.
 * 
 * Provides a standardized interface for terminal operations that abstracts
 * the underlying terminal implementation. This allows the TUI system to work
 * with different terminal backends (process-based, virtual, etc.) while
 * maintaining consistent behavior.
 * 
 * The interface handles:
 * - Terminal lifecycle (start/stop)
 * - Input event handling with proper cleanup
 * - Output writing with cursor and screen control
 * - Terminal dimension queries
 * - Enhanced keyboard protocol detection
 * 
 * @example
 * ```typescript
 * const terminal = new ProcessTerminal();
 * terminal.start(
 *   (data) => console.log('Input:', data),
 *   () => console.log('Resized')
 * );
 * terminal.write('Hello, terminal!');
 * terminal.stop();
 * ```
 */
export interface Terminal {
	/**
	 * Starts the terminal with input and resize handlers.
	 * 
	 * Initializes the terminal for TUI operation by:
	 * - Setting up raw mode for immediate key detection
	 * - Enabling enhanced keyboard protocols if available
	 * - Registering input and resize event handlers
	 * - Configuring terminal state for optimal TUI rendering
	 * 
	 * Must be called before any other terminal operations.
	 * 
	 * @param onInput - Handler called for each input sequence received
	 * @param onResize - Handler called when terminal dimensions change
	 * 
	 * @example
	 * ```typescript
	 * terminal.start(
	 *   (data) => {
	 *     if (data === '\x03') process.exit(); // Ctrl+C
	 *   },
	 *   () => {
	 *     console.log(`New size: ${terminal.columns}x${terminal.rows}`);
	 *   }
	 * );
	 * ```
	 */
	start(onInput: (data: string) => void, onResize: () => void): void;

	/**
	 * Stops the terminal and restores previous state.
	 * 
	 * Performs cleanup operations:
	 * - Disables enhanced keyboard protocols
	 * - Restores original terminal mode
	 * - Removes event handlers
	 * - Prevents input leakage to parent shell
	 * 
	 * Should be called before application exit to ensure proper cleanup.
	 * 
	 * @example
	 * ```typescript
	 * process.on('SIGINT', () => {
	 *   terminal.stop();
	 *   process.exit(0);
	 * });
	 * ```
	 */
	stop(): void;

	/**
	 * Drains stdin before exiting to prevent key release events from
	 * leaking to the parent shell over slow connections.
	 * 
	 * This is critical for preventing phantom keystrokes in the parent
	 * shell when the TUI exits, especially over SSH or slow connections.
	 * The function waits for input to stop arriving before returning.
	 * 
	 * Performance: O(1) with configurable timeout, typically completes
	 * within 50ms on local terminals, up to 1000ms over slow connections.
	 * 
	 * @param maxMs - Maximum time to drain in milliseconds (default: 1000)
	 * @param idleMs - Exit early if no input arrives within this time (default: 50)
	 * @returns Promise that resolves when draining is complete
	 * 
	 * @example
	 * ```typescript
	 * // Proper shutdown sequence
	 * terminal.stop();
	 * await terminal.drainInput();
	 * process.exit(0);
	 * ```
	 */
	drainInput(maxMs?: number, idleMs?: number): Promise<void>;

	/**
	 * Writes data to the terminal output.
	 * 
	 * Sends raw data to the terminal, typically containing text content
	 * and ANSI escape sequences for formatting and cursor control.
	 * The data is written synchronously to ensure proper ordering.
	 * 
	 * @param data - Data to write (text and/or ANSI escape sequences)
	 * 
	 * @example
	 * ```typescript
	 * terminal.write('Hello, ');
	 * terminal.write('\x1b[31mred text\x1b[0m'); // Red colored text
	 * terminal.write('\x1b[2J\x1b[H'); // Clear screen and move to top
	 * ```
	 */
	write(data: string): void;

	/**
	 * Gets the terminal width in columns.
	 * 
	 * Returns the current terminal width, which may change during
	 * application runtime if the user resizes the terminal window.
	 * Used for text wrapping and layout calculations.
	 * 
	 * @returns Width in character columns
	 * 
	 * @example
	 * ```typescript
	 * const maxLineLength = terminal.columns - 2; // Leave margin
	 * const wrappedText = wrapText(text, maxLineLength);
	 * ```
	 */
	get columns(): number;

	/**
	 * Gets the terminal height in rows.
	 * 
	 * Returns the current terminal height, which may change during
	 * application runtime if the user resizes the terminal window.
	 * Used for viewport calculations and scrolling logic.
	 * 
	 * @returns Height in character rows
	 * 
	 * @example
	 * ```typescript
	 * const visibleLines = terminal.rows - 1; // Reserve bottom line
	 * const viewport = content.slice(scrollTop, scrollTop + visibleLines);
	 * ```
	 */
	get rows(): number;

	/**
	 * Indicates whether Kitty keyboard protocol is currently active.
	 * 
	 * The Kitty protocol provides enhanced key detection capabilities,
	 * allowing distinction between keys that would otherwise be ambiguous
	 * (e.g., Ctrl+I vs Tab, Shift+Enter vs Enter). When active, the
	 * terminal sends more detailed key event information.
	 * 
	 * @returns True if Kitty protocol is active, false otherwise
	 * 
	 * @example
	 * ```typescript
	 * if (terminal.kittyProtocolActive) {
	 *   // Can distinguish Ctrl+I from Tab
	 *   console.log('Enhanced key detection available');
	 * }
	 * ```
	 */
	get kittyProtocolActive(): boolean;

	/**
	 * Moves cursor up or down by the specified number of lines.
	 * 
	 * Provides relative cursor movement without changing the column position.
	 * Negative values move up, positive values move down. Movement is
	 * clamped to terminal boundaries.
	 * 
	 * @param lines - Number of lines to move (negative = up, positive = down)
	 * 
	 * @example
	 * ```typescript
	 * terminal.moveBy(-3); // Move cursor up 3 lines
	 * terminal.moveBy(1);  // Move cursor down 1 line
	 * ```
	 */
	moveBy(lines: number): void;

	/**
	 * Hides the terminal cursor.
	 * 
	 * Makes the cursor invisible, useful during rendering operations
	 * to prevent cursor flicker. Should be paired with showCursor()
	 * to restore visibility when rendering is complete.
	 * 
	 * @example
	 * ```typescript
	 * terminal.hideCursor();
	 * // Perform rendering operations
	 * terminal.showCursor();
	 * ```
	 */
	hideCursor(): void;

	/**
	 * Shows the terminal cursor.
	 * 
	 * Makes the cursor visible after it was hidden with hideCursor().
	 * The cursor will appear at its current position.
	 * 
	 * @example
	 * ```typescript
	 * terminal.hideCursor();
	 * renderComplexUI();
	 * terminal.showCursor(); // Restore cursor visibility
	 * ```
	 */
	showCursor(): void;

	/**
	 * Clears the current line from cursor position to end.
	 * 
	 * Erases content from the cursor position to the end of the current
	 * line, leaving the cursor at its current position. Useful for
	 * clearing partial line content during updates.
	 * 
	 * @example
	 * ```typescript
	 * terminal.write('Old content');
	 * terminal.write('\r'); // Return to start of line
	 * terminal.write('New ');
	 * terminal.clearLine(); // Clear remaining "content"
	 * ```
	 */
	clearLine(): void;

	/**
	 * Clears from cursor position to end of screen.
	 * 
	 * Erases all content from the current cursor position to the end
	 * of the screen, including the current line and all lines below.
	 * Cursor position remains unchanged.
	 * 
	 * @example
	 * ```typescript
	 * // Clear everything below current position
	 * terminal.clearFromCursor();
	 * ```
	 */
	clearFromCursor(): void;

	/**
	 * Clears the entire screen and moves cursor to top-left (0,0).
	 * 
	 * Performs a complete screen clear and cursor reset, equivalent
	 * to the 'clear' command. Use for full screen refreshes or
	 * application initialization.
	 * 
	 * @example
	 * ```typescript
	 * // Start with clean screen
	 * terminal.clearScreen();
	 * terminal.write('Welcome to the application!');
	 * ```
	 */
	clearScreen(): void;

	/**
	 * Enables SGR mouse tracking (mode 1000 + 1003 + 1006).
	 * Sends button events, all-motion events, and SGR encoding.
	 */
	enableMouse(): void;

	/**
	 * Disables mouse tracking.
	 */
	disableMouse(): void;

	/**
	 * Sets the terminal window title.
	 * 
	 * Updates the title bar of the terminal window (if supported).
	 * Not all terminals support title changes, and some may ignore
	 * this operation for security reasons.
	 * 
	 * @param title - The title to set
	 * 
	 * @example
	 * ```typescript
	 * terminal.setTitle('My TUI Application v1.0');
	 * terminal.setTitle(`Editor - ${filename}`);
	 * ```
	 */
	setTitle(title: string): void;
}