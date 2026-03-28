import React, { useMemo } from 'react';
import { Box } from 'ink';
import {
  MessageRole,
  type MessageType as StoreMessageType,
} from '../../stores/app-store';
import { Card } from '../ui/card/Card';
import { Message, MessageType } from '../chat/message/Message';
import { ToolUseMessage } from '../ui/ToolUseMessage';
import { ThinkingMessage } from '../chat/message/ThinkingMessage';
import { StatusBar } from '../chat/status-bar/StatusBar';
import { Text } from '../ui/text/Text';
import { getAgentColor } from '../../utils/agentColors';
import { useTheme } from '../../hooks/useThemeContext';
import { useSessionConversation } from '../../stores/session-conversations.js';
import type { AgentSession, InboxMessage } from '../../types/multi-session';
import type { AgentStreamEvent } from '../../types/agent-events';

export interface SessionOutputProps {
  sessionId: string;
  session?: AgentSession;
  messages: InboxMessage[];
  events?: AgentStreamEvent[];
  width?: number;
  height?: number;
}

interface ConversationTurn {
  userMessage: StoreMessageType;
  aiMessages: StoreMessageType[];
  isActive: boolean;
}

const NudgeMessage = React.memo(function NudgeMessage({
  message,
}: {
  message: InboxMessage;
}) {
  const { getColor: _getColor } = useTheme();

  return (
    <Box paddingX={1} marginY={1}>
      <StatusBar status="info">
        <Text>
          📧 Message from {message.from}: {message.content}
        </Text>
      </StatusBar>
    </Box>
  );
});

const _SystemMessage = React.memo(function _SystemMessage({
  message,
}: {
  message: StoreMessageType & { role: MessageRole.System };
}) {
  return (
    <Box paddingX={1} marginY={1}>
      <StatusBar status={message.success ? 'success' : 'error'}>
        <Text>{message.content}</Text>
      </StatusBar>
    </Box>
  );
});

const ConversationTurnCard = React.memo(
  function ConversationTurnCard({
    turn,
    agentBarColor,
    isProcessing,
  }: {
    turn: ConversationTurn;
    agentBarColor?: string;
    isProcessing: boolean;
  }) {
    const lastAiMsg = turn.aiMessages[turn.aiMessages.length - 1];
    const hasActiveContent = lastAiMsg
      ? (lastAiMsg.role === MessageRole.ToolUse && !lastAiMsg.isFinished) ||
        (lastAiMsg.role === MessageRole.Model &&
          turn.isActive &&
          isProcessing &&
          !!lastAiMsg.content)
      : false;

    const showThinking = turn.isActive && isProcessing && !hasActiveContent;

    // Find last model message (compatible with older JS targets)
    let lastModelIndex = -1;
    for (let i = turn.aiMessages.length - 1; i >= 0; i--) {
      const message = turn.aiMessages[i];
      if (message && message.role === MessageRole.Model) {
        lastModelIndex = i;
        break;
      }
    }

    return (
      <Card active={turn.isActive}>
        {turn.userMessage.content && (
          <Message
            key={turn.userMessage.id}
            content={turn.userMessage.content}
            type={MessageType.DEVELOPER}
            barColor={agentBarColor}
          />
        )}

        {turn.aiMessages.map((message, index) => {
          if (message.role === MessageRole.ToolUse) {
            return (
              <ToolUseMessage
                key={message.id}
                id={message.id}
                name={message.name}
                kind={message.kind}
                content={message.content}
                isFinished={message.isFinished}
                status={message.status}
                result={message.result}
                locations={message.locations}
                barColor={agentBarColor}
                isStatic={!turn.isActive || message.isFinished}
              />
            );
          }
          if (!message.content || message.content === '') return null;

          const isLastModel = index === lastModelIndex;
          const shouldStream = turn.isActive && isProcessing && isLastModel;

          if (shouldStream) {
            return (
              <Message
                key={message.id}
                content={message.content}
                type={MessageType.AGENT}
                barColor={agentBarColor}
              />
            );
          }

          return (
            <Message
              key={message.id}
              content={message.content}
              type={MessageType.AGENT}
              barColor={agentBarColor}
            />
          );
        })}

        {showThinking && (
          <ThinkingMessage
            key={`${turn.userMessage.id}-thinking`}
            barColor={agentBarColor}
          />
        )}
      </Card>
    );
  },
  (prev, next) => {
    // Custom comparator: turn objects are new refs from useMemo, so compare by identity of contents
    if (prev.agentBarColor !== next.agentBarColor) return false;
    if (prev.isProcessing !== next.isProcessing) return false;
    if (prev.turn.isActive !== next.turn.isActive) return false;
    if (prev.turn.userMessage !== next.turn.userMessage) return false;
    if (prev.turn.aiMessages.length !== next.turn.aiMessages.length)
      return false;
    // Compare last message ref — if it changed, new content arrived
    const prevLast = prev.turn.aiMessages[prev.turn.aiMessages.length - 1];
    const nextLast = next.turn.aiMessages[next.turn.aiMessages.length - 1];
    return prevLast === nextLast;
  }
);

const _StaticTurnCard = React.memo(function _StaticTurnCard({
  turn,
  agentBarColor,
}: {
  turn: ConversationTurn;
  agentBarColor?: string;
}) {
  return (
    <Box marginBottom={1}>
      <Card active={false}>
        {turn.userMessage.content && (
          <Message
            key={turn.userMessage.id}
            content={turn.userMessage.content}
            type={MessageType.DEVELOPER}
            barColor={agentBarColor}
          />
        )}

        {turn.aiMessages.map((message) => {
          if (message.role === MessageRole.ToolUse) {
            return (
              <ToolUseMessage
                key={message.id}
                id={message.id}
                name={message.name}
                kind={message.kind}
                content={message.content}
                isFinished={true}
                isStatic={true}
                status={message.status}
                result={message.result}
                locations={message.locations}
                barColor={agentBarColor}
              />
            );
          }
          if (!message.content || message.content === '') return null;
          return (
            <Message
              key={message.id}
              content={message.content}
              type={MessageType.AGENT}
              barColor={agentBarColor}
            />
          );
        })}
      </Card>
    </Box>
  );
});

const SessionHeader = React.memo(function SessionHeader({
  session,
  agentBarColor,
}: {
  session: AgentSession;
  agentBarColor?: string;
}) {
  const statusIcon =
    session.status === 'busy'
      ? '●'
      : session.status === 'terminated'
        ? '✓'
        : session.status === 'failed'
          ? '✗'
          : '○';

  const stagePrefix = session.stageInfo ? `[${session.stageInfo.name}] ` : '';

  return (
    <Box paddingX={1} marginBottom={1}>
      <StatusBar barColor={agentBarColor}>
        <Text>
          {statusIcon} {stagePrefix}
          {session.name}
        </Text>
        {session.stageInfo && session.stageInfo.role && (
          <Text> ({session.stageInfo.role})</Text>
        )}
        {!session.stageInfo && session.role && <Text> ({session.role})</Text>}
      </StatusBar>
    </Box>
  );
});

export const SessionOutput = React.memo(function SessionOutput({
  sessionId,
  session,
  messages,
  events: _events = [],
  width,
  height = 20,
}: SessionOutputProps) {
  const { getColor } = useTheme();
  const dim = getColor('secondary');
  const agentBarColor = useMemo(
    () =>
      session ? getAgentColor(session.name, getColor).hex : getColor('primary'),
    [session?.name, getColor]
  );
  const conversationMessages = useSessionConversation(sessionId);

  // Build conversation turns — memoized. Must be before early return (Rules of Hooks).
  const turns = useMemo(() => {
    const result: ConversationTurn[] = [];
    if (!session) return result;
    let currentTurn: ConversationTurn | null = null;
    for (const msg of conversationMessages) {
      if (msg.role === MessageRole.User) {
        if (currentTurn) result.push(currentTurn);
        currentTurn = { userMessage: msg, aiMessages: [], isActive: false };
      } else {
        if (!currentTurn) {
          currentTurn = {
            userMessage: {
              id: `implicit-${sessionId}`,
              role: MessageRole.User,
              content: '',
            },
            aiMessages: [],
            isActive: false,
          };
        }
        currentTurn.aiMessages.push(msg);
      }
    }
    if (currentTurn) {
      currentTurn.isActive = session.status === 'busy';
      result.push(currentTurn);
    }
    return result;
  }, [conversationMessages, session?.status, sessionId]);

  if (!session) {
    return (
      <Box
        flexDirection="column"
        width={width}
        height={height}
        justifyContent="center"
        alignItems="center"
      >
        <Text>Select a session to view output</Text>
      </Box>
    );
  }

  const isProcessing = session.status === 'busy';

  return (
    <Box flexDirection="column" width={width}>
      <SessionHeader session={session} agentBarColor={agentBarColor} />

      {turns.length > 0 ? (
        <>
          {turns.map((turn, i) => (
            <ConversationTurnCard
              key={turn.userMessage.id}
              turn={turn}
              agentBarColor={agentBarColor}
              isProcessing={isProcessing && i === turns.length - 1}
            />
          ))}
          {messages.map((m) => (
            <NudgeMessage key={m.id} message={m} />
          ))}
          {session.status === 'terminated' && session.summary && (
            <Box paddingX={1} marginTop={1}>
              <StatusBar status="success">
                <Text>Summary: {session.summary}</Text>
              </StatusBar>
            </Box>
          )}
        </>
      ) : (
        <Box paddingX={1}>
          <Text>
            {dim(
              isProcessing ? 'Waiting for agent output...' : 'No activity yet'
            )}
          </Text>
        </Box>
      )}
    </Box>
  );
});
