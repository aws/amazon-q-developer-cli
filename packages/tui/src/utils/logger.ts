import { writeFileSync, appendFileSync, existsSync, mkdirSync } from 'fs';
import { tmpdir } from 'os';
import { join } from 'path';

type LogLevel = 'error' | 'warn' | 'info' | 'debug' | 'trace';

const LOG_LEVELS: Record<LogLevel, number> = {
  error: 0,
  warn: 1,
  info: 2,
  debug: 3,
  trace: 4,
};

/**
 * Captured at module load so the logger's last-resort fallback never
 * resolves through whatever `console.error` currently points to. The
 * console interceptor (`utils/console-interceptor.ts`) replaces
 * `console.error` with a redirect into `logger.error`, which would
 * otherwise loop forever the moment a file write fails.
 */
const ORIGINAL_CONSOLE_ERROR = console.error.bind(console);

/**
 * Resolves the default log file path, matching the backend's log directory.
 * Backend uses: $TMPDIR/kiro-log/kiro-chat.log
 * TUI uses:     $TMPDIR/kiro-log/kiro-tui.log
 */
function getDefaultLogFile(): string {
  const logsDir = join(tmpdir(), 'kiro-log');
  if (!existsSync(logsDir)) {
    mkdirSync(logsDir, { recursive: true });
  }
  return join(logsDir, 'kiro-tui.log');
}

class Logger {
  private logFile: string | null;
  private logLevel: LogLevel;

  constructor() {
    this.logFile = process.env.KIRO_TUI_LOG_FILE || getDefaultLogFile();
    this.logLevel = (process.env.KIRO_TUI_LOG_LEVEL as LogLevel) || 'error';

    // Initialize log file
    if (this.logFile && !existsSync(this.logFile)) {
      try {
        writeFileSync(
          this.logFile,
          `=== TUI Log Started ${new Date().toISOString()} ===\n`
        );
      } catch {
        // If we can't write the default log file, disable file logging
        if (!process.env.KIRO_TUI_LOG_FILE) {
          this.logFile = null;
        }
      }
    }
  }

  private shouldLog(level: LogLevel): boolean {
    return LOG_LEVELS[level] <= LOG_LEVELS[this.logLevel];
  }

  private writeLog(level: LogLevel, message: string, ...args: any[]) {
    if (!this.logFile || !this.shouldLog(level)) return;

    const timestamp = new Date().toISOString();
    const formattedArgs =
      args.length > 0
        ? ' ' +
          args
            .map((arg) =>
              typeof arg === 'object' ? JSON.stringify(arg) : String(arg)
            )
            .join(' ')
        : '';

    const logLine = `[${timestamp}] ${level.toUpperCase()}: ${message}${formattedArgs}\n`;

    try {
      appendFileSync(this.logFile, logLine);
    } catch (error) {
      // Fallback to the captured original console.error so we don't
      // recurse if the interceptor has already redirected console.error
      // back into this logger.
      ORIGINAL_CONSOLE_ERROR('Logger write failed:', error);
    }
  }

  error(message: string, ...args: any[]) {
    this.writeLog('error', message, ...args);
  }

  warn(message: string, ...args: any[]) {
    this.writeLog('warn', message, ...args);
  }

  info(message: string, ...args: any[]) {
    this.writeLog('info', message, ...args);
  }

  debug(message: string, ...args: any[]) {
    this.writeLog('debug', message, ...args);
  }

  trace(message: string, ...args: any[]) {
    this.writeLog('trace', message, ...args);
  }
}

export const logger = new Logger();
