import { Box } from 'ink';
import { useAppStore, MessageRole, type MessageType as StoreMessageType } from '../../stores/app-store';
import { Card } from '../ui/card/Card';
import { Message, MessageType } from '../chat/message/Message';
import { ToolUseMessage } from './ToolUseMessage';
import { ThinkingIndicator } from './ThinkingIndicator';
import { StatusBar } from '../chat/status-bar/StatusBar';
import { Text } from '../ui/text/Text';

interface ConversationTurn {
  userMessage: StoreMessageType;
  aiMessages: StoreMessageType[];
  isActive: boolean;
}

/** Renders a system message (command result) */
const SystemMessage: React.FC<{ message: StoreMessageType & { role: MessageRole.System } }> = ({ message }) => {
  return (
    <Box paddingX={1} marginY={1}>
      <StatusBar status={message.success ? 'success' : 'error'}>
        <Text>{message.content}</Text>
      </StatusBar>
    </Box>
  );
};

const ConversationTurnCard: React.FC<{ turn: ConversationTurn }> = ({ turn }) => {
  const isProcessing = useAppStore((state) => state.isProcessing);
  const showThinking = turn.isActive && isProcessing && turn.aiMessages.length === 0;

  // Don't render if user message has no content
  if (!turn.userMessage.content) return null;

  return (
    <Card active={turn.isActive}>
      {/* User message */}
      <Message 
        key={turn.userMessage.id} 
        content={turn.userMessage.content} 
        type={MessageType.DEVELOPER} 
      />
      
      {/* AI responses - filter out empty messages */}
      {turn.aiMessages
        .filter((m) => m.content && m.content !== '')
        .map((message) => {
          if (message.role === MessageRole.ToolUse) {
            return (
              <ToolUseMessage 
                key={message.id}
                id={message.id}
                name={message.name} 
                content={message.content}
                isFinished={message.isFinished}
                status={message.status}
                result={message.result}
              />
            );
          }
          return (
            <Message 
              key={message.id} 
              content={message.content} 
              type={MessageType.AGENT} 
            />
          );
        })}

      {/* Thinking indicator */}
      {showThinking && <ThinkingIndicator key={`${turn.userMessage.id}-thinking`} />}
    </Card>
  );
};

export const ConversationView: React.FC = () => {
  const messages = useAppStore((state) => state.messages);

  if (messages.length === 0) {
    return null;
  }

  // Separate system messages and conversation messages
  const systemMessages: Array<StoreMessageType & { role: MessageRole.System }> = [];
  const conversationMessages: StoreMessageType[] = [];
  
  messages.forEach((message) => {
    if (message.role === MessageRole.System) {
      systemMessages.push(message as StoreMessageType & { role: MessageRole.System });
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

  return (
    <Box flexDirection="column">
      {/* Render system messages (command results) */}
      {systemMessages.map((msg) => (
        <SystemMessage key={msg.id} message={msg} />
      ))}
      
      {/* Render conversation turns */}
      {turns.map((turn, index) => {
        const turnKey = `${turn.userMessage.id}-${turn.aiMessages.length}-${turn.isActive ? 'active' : 'complete'}`;
        return (
          <Box key={turnKey} marginBottom={index < turns.length - 1 ? 1 : 0}>
            <ConversationTurnCard turn={turn} />
          </Box>
        );
      })}
    </Box>
  );
};
