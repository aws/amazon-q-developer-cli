/**
 * Session preview — fetch conversation snippets without loading.
 *
 * Reads the JSONL log for a given session and extracts:
 * 1. Summary: title, first prompt, tool summary, turn count
 * 2. Recent messages: the last N user/assistant exchanges
 *
 * Does NOT acquire a session lock or load the session as active.
 * Uses an LRU cache to avoid repeated disk reads on rapid cursor navigation.
 */

import { existsSync } from 'node:fs';
import { basename, join } from 'node:path';
import { kiroHomePath } from './kiro-home.js';
import {
  SESSION_METADATA_MAX_BYTES,
  readBoundedFileTail,
  readBoundedJson,
} from './bounded-json.js';
import { logger } from './logger.js';
import {
  containedRegularFile,
  findKasSessionDir,
  v2SessionPath,
} from './session-store.js';

/**
 * Read session metadata, tolerating oversized or corrupt files. The
 * transcript is what the preview renders — a metadata file over the read
 * limit (large legacy V2 sessions carry multi-MiB metadata) or a parse
 * error must degrade the title, not blank the whole preview.
 */
function readSessionMetaLenient(path: string): Record<string, unknown> {
  try {
    const value = readBoundedJson(path, SESSION_METADATA_MAX_BYTES);
    return value && typeof value === 'object' && !Array.isArray(value)
      ? (value as Record<string, unknown>)
      : {};
  } catch {
    return {};
  }
}

/**
 * Read the last `maxBytes` of a file and return complete lines (dropping
 * the first partial line at the slice boundary). Small files are read whole.
 * Prevents the event-loop freeze that a full readFileSync causes on 18MB+
 * transcripts.
 */
function readTailLines(
  filePath: string,
  maxBytes: number
): { lines: string[]; complete: boolean } {
  const { bytes, complete } = readBoundedFileTail(filePath, maxBytes);
  let content = bytes.toString('utf-8');
  if (!complete) {
    const nl = content.indexOf('\n');
    content = nl >= 0 ? content.slice(nl + 1) : '';
  }
  return {
    lines: content.split('\n').filter((line) => line.trim()),
    complete,
  };
}

/**
 * Summary view of a session (shown in the "Summary" preview tab).
 */
export interface SessionPreviewSummary {
  title: string;
  firstPrompt: string;
  /** Distinct tool names used in the session. */
  toolsSummary: string[];
  /** Number of user turns in the read scope. */
  turnCount: number;
  /** True when summary values cover the entire transcript. */
  isComplete: boolean;
  createdAt: string;
  updatedAt: string;
}

/**
 * A single message in the recent-messages view.
 */
export interface PreviewMessage {
  role: 'user' | 'assistant';
  /** Truncated text content (max ~300 chars). */
  content: string;
  /** Timestamp from message metadata, if available. */
  timestamp?: string;
}

/**
 * Full preview data for one session.
 */
export interface SessionPreview {
  sessionId: string;
  summary: SessionPreviewSummary;
  /** Most recent user/assistant exchanges (last 5 pairs). */
  recentMessages: PreviewMessage[];
}

/**
 * Simple LRU cache for session previews.
 */
class LRUCache<K, V> {
  private cache = new Map<K, V>();
  private maxSize: number;

  constructor(maxSize: number) {
    this.maxSize = maxSize;
  }

  get(key: K): V | undefined {
    const value = this.cache.get(key);
    if (value !== undefined) {
      // Move to end (most recently used).
      this.cache.delete(key);
      this.cache.set(key, value);
    }
    return value;
  }

  set(key: K, value: V): void {
    if (this.cache.has(key)) {
      this.cache.delete(key);
    } else if (this.cache.size >= this.maxSize) {
      // Evict the oldest entry (first in map).
      const firstKey = this.cache.keys().next().value!;
      this.cache.delete(firstKey);
    }
    this.cache.set(key, value);
  }

  has(key: K): boolean {
    return this.cache.get(key) !== undefined;
  }

  delete(key: K): void {
    this.cache.delete(key);
  }

  clear(): void {
    this.cache.clear();
  }

  get size(): number {
    return this.cache.size;
  }
}

const MAX_PREVIEW_CACHE = 20;
const MAX_RECENT_MESSAGES = 10;
const MAX_CONTENT_LENGTH = 300;

/**
 * Fetches a preview of a session without loading it as active.
 *
 * Reads the session's .json metadata and .jsonl log directly from disk.
 * Results are cached in an LRU cache (max 20 entries).
 */
export class SessionPreviewProvider {
  private cache: LRUCache<string, SessionPreview>;
  private sessionsDir: string;

  constructor(sessionsDir?: string) {
    this.cache = new LRUCache(MAX_PREVIEW_CACHE);
    this.sessionsDir =
      sessionsDir ??
      process.env.KIRO_TEST_SESSIONS_DIR ??
      kiroHomePath('sessions', 'cli');
  }

  /**
   * Get a preview for the given session. Returns from cache if available.
   * Returns null if the session doesn't exist or can't be read.
   */
  getPreview(
    sessionId: string,
    engine?: 'classic' | 'v2' | 'v3',
    source: 'local' | 'remote' = 'local'
  ): SessionPreview | null {
    if (source === 'remote') return null;
    const cacheKey = `${source}\u0000${engine ?? 'unknown'}\u0000${sessionId}`;
    const cached = this.cache.get(cacheKey);
    if (cached) return cached;

    const preview = this.buildPreview(sessionId, engine);
    if (preview) {
      this.cache.set(cacheKey, preview);
    }
    return preview;
  }

  /** Invalidate cached previews after the session changes. */
  invalidate(sessionId: string): void {
    for (const source of ['local', 'remote'] as const) {
      for (const engine of ['unknown', 'classic', 'v2', 'v3'] as const) {
        this.cache.delete(`${source}\u0000${engine}\u0000${sessionId}`);
      }
    }
  }

  /**
   * Clear the entire cache.
   */
  clearCache(): void {
    this.cache.clear();
  }

  get cacheSize(): number {
    return this.cache.size;
  }

  private buildPreview(
    sessionId: string,
    engine?: 'classic' | 'v2' | 'v3'
  ): SessionPreview | null {
    try {
      if (engine === 'classic') return null;
      // V2 store: flat files in the cli/ directory.
      const storeRoot = join(this.sessionsDir, '..');
      const metaPath =
        basename(this.sessionsDir) === 'cli'
          ? v2SessionPath(storeRoot, sessionId, '.json')
          : containedRegularFile(this.sessionsDir, `${sessionId}.json`);
      if (engine !== 'v3' && metaPath && existsSync(metaPath)) {
        const meta = readSessionMetaLenient(metaPath);
        const logPath =
          basename(this.sessionsDir) === 'cli'
            ? v2SessionPath(storeRoot, sessionId, '.jsonl')
            : containedRegularFile(this.sessionsDir, `${sessionId}.jsonl`);
        if (
          !logPath &&
          existsSync(join(this.sessionsDir, `${sessionId}.jsonl`))
        ) {
          return null;
        }
        const { summary, recentMessages } = this.extractPreviewData(
          logPath ?? join(this.sessionsDir, `${sessionId}.jsonl`),
          meta
        );
        return { sessionId, summary, recentMessages };
      }

      // KAS-native store: per-workspace hash directories.
      if (engine !== 'v2' && basename(this.sessionsDir) === 'cli') {
        const sessionsRoot = join(this.sessionsDir, '..');
        const kasDir = findKasSessionDir(sessionsRoot, sessionId);
        if (kasDir) {
          const kasMetaPath = containedRegularFile(kasDir, 'session.json');
          if (!kasMetaPath) return null;
          const kasMeta = readSessionMetaLenient(kasMetaPath);
          const lexicalLogPath = join(kasDir, 'messages.jsonl');
          const logPath = containedRegularFile(kasDir, 'messages.jsonl');
          if (!logPath && existsSync(lexicalLogPath)) return null;
          const { summary, recentMessages } = this.extractKasPreviewData(
            logPath ?? lexicalLogPath,
            kasMeta
          );
          return { sessionId, summary, recentMessages };
        }
      }

      return null;
    } catch (err) {
      logger.debug(
        `[session-preview] Failed to build preview for ${sessionId}:`,
        err
      );
      return null;
    }
  }

  private extractKasPreviewData(
    logPath: string,
    meta: Record<string, unknown>
  ): { summary: SessionPreviewSummary; recentMessages: PreviewMessage[] } {
    const title = (meta.title as string) || '';
    const createdAt = (meta.createdAt as string) || '';
    const updatedAt = (meta.lastModifiedAt as string) || '';

    if (!existsSync(logPath)) {
      return {
        summary: {
          title: title || '(empty session)',
          firstPrompt: '',
          toolsSummary: [],
          turnCount: 0,
          isComplete: true,
          createdAt,
          updatedAt,
        },
        recentMessages: [],
      };
    }

    try {
      // Read only the last 64KB — enough for ~10 recent rounds. Large
      // transcripts can be 18MB+; a full synchronous read freezes the UI.
      const TAIL_BYTES = 64 * 1024;
      const { lines, complete: wholeFileRead } = readTailLines(
        logPath,
        TAIL_BYTES
      );

      let parsedAll = true;
      let firstPrompt = '';
      let turnCount = 0;
      const tools = new Set<string>();
      const messages: PreviewMessage[] = [];

      for (const line of lines) {
        try {
          const entry = JSON.parse(line);
          const payload = entry?.payload;
          if (!payload) continue;
          if (payload.type === 'user' && typeof payload.content === 'string') {
            turnCount++;
            if (!firstPrompt)
              firstPrompt = truncate(payload.content, MAX_CONTENT_LENGTH);
            messages.push({
              role: 'user',
              content: truncate(payload.content, MAX_CONTENT_LENGTH),
              timestamp: entry.timestamp,
            });
          } else if (payload.type === 'assistant') {
            // KAS assistant content is a plain string (not an array of blocks).
            const text =
              typeof payload.content === 'string'
                ? payload.content
                : Array.isArray(payload.content)
                  ? (payload.content as { type?: string; text?: string }[])
                      .filter((b) => b?.type === 'text' && b.text)
                      .map((b) => b.text)
                      .join('\n')
                  : '';
            if (Array.isArray(payload.content)) {
              for (const block of payload.content as {
                type?: string;
                name?: string;
              }[]) {
                if (block?.type === 'tool_use' && block.name)
                  tools.add(block.name);
              }
            }
            if (text) {
              messages.push({
                role: 'assistant',
                content: truncate(text, MAX_CONTENT_LENGTH),
                timestamp: entry.timestamp,
              });
            }
          }
        } catch {
          parsedAll = false;
        }
      }

      const isComplete = wholeFileRead && parsedAll;
      const recentMessages =
        collapseAssistantRuns(messages).slice(-MAX_RECENT_MESSAGES);

      return {
        summary: {
          title:
            title ||
            (isComplete ? truncate(firstPrompt, 100) : '') ||
            '(empty session)',
          firstPrompt: isComplete ? firstPrompt : '',
          toolsSummary: [...tools].slice(0, 10),
          turnCount,
          isComplete,
          createdAt,
          updatedAt,
        },
        recentMessages,
      };
    } catch {
      return {
        summary: {
          title: title || '(unreadable session)',
          firstPrompt: '',
          toolsSummary: [],
          turnCount: 0,
          isComplete: false,
          createdAt,
          updatedAt,
        },
        recentMessages: [],
      };
    }
  }

  private extractPreviewData(
    logPath: string,
    meta: Record<string, unknown>
  ): { summary: SessionPreviewSummary; recentMessages: PreviewMessage[] } {
    const title = (meta.title as string) || '';
    const createdAt = (meta.created_at as string) || '';
    const updatedAt = (meta.updated_at as string) || '';

    if (!existsSync(logPath)) {
      return {
        summary: {
          title: title || '(empty session)',
          firstPrompt: '',
          toolsSummary: [],
          turnCount: 0,
          isComplete: true,
          createdAt,
          updatedAt,
        },
        recentMessages: [],
      };
    }

    try {
      // Cap to last 64KB (~10 recent rounds). V2 transcripts can reach 19MB.
      const TAIL_BYTES = 64 * 1024;
      const { lines, complete: wholeFileRead } = readTailLines(
        logPath,
        TAIL_BYTES
      );

      let parsedAll = true;
      let firstPrompt = '';
      let turnCount = 0;
      const tools = new Set<string>();
      const messages: PreviewMessage[] = [];

      for (const line of lines) {
        try {
          const entry = JSON.parse(line);
          const kind = entry?.kind;
          const data = entry?.data;
          if (!kind || !data) continue;

          if (kind === 'Prompt' && Array.isArray(data.content)) {
            turnCount++;
            const text = extractTextContent(data.content);
            if (!firstPrompt && text) {
              firstPrompt = truncate(text, MAX_CONTENT_LENGTH);
            }
            if (text) {
              messages.push({
                role: 'user',
                content: truncate(text, MAX_CONTENT_LENGTH),
                timestamp: data.meta?.timestamp,
              });
            }
          } else if (
            kind === 'AssistantMessage' &&
            Array.isArray(data.content)
          ) {
            const text = extractTextContent(data.content);
            // Collect tool names from tool_use blocks.
            for (const block of data.content) {
              if (block?.kind === 'tool_use' && block.data?.name) {
                tools.add(block.data.name);
              }
            }
            if (text) {
              messages.push({
                role: 'assistant',
                content: truncate(text, MAX_CONTENT_LENGTH),
              });
            }
          } else if (kind === 'ToolResults' && data.results) {
            for (const key of Object.keys(data.results)) {
              const result = data.results[key];
              if (result?.tool?.name) {
                tools.add(result.tool.name);
              }
            }
          }
        } catch {
          parsedAll = false;
        }
      }

      const isComplete = wholeFileRead && parsedAll;
      // Keep only the last N messages.
      const recentMessages =
        collapseAssistantRuns(messages).slice(-MAX_RECENT_MESSAGES);

      return {
        summary: {
          title: title || (isComplete ? firstPrompt : '') || '(empty session)',
          firstPrompt: isComplete ? firstPrompt : '',
          toolsSummary: [...tools],
          turnCount,
          isComplete,
          createdAt,
          updatedAt,
        },
        recentMessages,
      };
    } catch {
      return {
        summary: {
          title: title || '(unreadable session)',
          firstPrompt: '',
          toolsSummary: [],
          turnCount: 0,
          isComplete: false,
          createdAt,
          updatedAt,
        },
        recentMessages: [],
      };
    }
  }
}

/**
 * Extract concatenated text from a content blocks array.
 */
function extractTextContent(content: unknown[]): string {
  const texts: string[] = [];
  for (const block of content) {
    if (
      block &&
      typeof block === 'object' &&
      'kind' in block &&
      'data' in block &&
      (block as { kind: string }).kind === 'text' &&
      typeof (block as { data: unknown }).data === 'string'
    ) {
      texts.push((block as { data: string }).data);
    }
  }
  return texts.join('\n');
}

/**
 * Collapse each run of consecutive assistant messages to its final entry.
 * A long agent working stretch logs dozens of narration segments ("Now let
 * me..."); the conclusion is the one worth previewing, and collapsing keeps
 * earlier user turns inside the recent-messages window.
 */
function collapseAssistantRuns(messages: PreviewMessage[]): PreviewMessage[] {
  const collapsed: PreviewMessage[] = [];
  for (const message of messages) {
    const prev = collapsed[collapsed.length - 1];
    if (message.role === 'assistant' && prev?.role === 'assistant') {
      collapsed[collapsed.length - 1] = message;
    } else {
      collapsed.push(message);
    }
  }
  return collapsed;
}

/**
 * Truncate text to a maximum length, adding ellipsis if needed.
 * Replaces newlines with spaces for single-line display.
 */
function truncate(text: string, maxLen: number): string {
  const singleLine = text.replace(/[\n\r]+/g, ' ').trim();
  if (singleLine.length <= maxLen) return singleLine;
  return singleLine.slice(0, maxLen - 3) + '...';
}

/**
 * Singleton preview provider instance.
 */
let globalProvider: SessionPreviewProvider | null = null;

export function getSessionPreviewProvider(): SessionPreviewProvider {
  if (!globalProvider) {
    globalProvider = new SessionPreviewProvider();
  }
  return globalProvider;
}

export function resetSessionPreviewProvider(): void {
  globalProvider = null;
}
