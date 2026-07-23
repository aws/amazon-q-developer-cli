const HISTORY_ONLY_ASSISTANT_MESSAGES = [
  'Response was interrupted by the user',
  'Tool uses were interrupted, waiting for the next user prompt',
] as const;

export function isHistoryOnlyAssistantMessage(text: string): boolean {
  const candidate = text.trim();
  return HISTORY_ONLY_ASSISTANT_MESSAGES.some(
    (message) => candidate === message
  );
}

export function isHistoryOnlyAssistantMessagePrefix(text: string): boolean {
  const candidate = text.trim();
  return (
    candidate.length > 0 &&
    HISTORY_ONLY_ASSISTANT_MESSAGES.some((message) =>
      message.startsWith(candidate)
    )
  );
}
