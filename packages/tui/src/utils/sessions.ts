/**
 * Session discovery utilities for --resume and --resume-picker.
 *
 * Reads V2 session JSON files from ~/.kiro/sessions/cli/ to find
 * sessions matching the current working directory.
 */

import { existsSync, readdirSync, readFileSync, realpathSync } from 'fs';
import { join, resolve } from 'path';
import { homedir } from 'os';
import { logger } from './logger.js';

export interface SessionEntry {
  sessionId: string;
  cwd: string;
  createdAt: string;
  updatedAt: string;
  msgCount: number;
  summary: string;
}

function getSessionsDir(): string {
  return (
    process.env.KIRO_TEST_SESSIONS_DIR ??
    join(homedir(), '.kiro', 'sessions', 'cli')
  );
}

function canonicalize(p: string): string {
  try {
    return realpathSync(p);
  } catch {
    return resolve(p);
  }
}

/**
 * List all sessions for the given working directory, sorted by most recent first.
 */
export function listSessionsForCwd(cwd: string): SessionEntry[] {
  const sessionsDir = getSessionsDir();
  const canonicalCwd = canonicalize(cwd);

  let files: string[];
  try {
    files = readdirSync(sessionsDir).filter((f) => f.endsWith('.json'));
  } catch {
    return [];
  }

  const sessions: SessionEntry[] = [];

  for (const file of files) {
    try {
      const content = readFileSync(join(sessionsDir, file), 'utf-8');
      const data = JSON.parse(content);

      const sessionCwd = canonicalize(data.cwd ?? '');
      if (sessionCwd !== canonicalCwd) continue;

      const sessionId = data.session_id ?? file.replace('.json', '');

      // Read the .jsonl event log for message count and last user prompt
      const { msgCount, lastUserPrompt } = readEventLog(
        join(sessionsDir, sessionId + '.jsonl')
      );

      sessions.push({
        sessionId,
        cwd: data.cwd,
        createdAt: data.created_at ?? '',
        updatedAt: data.updated_at ?? '',
        msgCount,
        summary: formatSummary(lastUserPrompt),
      });
    } catch (err) {
      logger.debug(`Skipping malformed session file ${file}:`, err);
    }
  }

  // Sort by updated_at descending (most recent first)
  sessions.sort((a, b) => b.updatedAt.localeCompare(a.updatedAt));
  return sessions;
}

/**
 * Get the most recent session ID for the current working directory.
 * Returns undefined if no sessions exist.
 */
export function getMostRecentSessionId(cwd: string): string | undefined {
  const sessions = listSessionsForCwd(cwd);
  return sessions[0]?.sessionId;
}

/**
 * Read the JSONL event log file to extract message count and last user prompt.
 *
 * V2 stores conversation history as append-only JSONL alongside the .json metadata.
 * Each line is a LogEntry: { "version": "v1", "kind": "Prompt"|"AssistantMessage"|..., "data": { ... } }
 */
function readEventLog(jsonlPath: string): {
  msgCount: number;
  lastUserPrompt: string | undefined;
} {
  if (!existsSync(jsonlPath)) {
    return { msgCount: 0, lastUserPrompt: undefined };
  }

  try {
    const content = readFileSync(jsonlPath, 'utf-8');
    const lines = content.split('\n').filter((l) => l.trim().length > 0);
    let lastUserPrompt: string | undefined;

    // Walk backwards to find the last Prompt entry
    for (let i = lines.length - 1; i >= 0; i--) {
      try {
        const entry = JSON.parse(lines[i]!);
        if (entry?.kind === 'Prompt' && entry?.data?.content) {
          const textBlock = entry.data.content.find(
            (b: any) => b.kind === 'text' && typeof b.data === 'string'
          );
          if (textBlock) {
            lastUserPrompt = textBlock.data;
            break;
          }
        }
      } catch {
        // skip malformed lines
      }
    }

    return { msgCount: lines.length, lastUserPrompt };
  } catch {
    return { msgCount: 0, lastUserPrompt: undefined };
  }
}

/**
 * Format a user prompt string as a summary line.
 * Matches V1: single-line, truncated at 150 chars.
 */
function formatSummary(prompt: string | undefined): string {
  if (!prompt) return '(empty conversation)';
  const singleLine = prompt.replace(/[\n\r]/g, ' ');
  return singleLine.length > 150
    ? singleLine.slice(0, 150) + '...'
    : singleLine;
}

/**
 * Format an ISO date string (or any parseable date) as a relative time string.
 * Matches V1's format_timestamp: "X seconds/minutes/hours/days ago"
 */
export function formatRelativeTime(dateStr: string): string {
  try {
    const then = new Date(dateStr).getTime();
    if (isNaN(then)) return 'unknown';
    const now = Date.now();
    const diffSecs = Math.floor((now - then) / 1000);

    if (diffSecs < 60) return `${diffSecs} seconds ago`;
    if (diffSecs < 3600) return `${Math.floor(diffSecs / 60)} minutes ago`;
    if (diffSecs < 86400) return `${Math.floor(diffSecs / 3600)} hours ago`;
    return `${Math.floor(diffSecs / 86400)} days ago`;
  } catch {
    return 'unknown';
  }
}

/**
 * Format a session entry for display in the picker.
 * Matches V1 format: "{relative_time} | {summary} | {count} msgs"
 */
export function formatSessionEntry(entry: SessionEntry): string {
  const timestamp = entry.updatedAt
    ? formatRelativeTime(entry.updatedAt)
    : 'unknown';
  return `${timestamp} | ${entry.summary} | ${entry.msgCount} msgs`;
}
