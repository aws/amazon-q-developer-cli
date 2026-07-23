/**
 * Utility for formatting tool call parameters for display in the TUI.
 *
 * Extracts the most relevant parameters from tool call content JSON
 * and formats them as individual `key=value` strings, excluding internal
 * fields and fields already shown elsewhere by the tool component.
 */

/** Fields always excluded — internal or large content fields (mainline set). */
const BASE_EXCLUDED = new Set([
  '__tool_use_purpose',
  'content',
  'text',
  'oldStr',
  'newStr',
]);

// In-cohort also hide the Rust wire's snake_case write payloads so they render
// as the diff body, never as arg chips. Off-cohort the excluded set stays
// mainline-exact (a stray `old_str` arg still shows, matching pre-port).
const PORT_EXCLUDED = ['old_str', 'new_str', 'file_text'];
function excludedFields(): Set<string> {
  if (process.env.KIRO_LITE_ROLLOUT_ENABLED !== '1') return BASE_EXCLUDED;
  return new Set([...BASE_EXCLUDED, ...PORT_EXCLUDED]);
}

/**
 * Parse tool call content JSON and return an array of formatted param strings.
 * Returns null if no displayable params exist.
 *
 * @param content - Raw JSON string of tool call args
 * @param exclude - Additional field names to exclude (fields already shown as target/title by the component)
 *
 * Example output: `['caseSensitive=true', 'includePattern=**\/*.ts']`
 */
export function formatToolParams(
  content: string | undefined,
  exclude?: ReadonlyArray<string>
): string[] | null {
  if (!content) return null;
  try {
    const args = JSON.parse(content);
    if (typeof args !== 'object' || args === null || Array.isArray(args)) {
      return null;
    }

    const excluded = excludedFields();
    const parts: string[] = [];
    for (const [key, value] of Object.entries(args)) {
      if (excluded.has(key)) continue;
      if (exclude && exclude.includes(key)) continue;
      if (value === null || value === undefined) continue;

      let display: string;
      if (typeof value === 'string') {
        display = value;
      } else if (typeof value === 'number' || typeof value === 'boolean') {
        display = String(value);
      } else if (Array.isArray(value)) {
        // Show arrays of primitives inline; skip arrays of objects
        const primitives = value.filter(
          (v) =>
            typeof v === 'string' ||
            typeof v === 'number' ||
            typeof v === 'boolean'
        );
        if (primitives.length === 0) continue;
        display = primitives.join(', ');
      } else {
        continue; // skip nested objects
      }

      parts.push(`${key}=${display}`);
    }

    return parts.length > 0 ? parts : null;
  } catch {
    return null;
  }
}
