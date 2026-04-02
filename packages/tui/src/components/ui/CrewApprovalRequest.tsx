import React, { useState } from 'react';
import { Box } from './../../renderer.js';
import { Text } from './text/Text.js';
import { useTheme } from '../../hooks/useThemeContext.js';
import { Menu } from './menu/Menu.js';
import { useKeypress } from '../../hooks/useKeypress.js';
import { useApprovalState } from '../../stores/selectors.js';
import { useAppStore } from '../../stores/app-store.js';
import type { PermissionOption } from '../../types/agent-events.js';

const TRUST_ENTRY_LABEL = '(t) Trust';

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

  const [page, setPage] = useState<'default' | 'trust'>('default');
  const [focusedIndex, setFocusedIndex] = useState(0);

  const count = approvalQueue.length;

  const defaultItems = [
    { label: '(a) Approve all pending', value: 'approve' },
    {
      label: TRUST_ENTRY_LABEL,
      value: 'trust',
      description: '',
    },
    { label: '(c) Configure individually (agent monitor)', value: 'configure' },
    { label: '(x) Exit (cancel subagents)', value: 'exit' },
  ];

  const trustItems = [
    {
      label: '(f) Approve all pending and auto-approve all future requests',
      value: 'auto',
    },
  ];

  const visibleItems = page === 'trust' ? trustItems : defaultItems;
  const focusedOnTrust =
    page === 'default' && defaultItems[focusedIndex]?.value === 'trust';

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
    const allItems = [...defaultItems, ...trustItems];
    const match = allItems.find((i) => i.label === item.label);
    if (!match) return;
    if (match.value === 'trust') {
      setPage('trust');
      setFocusedIndex(0);
    } else if (match.value === 'approve') {
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
    if (k === 'a') handleSelect({ label: defaultItems[0]!.label });
    else if (k === 't') {
      if (page === 'default') {
        setPage('trust');
        setFocusedIndex(0);
      } else {
        setPage('default');
        setFocusedIndex(0);
      }
    } else if (k === 'f') handleSelect({ label: trustItems[0]!.label });
    else if (k === 'c') handleSelect({ label: defaultItems[2]!.label });
    else if (k === 'x') handleSelect({ label: defaultItems[3]!.label });
  });

  const secondary = getColor('secondary');
  const primary = getColor('primary');

  return (
    <Box flexDirection="column" paddingX={1}>
      <Box>
        <Text>
          {getColor('warning').bold(
            `⚠ ${count} tool approval${count !== 1 ? 's' : ''} pending from subagents`
          )}
        </Text>
      </Box>
      {page === 'trust' && (
        <Box>
          <Text>{secondary('Trust options:')}</Text>
        </Box>
      )}
      <Menu
        key={page}
        items={visibleItems.map((i) => ({
          label: i.label,
          description: (i as any).description ?? '',
        }))}
        onSelect={handleSelect}
        onHighlight={(item) => {
          const idx = visibleItems.findIndex((i) => i.label === item.label);
          if (idx >= 0) setFocusedIndex(idx);
        }}
        showSelectedIndicator={true}
      />
      {focusedOnTrust ? (
        <Box paddingX={1}>
          <Text>
            {primary('Enter')} {secondary('to see more options')}
          </Text>
        </Box>
      ) : null}
    </Box>
  );
};
