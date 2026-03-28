import type { SessionStatus } from '../../../types/multi-session.js';

export type StageState = 'Pending' | 'Executing' | 'Completed' | 'Failed';

export interface Stage {
  name: string;
  agentName: string;
  state: StageState;
  description: string;
  events: number;
  role: string;
  sessionId: string;
  group?: string;
  isPending?: boolean;
  dependsOn?: string[];
  activeStatus?: string;
}

export const mapSessionStatusToStageState = (
  status: SessionStatus
): StageState => {
  switch (status) {
    case 'busy':
      return 'Executing';
    case 'terminated':
      return 'Completed';
    case 'failed':
      return 'Failed';
    case 'pending':
      return 'Pending';
    default:
      return 'Pending';
  }
};

export const getStaticIcon = (
  state: StageState
): { icon: string; color: string } => {
  switch (state) {
    case 'Completed':
      return { icon: '✓', color: 'gray' };
    case 'Failed':
      return { icon: '✗', color: 'red' };
    case 'Executing':
      return { icon: '◐', color: 'magenta' };
    default:
      return { icon: '○', color: 'gray' };
  }
};

export const truncate = (s: string, max: number) =>
  s.length > max ? s.slice(0, max - 1) + '…' : s;

export const SPINNERS = ['◐', '◓', '◑', '◒'];
export const ATTENTION_TEXT = 'tool approval needed';
export const ATTENTION_COL_W = ATTENTION_TEXT.length + 3;
export const EMPTY_INBOX: never[] = [];
