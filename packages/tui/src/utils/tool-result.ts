import type { ToolResult } from '../stores/app-store.js';

/** Parse a JSON tool call content string and extract a specific field */
export function parseToolArg(
  content: string | undefined,
  field: string
): string | null {
  if (!content) return null;
  try {
    return JSON.parse(content)[field] || null;
  } catch {
    return null;
  }
}

/** Extract the __tool_use_purpose (intent) from tool call content */
export function parseToolIntent(content: string | undefined): string | null {
  return parseToolArg(content, '__tool_use_purpose');
}

/**
 * Unwrap the common ACP result envelope structure.
 * Handles: `{items: [{Json: {...}}]}`, `{items: [{Text: "..."}]}`, or plain objects.
 * Returns `{ obj, text }` — one of which will be populated.
 */
export function unwrapResultOutput(result: ToolResult | undefined): {
  obj: Record<string, unknown> | null;
  text: string | null;
} {
  if (!result || result.status !== 'success') return { obj: null, text: null };

  const raw = result.output;
  if (typeof raw === 'string') return { obj: null, text: raw };

  if (raw && typeof raw === 'object') {
    const obj = raw as Record<string, unknown>;
    if ('items' in obj && Array.isArray(obj.items) && obj.items.length > 0) {
      const first = obj.items[0] as Record<string, unknown>;
      if ('Text' in first && typeof first.Text === 'string') {
        return { obj: null, text: first.Text };
      }
      if ('Json' in first && typeof first.Json === 'object') {
        return { obj: first.Json as Record<string, unknown>, text: null };
      }
      return { obj: first, text: null };
    }
    return { obj, text: null };
  }

  return { obj: null, text: null };
}

/** Extract the text content from a ToolResult output */
export function extractResultText(
  result: ToolResult | undefined
): string | null {
  const { obj, text } = unwrapResultOutput(result);
  if (text) return text;
  if (!obj) return null;
  if ('text' in obj && typeof obj.text === 'string') return obj.text;
  if ('content' in obj && typeof obj.content === 'string') return obj.content;
  return null;
}

/** Pull a text string out of a single result item across the known envelopes:
 *  `{Text}` | `{text}` | `{content}` | ACP `{content:[{text}]}` | KAS `{message}`. */
function itemText(item: unknown): string | null {
  if (typeof item === 'string') return item;
  if (!item || typeof item !== 'object') return null;
  const o = item as Record<string, unknown>;
  if (typeof o.Text === 'string') return o.Text;
  if (typeof o.text === 'string') return o.text;
  if (typeof o.message === 'string') return o.message;
  if (Array.isArray(o.content)) {
    const parts = o.content
      .map((c) => (c && typeof c === 'object' ? (c as any).text : null))
      .filter((t): t is string => typeof t === 'string');
    if (parts.length > 0) return parts.join('\n');
  }
  if (typeof o.content === 'string') return o.content;
  const json = 'Json' in o ? o.Json : null;
  if (json && typeof json === 'object') return itemText(json);
  return null;
}

/**
 * Full body text a read/knowledge result carries, concatenating EVERY item
 * (multi-file reads emit one item per file) across the `{items:[...]}`,
 * ACP `{content:[{text}]}`, and KAS `{message}` envelopes. Returns null when
 * nothing textual is present. Distinct from extractResultText, which peeks
 * only at the first item.
 */
export function extractResultBodyItems(
  result: ToolResult | undefined
): string[] {
  if (!result || result.status !== 'success') return [];
  const raw = result.output;
  if (typeof raw === 'string') return [raw];
  if (!raw || typeof raw !== 'object') return [];
  const o = raw as Record<string, unknown>;
  if (Array.isArray(o.items)) {
    return o.items
      .map(itemText)
      .filter((t): t is string => typeof t === 'string');
  }
  const text = itemText(o);
  return text == null ? [] : [text];
}

export function extractResultBodyText(
  result: ToolResult | undefined
): string | null {
  const parts = extractResultBodyItems(result);
  return parts.length > 0 ? parts.join('\n') : null;
}

/** Turn JSON-escaped line breaks into display rows for unknown result shapes. */
export function unescapeJsonNewlines(text: string): string {
  return text
    .replace(/(^|[^\\])((?:\\\\)*)\\r\\n/g, '$1$2\n')
    .replace(/(^|[^\\])((?:\\\\)*)\\n/g, '$1$2\n');
}

/** Split a tool result body into rows, dropping leading/trailing blank (incl.
 *  whitespace-only / CRLF) lines so an all-whitespace payload collapses to `[]`
 *  (→ a `(no output)` placeholder rather than an orphan header over blanks). */
export function splitBodyLines(text: string | null): string[] {
  if (!text || text.trim().length === 0) return [];
  const rows = text.replace(/\r\n/g, '\n').split('\n');
  while (rows.length && rows[0]!.trim() === '') rows.shift();
  while (rows.length && rows[rows.length - 1]!.trim() === '') rows.pop();
  return rows;
}

/** Format text length as a human-readable char count */
export function formatCharCount(text: string): string {
  const chars = text.length;
  if (chars < 1000) return `${chars} chars`;
  return `${(chars / 1000).toFixed(1)}k chars`;
}

/** Get a char count summary string from a ToolResult */
export function getResultSummary(
  result: ToolResult | undefined
): string | null {
  const text = extractResultText(result);
  return text ? formatCharCount(text) : null;
}
