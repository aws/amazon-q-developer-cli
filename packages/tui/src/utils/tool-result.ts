import type { ToolResult } from '../stores/app-store.js';

/** Parse a JSON tool call content string and extract a specific field */
export function parseToolArg(content: string | undefined, field: string): string | null {
  if (!content) return null;
  try {
    return JSON.parse(content)[field] || null;
  } catch {
    return null;
  }
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
    let obj = raw as Record<string, unknown>;
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
export function extractResultText(result: ToolResult | undefined): string | null {
  const { obj, text } = unwrapResultOutput(result);
  if (text) return text;
  if (!obj) return null;
  if ('text' in obj && typeof obj.text === 'string') return obj.text;
  if ('content' in obj && typeof obj.content === 'string') return obj.content;
  return null;
}

/** Format text length as a human-readable char count */
export function formatCharCount(text: string): string {
  const chars = text.length;
  if (chars < 1000) return `${chars} chars`;
  return `${(chars / 1000).toFixed(1)}k chars`;
}

/** Get a char count summary string from a ToolResult */
export function getResultSummary(result: ToolResult | undefined): string | null {
  const text = extractResultText(result);
  return text ? formatCharCount(text) : null;
}
