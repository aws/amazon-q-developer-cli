import React, { useState } from 'react';
import { Box } from './../../renderer.js';
import { Text } from './text/Text.js';
import { useTheme } from '../../hooks/useThemeContext.js';
import { Panel } from './panel/Panel.js';
import { Menu } from './menu/Menu.js';
import { PromptInput } from '../chat/prompt-bar/PromptInput.js';
import { useKeypress } from '../../hooks/useKeypress.js';
import { useApprovalState, useConversationState } from '../../stores/selectors';
import { type PermissionOption } from '../../types/agent-events';
import { MessageRole } from '../../stores/app-store.js';

interface ApprovalRequestProps {
  onDrillInSubmit: (value: string) => void;
}

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

  // Derive subagent name if this approval is from a subagent session
  const approvalSessionId = pendingApproval?.sessionId;
  const subagentName =
    approvalSessionId && approvalSessionId !== mainSessionId
      ? sessions.get(approvalSessionId)?.name
      : undefined;

  const [focusedIndex, setFocusedIndex] = useState(0);

  // Build options from what the backend sends, preserving order
  const options: PermissionOption[] = pendingApproval
    ? pendingApproval.permissionOptions
    : [];

  const optionLabels: Record<string, string> = {
    allow_once: 'Yes, single permission',
    allow_always: 'Trust, always allow in this session',
    allow_all_session: 'Trust, allow all for this session',
    reject_once: 'No',
    reject_always: 'Never',
  };

  const shortLabels: Record<string, string> = {
    allow_once: 'Yes',
    allow_always: 'Trust',
    allow_all_session: 'Trust all',
    reject_once: 'No',
    reject_always: 'Never',
  };

  // Keyboard shortcuts: y=allow_once, n=reject_once, t=whichever "always" option is present
  const keyMap: Record<string, string> = { y: 'allow_once', n: 'reject_once' };
  const alwaysOpt = options.find(
    (o) => o.optionId === 'allow_all_session' || o.optionId === 'allow_always'
  );
  if (alwaysOpt) keyMap['t'] = alwaysOpt.optionId;

  const canDrillIn =
    options[focusedIndex]?.optionId === 'allow_once' ||
    options[focusedIndex]?.optionId === 'reject_once';

  useKeypress((input) => {
    if (mode !== 'dropdown') return;
    const optionId = keyMap[input.toLowerCase()];
    if (!optionId) return;
    const opt = options.find((o) => o.optionId === optionId);
    if (opt) respondToApproval(opt.optionId);
  });

  const menuItems = options.map((opt) => ({
    label: optionLabels[opt.optionId] ?? opt.name,
    description: '',
  }));

  if (!pendingApproval) return null;

  const toolMsg = messages.find(
    (m) =>
      m.role === MessageRole.ToolUse &&
      m.id === pendingApproval.toolCall.toolCallId
  );
  const toolName =
    toolMsg && toolMsg.role === MessageRole.ToolUse ? toolMsg.name : 'Tool';

  const prefix = subagentName ? `${subagentName} > ` : '';
  const focusedOptId = options[focusedIndex]?.optionId ?? '';
  const title =
    mode === 'drill-in'
      ? `${prefix}${toolName} requires approval · ${shortLabels[focusedOptId] ?? ''}`
      : `${prefix}${toolName} requires approval`;

  const handleClose = () => {
    if (mode === 'drill-in') {
      setApprovalMode('dropdown');
    } else {
      cancelApproval();
    }
  };

  const handleTabSwitch = () => {
    if (mode === 'dropdown' && canDrillIn) {
      setApprovalMode('drill-in');
    } else {
      setApprovalMode('dropdown');
    }
  };

  return (
    <Panel
      title={title}
      onClose={handleClose}
      onTabSwitch={
        mode === 'dropdown' && canDrillIn ? handleTabSwitch : undefined
      }
      showTabHint={false}
      hideTitleDivider={true}
      footerLeft={
        mode === 'dropdown' && canDrillIn ? (
          <Text>
            {primary('Tab')} {secondary('to edit')}
          </Text>
        ) : undefined
      }
    >
      <Box flexDirection="column">
        {mode === 'dropdown' && (
          <Menu
            items={menuItems}
            onSelect={(item) => {
              const opt = options.find(
                (o) => (optionLabels[o.optionId] ?? o.name) === item.label
              );
              if (opt) respondToApproval(opt.optionId);
            }}
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
