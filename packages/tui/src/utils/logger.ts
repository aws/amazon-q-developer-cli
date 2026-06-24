import { writeFileSync, appendFileSync, existsSync, mkdirSync } from 'fs';
import { tmpdir } from 'os';
import { join } from 'path';

type LogLevel = 'error' | 'warn' | 'info' | 'debug' | 'trace';

// JSON.stringify(new Error('x')) returns "{}" because message/stack/name are
// non-enumerable. Pull them off explicitly so error logs aren't black holes.
export function formatArg(arg: unknown): string {
  if (arg instanceof Error) {
    const parts: Record<string, unknown> = {
      name: arg.name,
      message: arg.message,
    };
    if (arg.stack) parts.stack = arg.stack;
    if ((arg as any).code !== undefined) parts.code = (arg as any).code;
    if ((arg as any).cause !== undefined) {
      parts.cause = formatArg((arg as any).cause);
    }
    return JSON.stringify(parts);
  }
  if (typeof arg === 'object' && arg !== null) {
    try {
      return JSON.stringify(arg);
    } catch {
      return String(arg);
    }
  }
  return String(arg);
}

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

    // Per-launch separator: write a banner on every TUI start (not only
    // first-ever launch). Without this, multiple TUI processes appending to
    // the same file are indistinguishable in the log — a stdin EOF + restart
    // looks identical to a single long-running process. The banner anchors
    // post-mortem reads to a specific PID/launch.
    if (this.logFile) {
      try {
        if (!existsSync(this.logFile)) {
          writeFileSync(
            this.logFile,
            `=== TUI Log Started ${new Date().toISOString()} pid=${process.pid} ===\n`
          );
        } else {
          appendFileSync(
            this.logFile,
            `=== TUI Launch ${new Date().toISOString()} pid=${process.pid} ===\n`
          );
        }
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
      args.length > 0 ? ' ' + args.map(formatArg).join(' ') : '';

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
