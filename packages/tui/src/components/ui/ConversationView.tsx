import React from 'react';
import { Box, Static } from 'ink';
import {
  MessageRole,
  type MessageType as StoreMessageType,
} from '../../stores/app-store';
import { useConversationState, useContextState } from '../../stores/selectors';
import { Card } from '../ui/card/Card';
import { Message, MessageType } from '../chat/message/Message';
import { StreamingMessage } from '../chat/message/StreamingMessage';
import { ToolUseMessage } from './ToolUseMessage';
import { ThinkingMessage } from '../chat/message/ThinkingMessage';
import { StatusBar } from '../chat/status-bar/StatusBar';
import { Text } from '../ui/text/Text';
import { WelcomeScreen } from '../welcome-screen/index.js';
import { getAgentColor } from '../../utils/agentColors.js';

interface ConversationTurn {
  userMessage: StoreMessageType;
  aiMessages: StoreMessageType[];
  isActive: boolean;
}

/** Renders a system message (command result) */
const SystemMessage = React.memo(function SystemMessage({
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

const ConversationTurnCard = React.memo(function ConversationTurnCard({
  turn,
}: {
  turn: ConversationTurn;
}) {
  const { isProcessing } = useConversationState();

  // Always use the stored agent name from the message
  // This preserves the color even when switching agents
  const agentName =
    'agentName' in turn.userMessage ? turn.userMessage.agentName : undefined;
  const agentBarColor = agentName ? getAgentColor(agentName).hex : undefined;

  // Check if the last AI message is still active (streaming or executing)
  const lastAiMsg = turn.aiMessages[turn.aiMessages.length - 1];
  const hasActiveContent = lastAiMsg
    ? (lastAiMsg.role === MessageRole.ToolUse && !lastAiMsg.isFinished) ||
      (lastAiMsg.role === MessageRole.Model &&
        turn.isActive &&
        isProcessing &&
        !!lastAiMsg.content)
    : false;

  // Show thinking when processing but no active content
  // (visible between tool calls, hidden while streaming)
  const showThinking = turn.isActive && isProcessing && !hasActiveContent;

  // Don't render if user message has no content
  if (!turn.userMessage.content) return null;

  // Find the last Model message index for streaming treatment
  const lastModelIndex = turn.aiMessages.findLastIndex(
    (msg) => msg.role === MessageRole.Model
  );

  return (
    <Card active={turn.isActive}>
      {/* User message */}
      <Message
        key={turn.userMessage.id}
        content={turn.userMessage.content}
        type={MessageType.DEVELOPER}
        barColor={agentBarColor}
      />

      {/* AI responses - render all messages, let individual components handle empty content */}
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
            />
          );
        }
        // Only filter empty content for text messages
        if (!message.content || message.content === '') return null;

        // Use StreamingMessage for the last Model message in an active, processing turn
        const isLastModel = index === lastModelIndex;
        const shouldStream = turn.isActive && isProcessing && isLastModel;

        if (shouldStream) {
          return (
            <StreamingMessage
              key={message.id}
              content={message.content}
              type={MessageType.AGENT}
              isStreaming={true}
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

      {/* Thinking indicator */}
      {showThinking && (
        <ThinkingMessage
          key={`${turn.userMessage.id}-thinking`}
          barColor={agentBarColor}
        />
      )}
    </Card>
  );
});

/** Static turn card for completed turns - no store subscriptions */
const StaticTurnCard = React.memo(function StaticTurnCard({
  turn,
}: {
  turn: ConversationTurn;
}) {
  // Use the stored agent name from the message
  const agentName =
    'agentName' in turn.userMessage ? turn.userMessage.agentName : undefined;
  const agentBarColor = agentName ? getAgentColor(agentName).hex : undefined;

  // Don't render if user message has no content
  if (!turn.userMessage.content) return null;

  return (
    <Box marginBottom={1}>
      <Card active={false}>
        {/* User message */}
        <Message
          key={turn.userMessage.id}
          content={turn.userMessage.content}
          type={MessageType.DEVELOPER}
          barColor={agentBarColor}
        />

        {/* AI responses - render all messages, let individual components handle empty content */}
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
          // Only filter empty content for text messages
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

export const ConversationView = React.memo(function ConversationView() {
  const { messages } = useConversationState();

  // Track if we've ever had messages (to know if this is initial load or post-clear)
  const hadMessagesRef = React.useRef(false);
  // Track if welcome was already added to Static
  const welcomeInStaticRef = React.useRef(false);

  if (messages.length > 0) {
    hadMessagesRef.current = true;
  }

  const hasMessages = messages.length > 0;
  const isInitialLoad = !hasMessages && !hadMessagesRef.current;

  // Separate system messages and conversation messages
  const systemMessages: Array<StoreMessageType & { role: MessageRole.System }> =
    [];
  const conversationMessages: StoreMessageType[] = [];

  messages.forEach((message) => {
    if (message.role === MessageRole.System) {
      systemMessages.push(
        message as StoreMessageType & { role: MessageRole.System }
      );
    } else {
      conversationMessages.push(message);
    }
  });

  // Group conversation messages into turns
  const turns: ConversationTurn[] = [];
  let currentTurn: ConversationTurn | null = null;

  conversationMessages.forEach((message) => {
    if (message.role === MessageRole.User) {
      // Mark previous turn as inactive since a new turn is starting
      if (currentTurn) {
        currentTurn.isActive = false;
        turns.push(currentTurn);
      }
      // Start new turn - active by default until next turn comes in
      currentTurn = {
        userMessage: message,
        aiMessages: [],
        isActive: true,
      };
    } else if (currentTurn) {
      // Add AI message to current turn
      currentTurn.aiMessages.push(message);
    }
  });

  // Add the last turn if it exists
  if (currentTurn) {
    turns.push(currentTurn);
  }

  // Split turns into completed (static) and active (dynamic)
  const completedTurns = turns.filter((t) => !t.isActive);
  const activeTurn = turns.find((t) => t.isActive);

  // Create a combined static items array with type discriminator
  type StaticItem =
    | { type: 'welcome'; id: string }
    | {
        type: 'system';
        id: string;
        message: StoreMessageType & { role: MessageRole.System };
      }
    | { type: 'turn'; id: string; turn: ConversationTurn };

  const staticItems: StaticItem[] = [];

  // Add welcome screen to static ONCE when first message arrives
  if (hasMessages && !welcomeInStaticRef.current) {
    staticItems.push({ type: 'welcome', id: '__welcome__' });
    welcomeInStaticRef.current = true;
  }

  // Add system messages
  systemMessages.forEach((msg) => {
    staticItems.push({ type: 'system', id: msg.id, message: msg });
  });

  // Add completed turns
  completedTurns.forEach((turn) => {
    staticItems.push({ type: 'turn', id: turn.userMessage.id, turn });
  });

  return (
    <Box flexDirection="column">
      {/* Welcome screen - only shown on initial load, not after clear */}
      {isInitialLoad && (
        <Box marginBottom={1}>
          <WelcomeScreen agent="kiro" mcpServers={[]} animate={true} />
        </Box>
      )}

      {/* Render static content: welcome + system messages + completed turns */}
      {staticItems.length > 0 && (
        <Static items={staticItems}>
          {(item) => {
            if (item.type === 'welcome') {
              return (
                <Box key={item.id} marginBottom={1}>
                  <WelcomeScreen agent="kiro" mcpServers={[]} animate={false} />
                </Box>
              );
            }
            if (item.type === 'system') {
              return <SystemMessage key={item.id} message={item.message} />;
            }
            if (item.type === 'turn') {
              return <StaticTurnCard key={item.id} turn={item.turn} />;
            }
            return null;
          }}
        </Static>
      )}

      {/* Only the active turn is dynamically rendered */}
      {activeTurn && (
        <Box marginBottom={0}>
          <ConversationTurnCard turn={activeTurn} />
        </Box>
      )}
    </Box>
  );
});
