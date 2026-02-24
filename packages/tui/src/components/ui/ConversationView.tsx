import React from 'react';
import { Box, Static, Text as InkText } from 'ink';
import {
  MessageRole,
  type MessageType as StoreMessageType,
} from '../../stores/app-store';
import { useConversationState } from '../../stores/selectors';
import { QueueStack } from './QueueStack';
import { Card, CardContext } from '../ui/card/Card';
import { Divider } from '../ui/divider/Divider';
import { Message, MessageType } from '../chat/message/Message';
import { StreamingMessage } from '../chat/message/StreamingMessage';
import { ShellOutputMessage } from '../chat/message/ShellOutputMessage';
import { ToolUseMessage } from './ToolUseMessage';
import { ThinkingMessage } from '../chat/message/ThinkingMessage';
import { StatusBar } from '../chat/status-bar/StatusBar';
import { Text } from '../ui/text/Text';
import { WelcomeScreen } from '../welcome-screen/index.js';
import { getAgentColor } from '../../utils/agentColors.js';
import { computeFlushCount } from '../../utils/turn-flush-machine.js';

interface ConversationTurn {
  userMessage: StoreMessageType;
  aiMessages: StoreMessageType[];
  isActive: boolean;
}

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

/** Renders a single message (user, tool use, or model text) in static context */
const StaticMessage = React.memo(function StaticMessage({
  message,
  agentBarColor,
}: {
  message: StoreMessageType;
  agentBarColor: string | undefined;
}) {
  if (message.role === MessageRole.User) {
    return (
      <Message
        content={message.content}
        type={MessageType.DEVELOPER}
        barColor={agentBarColor}
      />
    );
  }
  if (message.role === MessageRole.ToolUse) {
    return (
      <ToolUseMessage
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
  if (message.role === MessageRole.Model) {
    if (!message.content) return null;
    if ('shellOutput' in message && message.shellOutput) {
      return (
        <ShellOutputMessage
          content={message.content}
          isStatic={true}
          barColor={agentBarColor}
        />
      );
    }
    return (
      <Message
        content={message.content}
        type={MessageType.AGENT}
        barColor={agentBarColor}
      />
    );
  }
  return null;
});

/** Active turn tail — renders the last N messages dynamically, no Card/Divider */
const ActiveTurnTail = React.memo(function ActiveTurnTail({
  tailMessages,
  agentBarColor,
}: {
  tailMessages: StoreMessageType[];
  agentBarColor: string | undefined;
}) {
  const { isProcessing } = useConversationState();

  const lastAiMsg = tailMessages[tailMessages.length - 1];
  const hasActiveContent = lastAiMsg
    ? (lastAiMsg.role === MessageRole.ToolUse && !lastAiMsg.isFinished) ||
      (lastAiMsg.role === MessageRole.Model &&
        isProcessing &&
        !!lastAiMsg.content)
    : false;
  const showThinking = isProcessing && !hasActiveContent;

  const lastModelIndex = tailMessages.findLastIndex(
    (msg) => msg.role === MessageRole.Model
  );

  return (
    <>
      {tailMessages.map((message, index) => {
        if (message.role === MessageRole.User) {
          return (
            <Message
              key={message.id}
              content={message.content}
              type={MessageType.DEVELOPER}
              barColor={agentBarColor}
            />
          );
        }
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
        if (!message.content || message.content === '') return null;
        const isLastModel = index === lastModelIndex;
        const shouldStream = isProcessing && isLastModel;
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
        if ('shellOutput' in message && message.shellOutput) {
          return (
            <ShellOutputMessage
              key={message.id}
              content={message.content}
              isStatic={false}
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
      {showThinking && <ThinkingMessage barColor={agentBarColor} />}
    </>
  );
});

/** Static turn card for completed turns that were never incrementally flushed */
const StaticTurnCard = React.memo(function StaticTurnCard({
  turn,
}: {
  turn: ConversationTurn;
}) {
  const agentName =
    'agentName' in turn.userMessage ? turn.userMessage.agentName : undefined;
  const agentBarColor = agentName ? getAgentColor(agentName).hex : undefined;

  if (!turn.userMessage.content) return null;

  const hasAiContent = turn.aiMessages.some(
    (msg) =>
      msg.role === MessageRole.ToolUse || (msg.content && msg.content !== '')
  );

  return (
    <Box marginBottom={1}>
      <Card active={false}>
        <Message
          content={turn.userMessage.content}
          type={MessageType.DEVELOPER}
          barColor={agentBarColor}
        />
        {turn.aiMessages.map((message) => (
          <StaticMessage
            key={message.id}
            message={message}
            agentBarColor={agentBarColor}
          />
        ))}
        {!hasAiContent && (
          <StatusBar status="error">
            <InkText dimColor italic>
              Cancelled
            </InkText>
          </StatusBar>
        )}
      </Card>
    </Box>
  );
});

// How many messages to keep in the dynamic tail
const TAIL_SIZE = 2;

export const ConversationView = React.memo(function ConversationView() {
  const { messages, isProcessing } = useConversationState();

  const hadMessagesRef = React.useRef(false);
  const welcomeInStaticRef = React.useRef(false);
  // Per-turn set of message IDs already flushed to <Static>
  const flushedRef = React.useRef<Map<string, Set<string>>>(new Map());

  if (messages.length > 0) hadMessagesRef.current = true;

  const hasMessages = messages.length > 0;
  const isInitialLoad = !hasMessages && !hadMessagesRef.current;

  const systemMessages: Array<StoreMessageType & { role: MessageRole.System }> =
    [];
  const conversationMessages: StoreMessageType[] = [];

  messages.forEach((msg) => {
    if (msg.role === MessageRole.System) {
      systemMessages.push(
        msg as StoreMessageType & { role: MessageRole.System }
      );
    } else {
      conversationMessages.push(msg);
    }
  });

  const turns: ConversationTurn[] = [];
  let currentTurn: ConversationTurn | null = null;
  conversationMessages.forEach((msg) => {
    if (msg.role === MessageRole.User) {
      if (currentTurn) {
        currentTurn.isActive = false;
        turns.push(currentTurn);
      }
      currentTurn = { userMessage: msg, aiMessages: [], isActive: true };
    } else if (currentTurn) {
      currentTurn.aiMessages.push(msg);
    }
  });
  if (currentTurn) turns.push(currentTurn);

  const completedTurns = turns.filter((t) => !t.isActive);
  const activeTurn = turns.find((t) => t.isActive);

  const activeAgentName =
    activeTurn && 'agentName' in activeTurn.userMessage
      ? activeTurn.userMessage.agentName
      : undefined;
  const activeAgentBarColor = activeAgentName
    ? getAgentColor(activeAgentName).hex
    : undefined;

  // --- Compute what to flush for the active turn via state machine ---
  const activeAllMessages: StoreMessageType[] = activeTurn
    ? [activeTurn.userMessage, ...activeTurn.aiMessages]
    : [];
  const flushCount = computeFlushCount(
    activeAllMessages,
    isProcessing,
    TAIL_SIZE
  );
  const toFlush = activeAllMessages.slice(0, flushCount);
  const tailMessages = activeAllMessages.slice(flushCount);

  // Track which turns had incremental flushing (so StaticTurnCard skips them on completion).
  // Use a ref keyed by turnId — only needs to know IF a turn was flushed, not which messages.
  if (activeTurn && toFlush.length > 0) {
    const turnId = activeTurn.userMessage.id;
    if (!flushedRef.current.has(turnId)) {
      flushedRef.current.set(turnId, new Set());
    }
    // Record flushed IDs so completed turn path knows what to skip.
    // Safe to do here because we only add IDs, never remove them.
    const flushedIds = flushedRef.current.get(turnId)!;
    toFlush.forEach((msg) => flushedIds.add(msg.id));
  }

  // --- Build static items ---
  type StaticItem =
    | { type: 'welcome'; id: string }
    | {
        type: 'system';
        id: string;
        message: StoreMessageType & { role: MessageRole.System };
      }
    | { type: 'turn'; id: string; turn: ConversationTurn }
    | { type: 'divider'; id: string }
    | {
        type: 'msg';
        id: string;
        msg: StoreMessageType;
        agentBarColor: string | undefined;
        isLast: boolean;
      };

  const staticItems: StaticItem[] = [];

  // Always include welcome screen in static items once messages exist.
  // <Static> tracks items by array index, so the welcome must remain in
  // the array on every render to keep indices stable for new items.
  if (hasMessages) {
    staticItems.push({ type: 'welcome', id: '__welcome__' });
    welcomeInStaticRef.current = true;
  }

  systemMessages.forEach((msg) =>
    staticItems.push({ type: 'system', id: msg.id, message: msg })
  );

  // Completed turns: if they were incrementally flushed, emit remaining messages.
  // Ink's <Static> deduplicates by ID — already-printed items are no-ops.
  completedTurns.forEach((turn) => {
    const flushedIds = flushedRef.current.get(turn.userMessage.id);
    if (flushedIds && flushedIds.size > 0) {
      const agentName =
        'agentName' in turn.userMessage
          ? turn.userMessage.agentName
          : undefined;
      const agentBarColor = agentName
        ? getAgentColor(agentName).hex
        : undefined;
      const allMsgs = [turn.userMessage, ...turn.aiMessages];
      allMsgs.forEach((msg, i) => {
        staticItems.push({
          type: 'msg',
          id: msg.id,
          msg,
          agentBarColor,
          isLast: i === allMsgs.length - 1,
        });
      });
    } else {
      staticItems.push({ type: 'turn', id: turn.userMessage.id, turn });
    }
  });

  // Active turn: emit divider + all toFlush messages every render.
  // Ink's <Static> only renders each ID once — re-emitting is a no-op.
  if (activeTurn && toFlush.length > 0) {
    const turnId = activeTurn.userMessage.id;
    staticItems.push({ type: 'divider', id: `${turnId}__divider` });
    toFlush.forEach((msg) => {
      staticItems.push({
        type: 'msg',
        id: msg.id,
        msg,
        agentBarColor: activeAgentBarColor,
        isLast: false,
      });
    });
  }

  return (
    <Box flexDirection="column">
      {isInitialLoad && (
        <Box marginBottom={1}>
          <WelcomeScreen agent="kiro" mcpServers={[]} animate={true} />
        </Box>
      )}

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
            if (item.type === 'divider') {
              return (
                <Box key={item.id}>
                  <Box flexDirection="column" width="100%">
                    <Divider />
                  </Box>
                </Box>
              );
            }
            if (item.type === 'msg') {
              return (
                <Box key={item.id} marginBottom={item.isLast ? 1 : 0}>
                  <StaticMessage
                    message={item.msg}
                    agentBarColor={item.agentBarColor}
                  />
                </Box>
              );
            }
            return null;
          }}
        </Static>
      )}

      <QueueStack />

      {/* Active turn tail: last TAIL_SIZE messages, wrapped in CardContext for the left bar.
          No Card/Divider — those are already in <Static> once flushing starts.
          Before any flushing (short turns), use full Card for correct divider. */}
      {activeTurn &&
        (toFlush.length > 0 ? (
          // Flushing has started — render tail without Card (divider already in static)
          <CardContext.Provider value={{ active: true }}>
            <Box flexDirection="column" width="100%">
              <ActiveTurnTail
                tailMessages={tailMessages}
                agentBarColor={activeAgentBarColor}
              />
            </Box>
          </CardContext.Provider>
        ) : (
          // Nothing flushed yet — render full Card with divider as normal
          <Box marginBottom={0}>
            <Card active={true}>
              <ActiveTurnTail
                tailMessages={tailMessages}
                agentBarColor={activeAgentBarColor}
              />
            </Card>
          </Box>
        ))}
    </Box>
  );
});
