import type { ConversationTurn, MessageType } from '../stores/app-store.js';
import { MessageRole } from '../types/message-role.js';

/**
 * Whether a system row's placement is governed by the workflow rules below
 * (explicit `workflowTurnId` ownership; unowned lifecycle/completion rows stay
 * standalone) rather than by the inter-turn notice handling in
 * ConversationView. The single classification shared by both call sites, so a
 * workflow row can never be re-routed through the notice deferral path.
 */
export function isWorkflowPlacedRow(message: {
  kind?: string;
  workflowTurnId?: string;
}): boolean {
  return (
    message.workflowTurnId !== undefined ||
    message.kind === 'workflow-lifecycle' ||
    message.kind === 'workflow-completion'
  );
}

/**
 * Restore system rows after turn grouping filters them out. Workflow terminal
 * rows use their explicit launch-turn id; unowned workflow rows stay
 * standalone rather than attaching to an unrelated prompt.
 */
export function includeInterleavedSystemRows(
  turns: readonly ConversationTurn[],
  messages: readonly MessageType[]
): ConversationTurn[] {
  const messageIndexById = new Map(
    messages.map((message, index) => [message.id, index])
  );
  const nextPromptIndexAfter = (startIndex: number): number | undefined => {
    for (let index = startIndex + 1; index < messages.length; index += 1) {
      const message = messages[index];
      if (message?.role === MessageRole.User && message.steered !== true) {
        return index;
      }
    }
    return undefined;
  };

  return turns.map((turn) => {
    const startIndex = messageIndexById.get(turn.userMessage.id);
    if (startIndex === undefined) return turn;

    const turnBodyIds = new Set(turn.aiMessages.map((message) => message.id));
    const nextPromptIndex = nextPromptIndexAfter(startIndex);
    const endIndex = turn.isActive
      ? messages.length - 1
      : nextPromptIndex === undefined
        ? messages.length - 1
        : nextPromptIndex - 1;
    const hasLaterTurnBody = (index: number): boolean => {
      for (
        let laterIndex = index + 1;
        laterIndex <= endIndex;
        laterIndex += 1
      ) {
        const laterMessage = messages[laterIndex];
        if (laterMessage && turnBodyIds.has(laterMessage.id)) return true;
      }
      return false;
    };

    let addedSystemRow = false;
    const orderedBody: MessageType[] = [];
    for (let index = startIndex + 1; index <= endIndex; index += 1) {
      const message = messages[index];
      if (!message) continue;
      if (message.role === MessageRole.System) {
        const explicitlyOwned = message.workflowTurnId === turn.userMessage.id;
        // Workflow-placed rows (see isWorkflowPlacedRow) fold into a turn only
        // via explicit ownership; every other workflow row stays standalone.
        if (
          !(isWorkflowPlacedRow(message) && !explicitlyOwned) &&
          (message.turnOwned === true || hasLaterTurnBody(index))
        ) {
          addedSystemRow = true;
          orderedBody.push(message);
        } else if (explicitlyOwned) {
          addedSystemRow = true;
          orderedBody.push(message);
        }
      } else if (turnBodyIds.has(message.id)) {
        orderedBody.push(message);
      }
    }

    return addedSystemRow ? { ...turn, aiMessages: orderedBody } : turn;
  });
}
