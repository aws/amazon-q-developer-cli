import React, { type ComponentType } from 'react';
import type { AppActions, MessageType } from '../../stores/app-store.js';
import type { ApprovalRequestInfo } from '../../types/agent-events.js';
import { ApprovalRequest } from '../ui/ApprovalRequest.js';
import { ApprovalPrompt as LiteApprovalPrompt } from './lite/ApprovalPrompt.js';

export interface ApprovalSurfaceProps {
  messages: MessageType[];
  approval: ApprovalRequestInfo;
  respondToApproval: AppActions['respondToApproval'];
  getStageInputColor: (stageName: string) => (text: string) => string;
  mainAgentName: string | null;
  onInputSubmit: (value: string) => void;
}

export type ApprovalSurface = ComponentType<ApprovalSurfaceProps>;

export const TuiApprovalSurface: React.FC<ApprovalSurfaceProps> = ({
  onInputSubmit,
}) => <ApprovalRequest onDrillInSubmit={onInputSubmit} />;

export const LiteApprovalSurface: React.FC<ApprovalSurfaceProps> = ({
  onInputSubmit,
  ...props
}) => <LiteApprovalPrompt {...props} onNotesSubmit={onInputSubmit} />;
