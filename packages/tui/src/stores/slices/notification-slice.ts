/**
 * Notification slice - manages transient alerts and blocking errors
 */
import type { StateCreator } from 'zustand';
import type { StatusType } from '../../types/componentTypes';

export interface TransientAlert {
  message: string;
  status: StatusType;
  autoHideMs?: number;
}

export interface NotificationState {
  transientAlert: TransientAlert | null;
  agentError: string | null;
  agentErrorGuidance: string | null;
}

export interface NotificationActions {
  showTransientAlert: (alert: TransientAlert) => void;
  dismissTransientAlert: () => void;
  setAgentError: (error: string | null, guidance?: string | null) => void;
}

export type NotificationSlice = NotificationState & NotificationActions;

export const createNotificationSlice: StateCreator<NotificationSlice> = (
  set
) => ({
  // State
  transientAlert: null,
  agentError: null,
  agentErrorGuidance: null,

  // Actions
  showTransientAlert: (alert) => set({ transientAlert: alert }),
  dismissTransientAlert: () => set({ transientAlert: null }),
  setAgentError: (agentError, guidance) =>
    set({ agentError, agentErrorGuidance: guidance ?? null }),
});
