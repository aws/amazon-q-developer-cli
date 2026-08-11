/**
 * KAS-native session turn tree.
 *
 * The KAS store records a session as `messages.jsonl` (one event per line,
 * each with a `payload.type`) plus a `sub-executions/` directory of per-
 * subagent logs. This builds a turn-structured view where each user turn
 * carries its assistant response, tool calls, and any subagents spawned
 * from that turn nested underneath.
 *
 * Distinct from `session-preview.ts`, which reads the V2 flat store
 * (`{id}.jsonl` of tagged LogEntry lines). KAS-native sessions live under
 * `~/.kiro/sessions/{workspace-hash}/sess_{id}/`.
 */

import { existsSync, readdirSync } from 'node:fs';
import { readBoundedFileTail } from './bounded-json.js';
import {
  containedDirectory,
  containedRegularFile,
  isValidSessionId,
} from './session-store.js';

const MAIN_TRANSCRIPT_BYTES = 256 * 1024;
const SUBEXECUTION_BYTES = 64 * 1024;
const MAX_SUBEXECUTION_FILES = 64;

/** A subagent run spawned from a turn. */
export interface SubagentRun {
  subExecutionId: string;
  /** Count of tool calls the subagent made (cheap activity signal). */
  toolCallCount: number;
  /** First assistant text from the subagent, if any (a rough title). */
  summary: string;
}

/** One conversation turn: a user prompt + the agent's response to it. */
export interface SessionTurn {
  /** KAS execution id grouping this turn's events. */
  executionId: string;
  /** User prompt text for the turn (may be empty for the first boot turn). */
  userText: string;
  /** Assistant response text (concatenated). */
  assistantText: string;
  /** Names of tools the main agent called this turn. */
  toolNames: string[];
  /** Subagents spawned from this turn, nested underneath it. */
  subagents: SubagentRun[];
}

interface KasEvent {
  payload?: {
    type?: string;
    executionId?: string;
    subExecutionId?: string;
    text?: string;
    content?: unknown;
    name?: string;
    toolName?: string;
    role?: string;
  };
}

/** Extract plain text from a KAS payload's `text` or `content` field. */
function payloadText(payload: KasEvent['payload']): string {
  if (!payload) return '';
  if (typeof payload.text === 'string') return payload.text;
  const content = payload.content;
  if (typeof content === 'string') return content;
  if (Array.isArray(content)) {
    return content
      .map((b) =>
        b &&
        typeof b === 'object' &&
        typeof (b as { text?: string }).text === 'string'
          ? (b as { text: string }).text
          : ''
      )
      .filter(Boolean)
      .join(' ');
  }
  return '';
}

function readJsonl(path: string, maxBytes: number): KasEvent[] {
  if (!existsSync(path)) return [];
  try {
    const { bytes, complete } = readBoundedFileTail(path, maxBytes);
    let content = bytes.toString('utf-8');
    if (!complete) {
      const nl = content.indexOf('\n');
      content = nl >= 0 ? content.slice(nl + 1) : '';
    }
    return content
      .split('\n')
      .filter((l) => l.trim())
      .map((l) => {
        try {
          return JSON.parse(l) as KasEvent;
        } catch {
          return null;
        }
      })
      .filter((e): e is KasEvent => e !== null);
  } catch {
    return [];
  }
}

/**
 * Parse a subagent's execution log into a compact {@link SubagentRun}.
 */
function parseSubagent(
  events: KasEvent[],
  subExecutionId: string
): SubagentRun {
  let toolCallCount = 0;
  let summary = '';
  for (const e of events) {
    const t = e.payload?.type;
    if (t === 'tool_call') toolCallCount++;
    else if (t === 'assistant' && !summary) {
      const text = payloadText(e.payload).replace(/\s+/g, ' ').trim();
      if (text) summary = text.slice(0, 120);
    }
  }
  return { subExecutionId, toolCallCount, summary };
}

/**
 * Build the turn tree for a KAS-native session directory
 * (`.../sess_<id>/` containing `messages.jsonl` + `sub-executions/`).
 *
 * Returns turns in chronological order, each with its subagents nested.
 */
export function buildKasTurnTree(sessionDir: string): SessionTurn[] {
  const messagesPath = containedRegularFile(sessionDir, 'messages.jsonl');
  if (!messagesPath) return [];
  const events = readJsonl(messagesPath, MAIN_TRANSCRIPT_BYTES);
  if (events.length === 0) return [];

  // Group main-thread events by executionId, preserving first-seen order.
  const turnOrder: string[] = [];
  const turns = new Map<string, SessionTurn>();
  // A `user` prompt precedes its turn_start with no executionId — buffered here.
  let pendingUserText = '';
  const ensure = (executionId: string): SessionTurn => {
    let turn = turns.get(executionId);
    if (!turn) {
      turn = {
        executionId,
        userText: '',
        assistantText: '',
        toolNames: [],
        subagents: [],
      };
      turns.set(executionId, turn);
      turnOrder.push(executionId);
    }
    return turn;
  };

  for (const e of events) {
    const p = e.payload;
    if (!p) continue;
    const text = payloadText(p).replace(/\s+/g, ' ').trim();

    // A `user` prompt is emitted BEFORE its turn_start and carries no
    // executionId — buffer it and attach to the next turn that opens.
    if ((p.type === 'user' || p.type === 'prompt') && !p.executionId) {
      if (text)
        pendingUserText = pendingUserText ? `${pendingUserText} ${text}` : text;
      continue;
    }

    if (!p.executionId) continue;

    // A real conversational turn is defined by `turn_start`. Other events
    // carry an executionId too (approval interactions use tool-call ids like
    // `toolu_…`/`run_command…`), so ONLY turn_start opens a turn — otherwise
    // every tool interaction would become a spurious "(no prompt)" turn.
    if (p.type === 'turn_start') {
      const turn = ensure(p.executionId);
      if (pendingUserText) {
        turn.userText = turn.userText
          ? `${turn.userText} ${pendingUserText}`
          : pendingUserText;
        pendingUserText = '';
      }
      continue;
    }

    // Content events attach to an already-opened turn; if none exists for
    // this id (a non-turn interaction id), drop the event.
    const turn = turns.get(p.executionId);
    if (!turn) continue;
    switch (p.type) {
      case 'user':
      case 'prompt':
        if (text)
          turn.userText = turn.userText ? `${turn.userText} ${text}` : text;
        break;
      case 'assistant':
        if (text)
          turn.assistantText = turn.assistantText
            ? `${turn.assistantText} ${text}`
            : text;
        break;
      case 'tool_call': {
        const name = p.toolName || p.name;
        if (name && !turn.toolNames.includes(name)) turn.toolNames.push(name);
        break;
      }
      default:
        break;
    }
  }

  // Attach subagents: each sub-execution file's first entry carries the
  // parent executionId. Group them under the matching turn.
  const subDir = containedDirectory(sessionDir, 'sub-executions');
  if (subDir) {
    let subFiles: string[];
    try {
      subFiles = readdirSync(subDir)
        .filter((f) => f.endsWith('.jsonl'))
        .slice(0, MAX_SUBEXECUTION_FILES);
    } catch {
      subFiles = [];
    }
    let remainingBytes = MAX_SUBEXECUTION_FILES * SUBEXECUTION_BYTES;
    for (const file of subFiles) {
      const subPath = containedRegularFile(subDir, file);
      if (!subPath || remainingBytes <= 0) continue;
      const maxBytes = Math.min(SUBEXECUTION_BYTES, remainingBytes);
      remainingBytes -= maxBytes;
      const events = readJsonl(subPath, maxBytes);
      const first = events[0];
      const executionId = first?.payload?.executionId;
      const subExecutionId =
        first?.payload?.subExecutionId ?? file.replace('.jsonl', '');
      if (!executionId) continue;
      const turn = turns.get(executionId);
      if (turn) {
        turn.subagents.push(parseSubagent(events, subExecutionId));
      }
    }
  }

  return turnOrder.map((id) => turns.get(id)!);
}

/**
 * Flat turn list for a V2-store session (`cli/{id}.jsonl`). V2 logs record
 * no subagent lineage, so every turn is top-level: a `Prompt` opens a turn,
 * `AssistantMessage` blocks fill its response text and tool names.
 */
export function buildV2TurnList(
  cliDir: string,
  sessionId: string
): SessionTurn[] {
  if (!isValidSessionId(sessionId)) return [];
  const logPath = containedRegularFile(cliDir, `${sessionId}.jsonl`);
  if (!logPath) return [];
  const events = readJsonl(logPath, MAIN_TRANSCRIPT_BYTES);
  if (events.length === 0) return [];

  const turns: SessionTurn[] = [];
  let current: SessionTurn | null = null;
  const text = (content: unknown[]): string => {
    const out: string[] = [];
    for (const block of content) {
      const b = block as { kind?: string; data?: unknown } | null;
      if (b?.kind === 'text' && typeof b.data === 'string') out.push(b.data);
    }
    return out.join('\n');
  };

  for (const ev of events) {
    const e = ev as { kind?: string; data?: { content?: unknown[] } };
    if (!e.kind || !e.data) continue;
    if (e.kind === 'Prompt' && Array.isArray(e.data.content)) {
      current = {
        executionId: `turn-${turns.length + 1}`,
        userText: text(e.data.content),
        assistantText: '',
        toolNames: [],
        subagents: [],
      };
      turns.push(current);
    } else if (
      e.kind === 'AssistantMessage' &&
      Array.isArray(e.data.content) &&
      current
    ) {
      const t = text(e.data.content);
      if (t) {
        current.assistantText = current.assistantText
          ? `${current.assistantText}\n${t}`
          : t;
      }
      for (const block of e.data.content) {
        const b = block as {
          kind?: string;
          data?: { name?: string };
        } | null;
        if (b?.kind === 'tool_use' && b.data?.name) {
          current.toolNames.push(b.data.name);
        }
      }
    }
  }
  return turns;
}
