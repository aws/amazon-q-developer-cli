import { homedir } from 'os';
import { join } from 'path';
import { readFileSync, writeFileSync, mkdirSync, existsSync } from 'fs';
import { logger } from './logger.js';

const HISTORY_FILE = join(homedir(), '.kiro', '.cli_bash_history');
const MAX_HISTORY_SIZE = 1000;

export class CommandHistory {
  private static instance: CommandHistory;
  private history: string[] = [];
  private currentIndex = -1;

  private constructor() {
    this.history = this.load();
  }

  static getInstance(): CommandHistory {
    if (!CommandHistory.instance) {
      CommandHistory.instance = new CommandHistory();
    }
    return CommandHistory.instance;
  }

  private load(): string[] {
    try {
      if (existsSync(HISTORY_FILE)) {
        const content = readFileSync(HISTORY_FILE, 'utf-8');
        return content.split('\n').filter(line => line.trim()).slice(-MAX_HISTORY_SIZE);
      }
    } catch (err) {
      logger.warn('Failed to load history:', err);
    }
    return [];
  }

  private save(): void {
    try {
      const dir = join(homedir(), '.kiro');
      if (!existsSync(dir)) {
        mkdirSync(dir, { recursive: true });
      }
      const content = this.history.slice(-MAX_HISTORY_SIZE).join('\n') + '\n';
      writeFileSync(HISTORY_FILE, content, 'utf-8');
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

  navigate(direction: 'up' | 'down'): string | null {
    if (this.history.length === 0) return null;

    if (direction === 'up') {
      this.currentIndex = this.currentIndex === -1 
        ? this.history.length - 1 
        : Math.max(0, this.currentIndex - 1);
      return this.history[this.currentIndex] ?? null;
    } else {
      if (this.currentIndex === -1) return null;
      this.currentIndex++;
      if (this.currentIndex >= this.history.length) {
        this.currentIndex = -1;
        return null;
      }
      return this.history[this.currentIndex] ?? null;
    }
  }

  reset(): void {
    this.currentIndex = -1;
  }

  clear(): void {
    this.history = [];
    this.currentIndex = -1;
  }

  getAll(): string[] {
    return [...this.history];
  }
}
