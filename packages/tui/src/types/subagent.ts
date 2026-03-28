export type SubagentStatus = 'working' | 'awaiting_instruction';

export interface SubagentInfo {
  sessionId: string;
  agentName: string;
  initialQuery: string;
  status: SubagentStatus;
}
