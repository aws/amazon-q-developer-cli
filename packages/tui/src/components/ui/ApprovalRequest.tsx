import React from 'react';
import { SnackBar } from '../chat/prompt-bar/SnackBar';
import { useApprovalState, useConversationState } from '../../stores/selectors';
import { ApprovalOptionId, type PermissionOption } from '../../types/agent-events';
import { useKeypress } from '../../hooks/useKeypress';

export const ApprovalRequest: React.FC = () => {
  const { pendingApproval, respondToApproval, cancelApproval } = useApprovalState();
  const { messages } = useConversationState();

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

  // Find the tool message by ID to get the tool name
  const toolMessage = messages.find(
    (msg) => msg.role === 'tool_use' && msg.id === pendingApproval.toolCall.toolCallId
  );
  const toolName = toolMessage && 'name' in toolMessage ? toolMessage.name : 'Tool';

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
      title={`${toolName} requires approval`}
      actions={actions}
      slideIn={true}
    />
  );
};
