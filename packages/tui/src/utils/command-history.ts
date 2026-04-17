import { homedir } from 'os';
import { join } from 'path';
import { readFileSync, writeFileSync, mkdirSync, existsSync } from 'fs';
import { logger } from './logger.js';

const DEFAULT_HISTORY_FILE = join(homedir(), '.kiro', '.cli_bash_history');
const MAX_HISTORY_SIZE = 1000;

export class CommandHistory {
  private static instance: CommandHistory;
  private historyFile: string;
  private history: string[] = [];
  private currentIndex = -1;
  private savedInput: string | null = null;

  private constructor(historyFile?: string) {
    this.historyFile = historyFile ?? DEFAULT_HISTORY_FILE;
    this.history = this.load();
  }

  static getInstance(): CommandHistory {
    if (!CommandHistory.instance) {
      CommandHistory.instance = new CommandHistory();
    }
    return CommandHistory.instance;
  }

  /** Create a standalone instance with a custom history file path. Useful for testing. */
  static createWithFile(historyFile: string): CommandHistory {
    return new CommandHistory(historyFile);
  }

  private load(): string[] {
    try {
      if (existsSync(this.historyFile)) {
        const content = readFileSync(this.historyFile, 'utf-8');
        return content
          .split('\n')
          .filter((line) => line.trim())
          .map((line) => line.replaceAll('\x00', '\n'))
          .slice(-MAX_HISTORY_SIZE);
      }
    } catch (err) {
      logger.warn('Failed to load history:', err);
    }
    return [];
  }

  private save(): void {
    try {
      const dir = join(this.historyFile, '..');
      if (!existsSync(dir)) {
        mkdirSync(dir, { recursive: true });
      }
      const content =
        this.history
          .slice(-MAX_HISTORY_SIZE)
          .map((entry) => entry.replaceAll('\n', '\\n'))
          .join('\n') + '\n';
      writeFileSync(this.historyFile, content, 'utf-8');
    } catch (err) {
      logger.warn('Failed to save history:', err);
    }
  }

  add(command: string): void {
    const trimmed = command.trim();
    if (!trimmed) return;

    this.history = [...this.history, trimmed].slice(-MAX_HISTORY_SIZE);
    this.currentIndex = -1;
    this.save();
  }

  navigate(direction: 'up' | 'down', currentInput?: string): string | null {
    if (this.history.length === 0) return null;

    if (direction === 'up') {
      // Save current input before first navigation
      if (this.currentIndex === -1) {
        this.savedInput = currentInput ?? '';
      }
      this.currentIndex =
        this.currentIndex === -1
          ? this.history.length - 1
          : Math.max(0, this.currentIndex - 1);
      return this.history[this.currentIndex] ?? null;
    } else {
      if (this.currentIndex === -1) return null;
      this.currentIndex++;
      if (this.currentIndex >= this.history.length) {
        // Restore saved input when returning past newest history entry
        const restored = this.savedInput;
        this.currentIndex = -1;
        this.savedInput = null;
        return restored ?? '';
      }
      return this.history[this.currentIndex] ?? null;
    }
  }

  isNavigating(): boolean {
    return this.currentIndex !== -1;
  }

  reset(): void {
    this.currentIndex = -1;
    this.savedInput = null;
  }

  /** Set the navigation index directly (e.g. after reverse search acceptance). */
  setIndex(index: number): void {
    this.currentIndex = index;
  }

  clear(): void {
    this.history = [];
    this.currentIndex = -1;
    this.savedInput = null;
  }

  getAll(): string[] {
    return [...this.history];
  }
}
