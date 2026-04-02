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

  // Page 1: all options as-is, but Trust gets a hint when _meta.trustOptions exists
  const defaultMenuItems = options.map((opt) => ({
    label: optionLabels[opt.optionId] ?? opt.name,
    description: '',
  }));

  const trustMenuItems = trustOptions.map((t) => ({
    label: t.label,
    description: t.display,
  }));

  const menuItems = page === 'trust' ? trustMenuItems : defaultMenuItems;

  const focusedOnTrust =
    page === 'default' &&
    options[focusedIndex]?.optionId === TRUST_ENTRY_ID &&
    hasTrustPage;

  const canDrillIn =
    page === 'default' &&
    (options[focusedIndex]?.optionId === 'allow_once' ||
      options[focusedIndex]?.optionId === 'reject_once');

  // Keyboard shortcuts: y=allow_once, n=reject_once, t=allow_always (direct, no page 2)
  const keyMap: Record<string, string> = { y: 'allow_once', n: 'reject_once' };
  const alwaysOpt = options.find(
    (o) => o.optionId === 'allow_all_session' || o.optionId === 'allow_always'
  );
  if (alwaysOpt && !hasTrustPage) keyMap['t'] = alwaysOpt.optionId;

  useKeypress((input) => {
    if (mode !== 'dropdown') return;
    const optionId = keyMap[input.toLowerCase()];
    if (!optionId) return;
    const opt = options.find((o) => o.optionId === optionId);
    if (opt) respondToApproval(opt.optionId);
  });

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
    if (mode === 'dropdown' && canDrillIn) {
      setApprovalMode('drill-in');
    } else {
      setApprovalMode('dropdown');
    }
  };

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
      const selected = trustOptions.find((t) => t.label === item.label);
      if (selected) {
        respondToApproval('allow_always', undefined, { trustOption: selected });
      }
    }
  };

  let footerLeft: React.ReactNode | undefined;
  if (mode === 'dropdown') {
    if (focusedOnTrust) {
      footerLeft = (
        <Text>
          {primary('Enter')} {secondary('to see more options')}
        </Text>
      );
    } else if (canDrillIn) {
      footerLeft = (
        <Text>
          {primary('Tab')} {secondary('to edit')}
        </Text>
      );
    }
  }

  return (
    <Panel
      title={title}
      onClose={handleClose}
      onTabSwitch={
        mode === 'dropdown' && canDrillIn ? handleTabSwitch : undefined
      }
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
