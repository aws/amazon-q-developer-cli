import React, { useState } from 'react';
import { Box } from './../../renderer.js';
import { Text } from './text/Text.js';
import { useTheme } from '../../hooks/useThemeContext.js';
import { Panel } from './panel/Panel.js';
import { Menu } from './menu/Menu.js';
import { PromptInput } from '../chat/prompt-bar/PromptInput.js';
import { useKeypress } from '../../hooks/useKeypress.js';
import { useApprovalState, useConversationState } from '../../stores/selectors';
import {
  type PermissionOption,
  type TrustOption,
} from '../../types/agent-events';
import { MessageRole } from '../../stores/app-store.js';

interface ApprovalRequestProps {
  onDrillInSubmit: (value: string) => void;
}

const TRUST_ENTRY_ID = 'allow_always';

export const ApprovalRequest: React.FC<ApprovalRequestProps> = ({
  onDrillInSubmit,
}) => {
  const {
    pendingApproval,
    approvalMode: mode,
    respondToApproval,
    cancelApproval,
    setApprovalMode,
    sessionId: mainSessionId,
    sessions,
  } = useApprovalState();
  const { messages } = useConversationState();
  const { getColor } = useTheme();
  const secondary = getColor('secondary');
  const primary = getColor('primary');

  const approvalSessionId = pendingApproval?.sessionId;
  const subagentName =
    approvalSessionId && approvalSessionId !== mainSessionId
      ? sessions.get(approvalSessionId)?.name
      : undefined;

  const [focusedIndex, setFocusedIndex] = useState(0);
  const [page, setPage] = useState<'default' | 'trust'>('default');

  const options: PermissionOption[] = pendingApproval
    ? pendingApproval.permissionOptions
    : [];
  const trustOptions: TrustOption[] = pendingApproval?.trustOptions ?? [];
  const hasTrustPage = trustOptions.length > 0;

  const optionLabels: Record<string, string> = {
    allow_once: 'Yes, single permission',
    allow_always: 'Trust, always allow in this session',
    allow_all_session: 'Trust, allow all for this session',
    reject_once: 'No (Tab to edit)',
  };

  const defaultMenuItems = options.map((opt) => ({
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
    options[focusedIndex]?.optionId === TRUST_ENTRY_ID &&
    hasTrustPage;

  if (!pendingApproval) return null;

  const toolMsg = messages.find(
    (m) =>
      m.role === MessageRole.ToolUse &&
      m.id === pendingApproval.toolCall.toolCallId
  );
  const toolName =
    toolMsg && toolMsg.role === MessageRole.ToolUse ? toolMsg.name : 'Tool';

  const prefix = subagentName ? `${subagentName} > ` : '';
  const title =
    mode === 'drill-in'
      ? `${prefix}${toolName} requires approval · Modify request`
      : page === 'trust'
        ? `${prefix}${toolName} requires approval · trust options`
        : `${prefix}${toolName} requires approval`;

  const handleClose = () => {
    if (mode === 'drill-in') {
      setApprovalMode('dropdown');
    } else if (page === 'trust') {
      setPage('default');
      setFocusedIndex(0);
    } else {
      cancelApproval();
    }
  };

  const handleTabSwitch = () => {
    if (mode === 'dropdown') {
      setApprovalMode('drill-in');
    } else {
      setApprovalMode('dropdown');
    }
  };

  // Right arrow → drill-in, Left arrow → back (same as Esc)
  useKeypress((_input, key) => {
    if (key.rightArrow && mode === 'dropdown') {
      setApprovalMode('drill-in');
    } else if (key.leftArrow) {
      handleClose();
    }
  });

  const handleSelect = (item: { label: string }) => {
    if (page === 'default') {
      const opt = options.find(
        (o) => (optionLabels[o.optionId] ?? o.name) === item.label
      );
      if (opt?.optionId === TRUST_ENTRY_ID && hasTrustPage) {
        setPage('trust');
        setFocusedIndex(0);
        return;
      }
      if (opt) respondToApproval(opt.optionId);
    } else {
      if (item.label === ENTIRE_TOOL_LABEL) {
        respondToApproval('allow_always');
        return;
      }
      const selected = trustOptions.find((t) => t.label === item.label);
      if (selected) {
        respondToApproval('allow_always', undefined, { trustOption: selected });
      }
    }
  };

  let footerLeft: React.ReactNode | undefined;
  if (mode === 'dropdown' && focusedOnTrust) {
    footerLeft = (
      <Text>
        {primary('Enter')} {secondary('to see more options')}
      </Text>
    );
  } else if (mode === 'dropdown') {
    footerLeft = (
      <Text>
        {primary('Tab')} {secondary('to edit')}
      </Text>
    );
  }

  return (
    <Panel
      title={title}
      onClose={handleClose}
      onTabSwitch={mode === 'dropdown' ? handleTabSwitch : undefined}
      showTabHint={false}
      hideTitleDivider={true}
      footerLeft={footerLeft}
    >
      <Box flexDirection="column">
        {mode === 'dropdown' && (
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
        )}
        {mode === 'drill-in' && (
          <PromptInput
            onSubmit={onDrillInSubmit}
            isProcessing={false}
            placeholder="add your feedback..."
          />
        )}
      </Box>
    </Panel>
  );
};
