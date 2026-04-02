import React from 'react';
import { Box } from './../../renderer.js';
import { Text } from './text/Text.js';
import { useTheme } from '../../hooks/useThemeContext.js';
import { Menu } from './menu/Menu.js';
import { useKeypress } from '../../hooks/useKeypress.js';
import { useApprovalState } from '../../stores/selectors.js';
import { useAppStore } from '../../stores/app-store.js';
import type { PermissionOption } from '../../types/agent-events.js';

/**
 * Consolidated crew approval UI shown at the main agent page when a subagent
 * requests tool permission.
 */
export const CrewApprovalRequest: React.FC<{
  onConfigure: () => void;
}> = ({ onConfigure }) => {
  const approvalQueue = useAppStore((state) => state.approvalQueue);
  const setAutoApproveCrewTools = useAppStore(
    (state) => state.setAutoApproveCrewTools
  );
  const cancelMessage = useAppStore((state) => state.cancelMessage);
  const terminateAllCrewSessions = useAppStore(
    (state) => state.terminateAllCrewSessions
  );
  const { respondToApproval, cancelApproval } = useApprovalState();
  const { getColor } = useTheme();

  const count = approvalQueue.length;

  const items = [
    { label: '(a) Approve all pending', value: 'approve' },
    {
      label: '(f) Approve all pending and auto-approve all future requests',
      value: 'auto',
    },
    { label: '(c) Configure individually (agent monitor)', value: 'configure' },
    { label: '(x) Exit (cancel subagents)', value: 'exit' },
  ];

  const approveAll = () => {
    for (const approval of [...approvalQueue]) {
      const opt = approval.permissionOptions.find(
        (o: PermissionOption) => o.optionId === 'allow_once'
      );
      if (opt) respondToApproval(opt.optionId, approval);
    }
  };

  const handleExit = async () => {
    cancelApproval();
    await terminateAllCrewSessions();
    await cancelMessage();
  };

  const handleSelect = (item: { label: string }) => {
    const match = items.find((i) => i.label === item.label);
    if (!match) return;
    if (match.value === 'approve') {
      approveAll();
    } else if (match.value === 'auto') {
      setAutoApproveCrewTools(true);
      approveAll();
    } else if (match.value === 'configure') {
      onConfigure();
    } else if (match.value === 'exit') {
      handleExit();
    }
  };

  useKeypress((input, key) => {
    if (key.ctrl || key.meta || key.shift) return;
    const k = input.toLowerCase();
    if (k === 'a') handleSelect({ label: items[0]!.label });
    else if (k === 'f') handleSelect({ label: items[1]!.label });
    else if (k === 'c') handleSelect({ label: items[2]!.label });
    else if (k === 'x') handleSelect({ label: items[3]!.label });
  });

  return (
    <Box flexDirection="column" paddingX={1}>
      <Box>
        <Text>
          {getColor('warning').bold(
            `⚠ ${count} tool approval${count !== 1 ? 's' : ''} pending from subagents`
          )}
        </Text>
      </Box>
      <Menu
        items={items.map((i) => ({ label: i.label, description: '' }))}
        onSelect={handleSelect}
        showSelectedIndicator={true}
      />
    </Box>
  );
};
