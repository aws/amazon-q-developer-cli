import React from 'react';
import { Box, Text } from '../../renderer.js';
import { useAppStore, MessageRole } from '../../stores/app-store.js';
import type { MessageType } from '../../stores/app-store.js';
import { SessionOutput } from '../multi-agent/SessionOutput.js';
import { getAgentColor } from '../../utils/agentColors.js';
import { useTheme } from '../../hooks/useThemeContext.js';
import { PromptBar } from '../chat/prompt-bar/PromptBar.js';
import { NotificationBar } from '../chat/notification-bar/NotificationBar.js';
import {
  useInputActions,
  useNotificationState,
  useNotificationActions,
} from '../../stores/selectors.js';
import { sessionConversationsStore } from '../../stores/session-conversations.js';

export const SessionViewScreen: React.FC = () => {
  const activeSessionId = useAppStore((s) => s.activeSessionId);
  const sessions = useAppStore((s) => s.sessions);
  const sessionMessages = useAppStore((s) => s.sessionMessages);
  const kiro = useAppStore((s) => s.kiro);
  const showTransientAlert = useAppStore((s) => s.showTransientAlert);
  const { clearInput } = useInputActions();
  const { transientAlert, loadingMessage } = useNotificationState();
  const { dismissTransientAlert } = useNotificationActions();
  const { getColor } = useTheme();

  const session = activeSessionId ? sessions.get(activeSessionId) : undefined;
  const messages = activeSessionId
    ? sessionMessages.get(activeSessionId) || []
    : [];
  const agentColor = session
    ? getAgentColor(session.name, getColor).hex
    : 'cyan';

  const handleSubmit = async (content: string) => {
    if (!activeSessionId || !content.trim()) return;
    try {
      await kiro.sendMessage(activeSessionId, content.trim());

      // Add user message directly to session conversations store
      const userMessage: MessageType = {
        id: crypto.randomUUID(),
        role: MessageRole.User,
        content: content.trim(),
      };

      sessionConversationsStore.setState((state) => {
        const conversations = new Map(state.conversations);
        const messages = conversations.get(activeSessionId) || [];
        conversations.set(activeSessionId, [...messages, userMessage]);
        return { ...state, conversations };
      });
    } catch {
      showTransientAlert({
        message: 'Failed to send message to session',
        status: 'error',
        autoHideMs: 3000,
      });
    }
    clearInput();
  };

  if (!session) {
    return (
      <Box justifyContent="center" alignItems="center" flexGrow={1}>
        <Text color="gray">No session selected. Press q to return.</Text>
      </Box>
    );
  }

  return (
    <Box flexDirection="column" flexGrow={1}>
      <Box paddingX={1}>
        <Text bold color={agentColor}>
          {session.name}
        </Text>
        <Text color="gray"> {session.status} </Text>
        <Text color="gray">q: back</Text>
      </Box>

      <NotificationBar
        message={loadingMessage ?? transientAlert?.message}
        status={loadingMessage ? 'loading' : transientAlert?.status}
        autoHideMs={loadingMessage ? undefined : transientAlert?.autoHideMs}
        onDismiss={loadingMessage ? undefined : dismissTransientAlert}
      />

      <Box flexGrow={1} overflow="hidden">
        <SessionOutput
          sessionId={activeSessionId!}
          session={session}
          messages={messages}
        />
      </Box>

      <PromptBar
        onSubmit={handleSubmit}
        isProcessing={session.status === 'busy'}
        placeholder={`message → ${session.name} ↵  q: back`}
      />
    </Box>
  );
};
