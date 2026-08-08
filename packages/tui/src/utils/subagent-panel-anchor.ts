import { MessageRole, type MessageType } from '../stores/app-store.js';
import { isSubagentWrapperTool } from './collapsed-tool-view.js';

export interface PanelAnchor {
  /** Index in the given list that should mount the panel, or -1 for none. */
  index: number;
  /**
   * Pipeline group to scope the panel to, or undefined to list every live session.
   *
   * Undefined whenever the in-flight cards do not all share one group: scoping to a
   * single anchor's group would hide the others, while undefined makes
   * selectSubagentToolSessions skip its group filter and return the union. Cards that
   * carry no group at all collapse to the same undefined, which is the same answer.
   */
  groupId: string | undefined;
}

/**
 * Picks the one card that mounts SubagentToolPanel.
 *
 * The panel lists every live sub-agent session, so mounting it per card repeats the
 * whole strip under each card of a parallel fan-out. Anchoring on the last in-flight
 * card shows it once.
 *
 * The filters here MUST mirror the ones the conversation render loop applies to the
 * same list. A message the loop drops can still win the anchor, and then the panel
 * mounts nowhere at all — strictly worse than mounting it everywhere. `isSubagentTool`
 * is the one that bites: nested wrapper cards arrive with it set and are skipped
 * during rendering.
 * @param messages - The tail messages the render loop will iterate.
 * @returns Anchor index and the group to scope the panel to.
 */
export function selectPanelAnchor(
  messages: readonly MessageType[]
): PanelAnchor {
  let index = -1;
  const groups = new Set<string | undefined>();

  for (let i = messages.length - 1; i >= 0; i--) {
    const msg = messages[i]!;
    if (msg.role !== MessageRole.ToolUse) continue;
    if (msg.isFinished) continue;
    if (msg.isSubagentTool) continue;
    if (!isSubagentWrapperTool(msg.name, msg.kind, msg.origin)) continue;
    if (index === -1) index = i;
    groups.add(msg.pipelineGroupId);
  }

  const [only] = groups;
  return { index, groupId: groups.size === 1 ? only : undefined };
}
