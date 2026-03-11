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
  ApprovalOptionId,
  type PermissionOption,
} from '../../types/agent-events';
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
  } = useApprovalState();
  const { messages } = useConversationState();
  const { getColor } = useTheme();
  const secondary = getColor('secondary');
  const primary = getColor('primary');

  const [focusedKind, setFocusedKind] = useState<ApprovalOptionId>(
    ApprovalOptionId.AllowOnce
  );

  const canDrillIn =
    focusedKind === ApprovalOptionId.AllowOnce ||
    focusedKind === ApprovalOptionId.RejectOnce;

  useKeypress((input) => {
    if (mode !== 'dropdown') return;
    const key = input.toLowerCase();
    const kindMap: Record<string, ApprovalOptionId> = {
      y: ApprovalOptionId.AllowOnce,
      n: ApprovalOptionId.RejectOnce,
      t: ApprovalOptionId.AllowAlways,
    };
    const kind = kindMap[key];
    if (!kind) return;
    const opt = options.find((o) => o.kind === kind);
    if (opt) respondToApproval(opt.optionId);
  });

  const optionOrder: ApprovalOptionId[] = [
    ApprovalOptionId.AllowOnce,
    ApprovalOptionId.AllowAlways,
    ApprovalOptionId.RejectOnce,
  ];
  const optionLabels: Record<ApprovalOptionId, string> = {
    [ApprovalOptionId.AllowOnce]: 'Yes, single permission',
    [ApprovalOptionId.RejectOnce]: 'No',
    [ApprovalOptionId.AllowAlways]: 'Trust, always allow in this session',
    [ApprovalOptionId.RejectAlways]: 'Never',
  };

  const options: PermissionOption[] = pendingApproval
    ? optionOrder
        .map((kind) =>
          pendingApproval.permissionOptions.find((o) => o.kind === kind)
        )
        .filter((o): o is PermissionOption => o !== undefined)
    : [];

  const menuItems = options.map((opt) => ({
    label: optionLabels[opt.kind] ?? opt.name,
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

  const shortLabels: Record<ApprovalOptionId, string> = {
    [ApprovalOptionId.AllowOnce]: 'Yes',
    [ApprovalOptionId.RejectOnce]: 'No',
    [ApprovalOptionId.AllowAlways]: 'Trust',
    [ApprovalOptionId.RejectAlways]: 'Never',
  };

  const title =
    mode === 'drill-in'
      ? `${toolName} requires approval · ${shortLabels[focusedKind]}`
      : `${toolName} requires approval`;

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
                (o) => (optionLabels[o.kind] ?? o.name) === item.label
              );
              if (opt) respondToApproval(opt.optionId);
            }}
            onHighlight={(item) => {
              const opt = options.find(
                (o) => (optionLabels[o.kind] ?? o.name) === item.label
              );
              if (opt) setFocusedKind(opt.kind);
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
