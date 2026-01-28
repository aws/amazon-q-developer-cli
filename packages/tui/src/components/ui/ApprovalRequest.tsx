import React from 'react';
import { SnackBar } from '../chat/prompt-bar/SnackBar';
import { useAppStore } from '../../stores/app-store';
import { ApprovalOptionId, type PermissionOption } from '../../types/agent-events';
import { useKeypress } from '../../hooks/useKeypress';

export const ApprovalRequest: React.FC = () => {
  const pendingApproval = useAppStore((state) => state.pendingApproval);
  const respondToApproval = useAppStore((state) => state.respondToApproval);
  const cancelApproval = useAppStore((state) => state.cancelApproval);

  useKeypress((input, key) => {
    if (!pendingApproval) return;

    // Handle escape to cancel
    if (key.escape) {
      cancelApproval();
      return;
    }

    const inputLower = input.toLowerCase();
    let selectedOption: PermissionOption | undefined;

    // Map keys to option kinds
    if (inputLower === 't') {
      selectedOption = pendingApproval.permissionOptions.find(
        (opt) => opt.kind === ApprovalOptionId.AllowAlways
      );
    } else if (inputLower === 'y') {
      selectedOption = pendingApproval.permissionOptions.find(
        (opt) => opt.kind === ApprovalOptionId.AllowOnce
      );
    } else if (inputLower === 'n') {
      selectedOption = pendingApproval.permissionOptions.find(
        (opt) => opt.kind === ApprovalOptionId.RejectOnce
      );
    }

    if (selectedOption) {
      respondToApproval(selectedOption.optionId);
    }
  });

  if (!pendingApproval) return null;

  // Build actions array with only the options we want to show
  const actions = [];
  
  if (pendingApproval.permissionOptions.find((opt) => opt.kind === ApprovalOptionId.AllowOnce)) {
    actions.push({ key: 'y', label: 'Yes' });
  }
  
  if (pendingApproval.permissionOptions.find((opt) => opt.kind === ApprovalOptionId.RejectOnce)) {
    actions.push({ key: 'n', label: 'No' });
  }
  
  if (pendingApproval.permissionOptions.find((opt) => opt.kind === ApprovalOptionId.AllowAlways)) {
    actions.push({ key: 't', label: 'Trust' });
  }

  return (
    <SnackBar
      title="Tool requires approval"
      actions={actions}
      slideIn={true}
    />
  );
};
