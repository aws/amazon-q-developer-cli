/**
 * Tiered comfort messages shown when the agent is processing and no content
 * has arrived yet. When the "show thinking" setting is OFF, we display
 * progressively reassuring messages. When ON, the user already sees thinking
 * traces so we only show the base "Thinking..." label.
 */
export function getComfortMessage(
  elapsedMs: number,
  thinkingEnabled: boolean
): string {
  if (thinkingEnabled) return 'Thinking...';

  if (elapsedMs >= 180_000) {
    return 'Still thinking, complex requests can take me longer. Show thinking in settings to see progress.';
  }
  if (elapsedMs >= 120_000) {
    return 'Still thinking, this is a tricky one...';
  }
  if (elapsedMs >= 60_000) {
    return 'Still thinking...';
  }
  return 'Thinking...';
}
