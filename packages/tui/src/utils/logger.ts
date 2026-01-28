import { writeFileSync, appendFileSync, existsSync } from 'fs';
import { join } from 'path';

type LogLevel = 'error' | 'warn' | 'info' | 'debug' | 'trace';

const LOG_LEVELS: Record<LogLevel, number> = {
  error: 0,
  warn: 1,
  info: 2,
  debug: 3,
  trace: 4,
};

class Logger {
  private logFile: string | null;
  private logLevel: LogLevel;

  constructor() {
    this.logFile = process.env.KIRO_TUI_LOG_FILE || null;
    this.logLevel = (process.env.KIRO_TUI_LOG_LEVEL as LogLevel) || 'info';

    // Initialize log file if configured
    if (this.logFile && !existsSync(this.logFile)) {
      writeFileSync(
        this.logFile,
        `=== TUI Log Started ${new Date().toISOString()} ===\n`
      );
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
      // Fallback to console if file write fails
      console.error('Logger write failed:', error);
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
