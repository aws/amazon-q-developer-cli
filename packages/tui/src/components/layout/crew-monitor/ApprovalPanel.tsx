import React, { useMemo } from 'react';
import { Panel } from '../../ui/panel/Panel.js';
import { Menu } from '../../ui/menu/Menu.js';
import { useKeypress } from '../../../hooks/useKeypress.js';
import { MessageRole, useAppStore } from '../../../stores/app-store.js';
import { useSessionConversation } from '../../../stores/session-conversations.js';
import type {
  ApprovalRequestInfo,
  PermissionOption,
} from '../../../types/agent-events.js';

export const ApprovalPanel = React.memo(function ApprovalPanel({
  approval,
}: {
  approval: ApprovalRequestInfo;
}) {
  const respondToApproval = useAppStore((state) => state.respondToApproval);
  const conversationMessages = useSessionConversation(approval.sessionId ?? '');

  const toolName = useMemo(() => {
    const toolMsg = conversationMessages.find(
      (m) =>
        m.role === MessageRole.ToolUse && m.id === approval.toolCall.toolCallId
    );
    return toolMsg && toolMsg.role === MessageRole.ToolUse
      ? toolMsg.name
      : 'Tool';
  }, [approval.toolCall.toolCallId, conversationMessages]);

  const options: PermissionOption[] = approval.permissionOptions;

  // Sort: allow options first, reject options last
  const sortedOptions = useMemo(() => {
    const order: Record<string, number> = {
      allow_once: 0,
      allow_always: 1,
      allow_all_session: 2,
      reject_always: 3,
      reject_once: 4,
    };
    return [...options].sort(
      (a, b) => (order[a.optionId] ?? 3) - (order[b.optionId] ?? 3)
    );
  }, [options]);

  const optionLabels: Record<string, string> = {
    allow_once: 'Yes, single permission',
    allow_always: 'Trust, always allow in this session',
    allow_all_session: 'Trust, allow all for this session',
    reject_once: 'No',
    reject_always: 'Never',
  };

  const keyMap: Record<string, string> = { y: 'allow_once', n: 'reject_once' };
  const alwaysOpt = sortedOptions.find(
    (o) => o.optionId === 'allow_all_session' || o.optionId === 'allow_always'
  );
  if (alwaysOpt) keyMap['t'] = alwaysOpt.optionId;

  useKeypress((input, key) => {
    if (key.ctrl || key.meta) return;
    const optionId = keyMap[input.toLowerCase()];
    if (!optionId) return;
    const opt = sortedOptions.find((o) => o.optionId === optionId);
    if (opt) respondToApproval(opt.optionId, approval);
  });

  const menuItems = sortedOptions.map((opt) => ({
    label: optionLabels[opt.optionId] ?? opt.name,
    description: '',
  }));

  const title = `${toolName} requires approval`;

  return (
    <Panel
      title={title}
      onClose={() => {
        const opt = sortedOptions.find((o) => o.optionId === 'reject_once');
        if (opt) respondToApproval(opt.optionId, approval);
      }}
      hideTitleDivider={true}
    >
      <Menu
        items={menuItems}
        onSelect={(item) => {
          const opt = sortedOptions.find(
            (o) => (optionLabels[o.optionId] ?? o.name) === item.label
          );
          if (opt) respondToApproval(opt.optionId, approval);
        }}
        showSelectedIndicator={true}
      />
    </Panel>
  );
});
