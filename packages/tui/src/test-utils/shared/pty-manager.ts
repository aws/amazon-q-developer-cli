/**
 * Shared PTY utilities for test cases.
 * Provides common functionality for creating PTYs, sending input, and capturing output.
 */

import { Terminal } from '@xterm/headless';
import * as pty from 'bun-pty';
import stripAnsi from 'strip-ansi';

/**
 * Configuration options for PTY creation.
 */
export interface PtyOptions {
  /** Terminal width in characters */
  width: number;
  /** Terminal height in characters */
  height: number;
  /** Working directory for the spawned process */
  cwd?: string;
  /** Environment variables to pass to the spawned process */
  env?: Record<string, string>;
}

export class PtyManager {
  private pty?: pty.IPty;
  private output: string = '';
  private terminal: Terminal;

  constructor(private options: PtyOptions) {
    this.terminal = new Terminal({
      cols: options.width,
      rows: options.height,
      scrollback: 10000,
      allowProposedApi: true,
    });
  }

  /**
   * Spawns a new PTY process with the given command and arguments.
   *
   * @param command - The command to execute
   * @param args - Arguments to pass to the command
   */
  spawn(command: string, args: string[]): void {
    this.pty = pty.spawn(command, args, {
      name: 'xterm-color',
      cols: this.options.width,
      rows: this.options.height,
      cwd: this.options.cwd || process.cwd(),
      env: {
        ...(process.env as Record<string, string>),
        ...this.options.env,
      },
    });

    // Capture output and feed to xterm for parsing
    this.pty.onData((data) => {
      this.output += data;
      this.terminal.write(data);
    });
  }

  /**
   * Sends keystrokes or raw bytes to the PTY.
   *
   * @param input - String to type or array of byte values (e.g., [0x03] for Ctrl+C)
   */
  async sendKeys(input: string | number[]): Promise<void> {
    if (!this.pty) throw new Error('PTY not spawned');

    if (typeof input === 'string') {
      this.pty.write(input);
    } else {
      this.pty.write(Buffer.from(input).toString('utf-8'));
    }
  }

  /**
   * Waits for specific text to appear in the terminal output.
   *
   * @param text - Text to wait for (case-sensitive)
   * @param timeout_ms - Timeout in milliseconds (defaults to 10000)
   */
  async waitForText(text: string, timeout_ms: number = 10000): Promise<void> {
    const startTime = Date.now();

    while (Date.now() - startTime < timeout_ms) {
      if (stripAnsi(this.output).includes(text)) {
        return;
      }
      await new Promise((resolve) => setTimeout(resolve, 50));
    }

    throw new Error(
      `Timeout waiting for text: "${text}". Output was:\n${stripAnsi(this.output)}`
    );
  }

  /**
   * Returns the current terminal output.
   */
  getOutput(): string {
    return this.output;
  }

  /**
   * Returns the terminal output with ANSI escape sequences stripped.
   */
  getOutputCleaned(): string {
    return stripAnsi(this.output);
  }

  /**
   * Waits for the PTY process to exit and returns the exit code.
   *
   * @param timeout_ms - Timeout in milliseconds (defaults to 10000)
   */
  async expectExit(timeout_ms: number = 10000): Promise<number> {
    if (!this.pty) throw new Error('PTY not spawned');

    return new Promise((resolve, reject) => {
      const timer = setTimeout(() => {
        reject(new Error('Process did not exit within timeout'));
      }, timeout_ms);

      this.pty!.onExit(({ exitCode }) => {
        clearTimeout(timer);
        resolve(exitCode);
      });
    });
  }

  /**
   * Captures the current terminal output as a snapshot for analysis.
   *
   * @returns Promise resolving to a TerminalSnapshot with analysis methods
   */
  async terminalSnapshot(): Promise<TerminalSnapshot> {
    return new TerminalSnapshot(this.output);
  }

  /**
   * Terminates the PTY process.
   */
  kill(): void {
    if (this.pty) {
      this.pty.kill();
    }
  }

  /**
   * Returns the PID of the spawned process.
   */
  getPid(): number | undefined {
    return this.pty?.pid;
  }

  /**
   * Returns the current terminal screen as a 2D array of characters.
   * Each row is a string representing one line of the terminal.
   * Escape codes are parsed by xterm, so this returns the actual rendered content.
   * Includes scrollback buffer content.
   */
  getSnapshot(): string[] {
    const buffer = this.terminal.buffer.active;
    const lines: string[] = [];

    // Include scrollback + visible rows
    const totalLines = buffer.baseY + this.terminal.rows;
    for (let i = 0; i < totalLines; i++) {
      const line = buffer.getLine(i);
      lines.push(line?.translateToString() ?? '');
    }

    return lines;
  }

  /**
   * Returns the snapshot formatted with a terminal border for display.
   */
  getSnapshotFormatted(): string {
    const lines = this.getSnapshot();
    const width = this.options.width;
    const top = '┌' + '─'.repeat(width) + '┐';
    const bottom = '└' + '─'.repeat(width) + '┘';
    const bordered = lines.map((line) => '│' + line.padEnd(width) + '│');
    return [top, ...bordered, bottom].join('\n');
  }

  /**
   * Returns the terminal screen as HTML with inline styles for colors.
   */
  getSnapshotHtml(): string {
    const buffer = this.terminal.buffer.active;
    const lines: string[] = [];

    // ANSI 256 color palette (standard 16 colors)
    const palette = [
      '#000000',
      '#cd0000',
      '#00cd00',
      '#cdcd00',
      '#0000ee',
      '#cd00cd',
      '#00cdcd',
      '#e5e5e5',
      '#7f7f7f',
      '#ff0000',
      '#00ff00',
      '#ffff00',
      '#5c5cff',
      '#ff00ff',
      '#00ffff',
      '#ffffff',
    ];

    const getColor = (
      color: number,
      isRgb: boolean,
      isPalette: boolean
    ): string | null => {
      if (isRgb) {
        return `#${color.toString(16).padStart(6, '0')}`;
      }
      if (isPalette && color < 256) {
        if (color < 16) return palette[color] ?? null;
        // 216 color cube (16-231)
        if (color < 232) {
          const c = color - 16;
          const r = Math.floor(c / 36) * 51;
          const g = Math.floor((c % 36) / 6) * 51;
          const b = (c % 6) * 51;
          return `rgb(${r},${g},${b})`;
        }
        // Grayscale (232-255)
        const gray = (color - 232) * 10 + 8;
        return `rgb(${gray},${gray},${gray})`;
      }
      return null;
    };

    for (let y = 0; y < this.terminal.rows; y++) {
      const line = buffer.getLine(y);
      if (!line) {
        lines.push('');
        continue;
      }

      let html = '';
      let currentStyle = '';
      let spanOpen = false;

      for (let x = 0; x < this.terminal.cols; x++) {
        const cell = line.getCell(x);
        if (!cell) continue;

        const char = cell.getChars() || ' ';
        if (cell.getWidth() === 0) continue; // Skip continuation cells

        const styles: string[] = [];

        const fg = getColor(
          cell.getFgColor(),
          cell.isFgRGB(),
          cell.isFgPalette()
        );
        const bg = getColor(
          cell.getBgColor(),
          cell.isBgRGB(),
          cell.isBgPalette()
        );

        if (fg) styles.push(`color:${fg}`);
        if (bg) styles.push(`background:${bg}`);
        if (cell.isBold()) styles.push('font-weight:bold');
        if (cell.isItalic()) styles.push('font-style:italic');
        if (cell.isUnderline()) styles.push('text-decoration:underline');
        if (cell.isDim()) styles.push('opacity:0.5');

        const style = styles.join(';');

        if (style !== currentStyle) {
          if (spanOpen) html += '</span>';
          if (style) {
            html += `<span style="${style}">`;
            spanOpen = true;
          } else {
            spanOpen = false;
          }
          currentStyle = style;
        }

        // Escape HTML entities
        const escaped = char
          .replace(/&/g, '&amp;')
          .replace(/</g, '&lt;')
          .replace(/>/g, '&gt;');
        html += escaped;
      }

      if (spanOpen) html += '</span>';
      lines.push(html);
    }

    return `<pre style="font-family:monospace;background:#fff;color:#000;padding:10px;margin:0">${lines.join('\n')}</pre>`;
  }

  /**
   * Waits for specific text to be visible on the terminal screen.
   * Uses xterm to check the rendered viewport.
   *
   * @param text - Text to wait for (case-sensitive)
   * @param timeout_ms - Timeout in milliseconds (defaults to 10000)
   */
  async waitForVisibleText(
    text: string,
    timeout_ms: number = 10000
  ): Promise<void> {
    const startTime = Date.now();

    while (Date.now() - startTime < timeout_ms) {
      const snapshot = this.getSnapshot();
      if (snapshot.some((line) => line.includes(text))) {
        return;
      }
      await new Promise((resolve) => setTimeout(resolve, 50));
    }

    throw new Error(
      `Timeout waiting for visible text: "${text}". Screen was:\n${this.getSnapshot().join('\n')}`
    );
  }
}

/**
 * Represents a snapshot of terminal output with analysis utilities.
 * Provides methods to search, match, and extract content from terminal output
 * while handling ANSI escape sequences appropriately.
 */
export class TerminalSnapshot {
  /**
   * @param rawOutput - Raw terminal output including ANSI escape sequences
   */
  constructor(private rawOutput: string) {}

  /**
   * Returns the terminal output with ANSI escape sequences stripped.
   *
   * @returns Clean text content
   */
  getCleanText(): string {
    return stripAnsi(this.rawOutput);
  }

  /**
   * Checks if the terminal output contains the specified text.
   *
   * @param text - Text to search for (case-sensitive)
   * @returns True if text is found
   */
  contains(text: string): boolean {
    return this.getCleanText().includes(text);
  }

  /**
   * Tests the terminal output against a regular expression.
   *
   * @param pattern - Regular expression to test
   * @returns True if pattern matches
   */
  matches(pattern: RegExp): boolean {
    return pattern.test(this.getCleanText());
  }

  /**
   * Splits the terminal output into individual lines.
   *
   * @returns Array of lines from the terminal output
   */
  getLines(): string[] {
    return this.getCleanText().split('\n');
  }
}
