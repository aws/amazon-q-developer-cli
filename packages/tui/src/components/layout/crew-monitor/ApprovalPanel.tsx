import React, { useMemo, useState } from 'react';
import { Text } from '../../ui/text/Text.js';
import { Panel } from '../../ui/panel/Panel.js';
import { Menu } from '../../ui/menu/Menu.js';
import { useKeypress } from '../../../hooks/useKeypress.js';
import { useTheme } from '../../../hooks/useThemeContext.js';
import { MessageRole, useAppStore } from '../../../stores/app-store.js';
import { useSessionConversation } from '../../../stores/session-conversations.js';
import type {
  ApprovalRequestInfo,
  TrustOption,
} from '../../../types/agent-events.js';

const TRUST_ENTRY_ID = 'allow_always';

export const ApprovalPanel = React.memo(function ApprovalPanel({
  approval,
}: {
  approval: ApprovalRequestInfo;
}) {
  const respondToApproval = useAppStore((state) => state.respondToApproval);
  const conversationMessages = useSessionConversation(approval.sessionId ?? '');
  const { getColor } = useTheme();
  const secondary = getColor('secondary');
  const primary = getColor('primary');

  const [page, setPage] = useState<'default' | 'trust'>('default');
  const [focusedIndex, setFocusedIndex] = useState(0);

  const toolName = useMemo(() => {
    const toolMsg = conversationMessages.find(
      (m) =>
        m.role === MessageRole.ToolUse && m.id === approval.toolCall.toolCallId
    );
    return toolMsg && toolMsg.role === MessageRole.ToolUse
      ? toolMsg.name
      : 'Tool';
  }, [approval.toolCall.toolCallId, conversationMessages]);

  const options = approval.permissionOptions;
  const trustOptions: TrustOption[] = approval.trustOptions ?? [];
  const hasTrustPage = trustOptions.length > 0;

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

  const defaultMenuItems = sortedOptions.map((opt) => ({
    label: optionLabels[opt.optionId] ?? opt.name,
    description: '',
  }));

  const ENTIRE_TOOL_LABEL = 'Entire tool';
  const trustMenuItems = [
    ...trustOptions.map((t) => ({
      label: t.label,
      description: t.display,
    })),
    { label: ENTIRE_TOOL_LABEL, description: '' },
  ];

  const menuItems = page === 'trust' ? trustMenuItems : defaultMenuItems;
  const focusedOnTrust =
    page === 'default' &&
    sortedOptions[focusedIndex]?.optionId === TRUST_ENTRY_ID &&
    hasTrustPage;

  const keyMap: Record<string, string> = { y: 'allow_once', n: 'reject_once' };
  const alwaysOpt = sortedOptions.find(
    (o) => o.optionId === 'allow_all_session' || o.optionId === 'allow_always'
  );
  if (alwaysOpt && !hasTrustPage) keyMap['t'] = alwaysOpt.optionId;

  useKeypress((input, key) => {
    if (key.ctrl || key.meta) return;
    const optionId = keyMap[input.toLowerCase()];
    if (!optionId) return;
    const opt = sortedOptions.find((o) => o.optionId === optionId);
    if (opt) respondToApproval(opt.optionId, approval);
  });

  const handleSelect = (item: { label: string }) => {
    if (page === 'default') {
      const opt = sortedOptions.find(
        (o) => (optionLabels[o.optionId] ?? o.name) === item.label
      );
      if (opt?.optionId === TRUST_ENTRY_ID && hasTrustPage) {
        setPage('trust');
        setFocusedIndex(0);
        return;
      }
      if (opt) respondToApproval(opt.optionId, approval);
    } else {
      if (item.label === ENTIRE_TOOL_LABEL) {
        respondToApproval('allow_always', approval);
        return;
      }
      const selected = trustOptions.find((t) => t.label === item.label);
      if (selected) {
        respondToApproval('allow_always', approval, { trustOption: selected });
      }
    }
  };

  const title =
    page === 'trust'
      ? `${toolName} requires approval · trust options`
      : `${toolName} requires approval`;

  return (
    <Panel
      title={title}
      onClose={() => {
        if (page === 'trust') {
          setPage('default');
          setFocusedIndex(0);
        } else {
          const opt = sortedOptions.find((o) => o.optionId === 'reject_once');
          if (opt) respondToApproval(opt.optionId, approval);
        }
      }}
      hideTitleDivider={true}
      footerLeft={
        focusedOnTrust ? (
          <Text>
            {primary('Enter')} {secondary('to see more options')}
          </Text>
        ) : undefined
      }
    >
      <Menu
        key={page}
        items={menuItems}
        onSelect={handleSelect}
        onHighlight={(item) => {
          const idx = menuItems.findIndex((m) => m.label === item.label);
          if (idx >= 0) setFocusedIndex(idx);
        }}
        showSelectedIndicator={true}
      />
    </Panel>
  );
});
