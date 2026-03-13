import React from 'react';
import { Box, Static, Text as InkText } from './../../renderer.js';
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
import { useTerminalSize } from '../../hooks/useTerminalSize.js';
import { useTheme } from '../../hooks/useThemeContext.js';

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
  onReadyToFlush,
}: {
  tailMessages: StoreMessageType[];
  agentBarColor: string | undefined;
  onReadyToFlush?: () => void;
}) {
  const { isProcessing } = useConversationState();
  const { height: termHeight } = useTerminalSize();

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
              onReadyToFlush={onReadyToFlush}
            />
          );
        }
        if (
          isLastModel &&
          message.content &&
          message.content.split('\n').length > termHeight - 13
        ) {
          return (
            <StreamingMessage
              key={message.id}
              content={message.content}
              type={MessageType.AGENT}
              isStreaming={false}
              barColor={agentBarColor}
              onReadyToFlush={onReadyToFlush}
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
  const { getColor } = useTheme();
  const agentName =
    'agentName' in turn.userMessage ? turn.userMessage.agentName : undefined;
  const agentBarColor = agentName
    ? getAgentColor(agentName, getColor).hex
    : undefined;

  if (!turn.userMessage.content && !turn.aiMessages.length) return null;

  // Orphan model message (e.g. welcome message) — render as standalone AI response
  const isOrphanModel = turn.userMessage.role === MessageRole.Model;

  const hasAiContent =
    isOrphanModel ||
    turn.aiMessages.some(
      (msg) =>
        msg.role === MessageRole.ToolUse || (msg.content && msg.content !== '')
    );

  return (
    <Box marginBottom={1}>
      <Card active={false}>
        {isOrphanModel ? (
          <Message
            content={turn.userMessage.content}
            type={MessageType.AGENT}
            barColor={agentBarColor}
            status="success"
          />
        ) : (
          <Message
            content={turn.userMessage.content}
            type={MessageType.DEVELOPER}
            barColor={agentBarColor}
          />
        )}
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

/**
 * # ConversationView — Incremental Static Rendering
 *
 * ## Architecture overview
 *
 * The terminal has two rendering zones:
 *
 *   ┌─────────────────────────────────────┐
 *   │  <Static>  — permanent, scrollback  │  ← completed content lives here
 *   │  (printed once, never redrawn)      │
 *   ├─────────────────────────────────────┤
 *   │  Dynamic tail — redrawn every frame │  ← active tool + streaming text
 *   └─────────────────────────────────────┘
 *
 * Ink's `<Static>` works as a **length cursor**, not an ID map:
 *
 *   const [index, setIndex] = useState(0);
 *   const itemsToRender = useMemo(() => items.slice(index), [items, index]);
 *   useLayoutEffect(() => setIndex(items.length), [items.length]);
 *
 * Consequences:
 *   1. The `items` array must be **append-only** — never remove or reorder items.
 *   2. `items` must be a **new array reference** each render so `useMemo` fires.
 *      Passing the same mutated reference makes `<Static>` blind to new items.
 *   3. There is **no ID deduplication** — re-emitting an item prints it again.
 *
 * ## Persistent ref pattern
 *
 * `staticItemsRef` is the single source of truth for all items ever emitted to
 * `<Static>`. It only grows. Each render we:
 *   1. Compute `newlyFlushed` — messages ready to leave the dynamic tail.
 *   2. Call `appendStatic()` for each new item (guarded by `emittedIds`).
 *   3. Pass `[...staticItemsRef.current]` to `<Static>` — new reference, same
 *      contents — so `useMemo([items, index])` re-evaluates correctly.
 *
 * ## Turn lifecycle
 *
 *   ACTIVE TURN
 *     │  computeFlushCount() decides how many leading messages are "done"
 *     │  keeping the last TAIL_SIZE in the dynamic area.
 *     │  newlyFlushed = toFlush − already in flushedRef  →  appendStatic()
 *     │  tail = last TAIL_SIZE messages  →  rendered in dynamic Card/CardContext
 *     ▼
 *   TURN COMPLETES (new user message arrives)
 *     │  Turn moves from activeTurn → completedTurns
 *     ├─ Never flushed (short turn, ≤ TAIL_SIZE messages):
 *     │    appendStatic({ type: 'turn' })  →  StaticTurnCard renders everything
 *     └─ Partially flushed (long turn):
 *          append only the unflushed tail (allMsgs − flushedIds)
 *          with isLast=true on the final message for bottom spacing
 *
 * ## Why TAIL_SIZE = 2
 *
 * We always keep the last finished tool + the current streaming model message
 * (or the running tool) visible in the dynamic area. This gives the user live
 * feedback without flickering. They move to `<Static>` only when the next turn
 * starts, so the transition is seamless.
 *
 * ## flushedRef
 *
 * `Map<turnId, Set<messageId>>` — tracks which message IDs have been appended
 * to `staticItemsRef` for each turn. Used for:
 *   - `newlyFlushed` computation (filter out already-emitted messages)
 *   - `completedTurns` path (know which messages are the unflushed tail)
 *   - Tail rendering condition (`flushedRef.has(turnId)` → use CardContext not Card)
 */

// How many messages to keep in the dynamic tail
const TAIL_SIZE = 2;

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

export const ConversationView = React.memo(function ConversationView() {
  const { messages, isProcessing } = useConversationState();
  const { getColor } = useTheme();

  const hadMessagesRef = React.useRef(false);
  const welcomeInStaticRef = React.useRef(false);
  const [tailOverride, setTailOverride] = React.useState<number | null>(null);
  const lastFlushTurnRef = React.useRef<string | undefined>(undefined);
  const activeTurnIdRef = React.useRef<string | undefined>(undefined);

  const handleReadyToFlush = React.useCallback(() => {
    setTailOverride(0);
    lastFlushTurnRef.current = activeTurnIdRef.current;
  }, []);
  // Per-turn set of message IDs already flushed to <Static>
  const flushedRef = React.useRef<Map<string, Set<string>>>(new Map());
  // Persistent, append-only array of static items — never shrinks.
  // <Static> uses array length as its index, so items must stay at stable positions.
  const staticItemsRef = React.useRef<StaticItem[]>([]);

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
    } else if (msg.role === MessageRole.Model && (msg as any).standalone) {
      // Standalone model message (e.g. welcome) — close current turn and create its own
      if (currentTurn) {
        currentTurn.isActive = false;
        turns.push(currentTurn);
        currentTurn = null;
      }
      turns.push({ userMessage: msg, aiMessages: [], isActive: false });
    } else if (currentTurn) {
      currentTurn.aiMessages.push(msg);
    } else {
      // Orphan model message — standalone turn
      turns.push({ userMessage: msg, aiMessages: [], isActive: false });
    }
  });
  if (currentTurn) turns.push(currentTurn);

  const completedTurns = turns.filter((t) => !t.isActive);
  const activeTurn = turns.find((t) => t.isActive);

  // Reset tailOverride when active turn changes (new user message)
  const activeTurnId = activeTurn?.userMessage.id;
  activeTurnIdRef.current = activeTurnId;
  const effectiveTailOverride =
    activeTurnId === lastFlushTurnRef.current ? tailOverride : null;

  const activeAgentName =
    activeTurn && 'agentName' in activeTurn.userMessage
      ? activeTurn.userMessage.agentName
      : undefined;
  const activeAgentBarColor = activeAgentName
    ? getAgentColor(activeAgentName, getColor).hex
    : undefined;

  // --- Compute what to flush for the active turn via state machine ---
  const activeAllMessages: StoreMessageType[] = activeTurn
    ? [activeTurn.userMessage, ...activeTurn.aiMessages]
    : [];
  const flushCount = computeFlushCount(
    activeAllMessages,
    isProcessing,
    effectiveTailOverride ?? TAIL_SIZE
  );
  const toFlush = activeAllMessages.slice(0, flushCount);
  const tailMessages = activeAllMessages.slice(flushCount);

  // Track which turns had incremental flushing (so StaticTurnCard skips them on completion).
  // Compute newly-flushed messages BEFORE updating flushedRef so we know what's new this render.
  let newlyFlushed: StoreMessageType[] = [];
  if (activeTurn && toFlush.length > 0) {
    const turnId = activeTurn.userMessage.id;
    if (!flushedRef.current.has(turnId)) {
      flushedRef.current.set(turnId, new Set());
    }
    const flushedIds = flushedRef.current.get(turnId)!;
    newlyFlushed = toFlush.filter((msg) => !flushedIds.has(msg.id));
    newlyFlushed.forEach((msg) => flushedIds.add(msg.id));
  }

  // --- Append new items to the persistent staticItems ref ---
  // <Static> uses array length as its cursor — items must never be removed or reordered.
  // We track emitted IDs/keys in emittedStaticIds to avoid double-appending.
  const emittedIds = new Set(staticItemsRef.current.map((i) => i.id));

  const appendStatic = (item: StaticItem) => {
    if (!emittedIds.has(item.id)) {
      emittedIds.add(item.id);
      staticItemsRef.current.push(item);
    }
  };

  // Welcome screen — emitted once when messages first appear
  if (hasMessages && !welcomeInStaticRef.current) {
    welcomeInStaticRef.current = true;
    appendStatic({ type: 'welcome', id: '__welcome__' });
  }

  // System messages
  systemMessages.forEach((msg) =>
    appendStatic({ type: 'system', id: msg.id, message: msg })
  );

  // Completed turns that were never incrementally flushed → StaticTurnCard.
  // Turns that WERE incrementally flushed → append only the unflushed tail messages.
  completedTurns.forEach((turn) => {
    const flushedIds = flushedRef.current.get(turn.userMessage.id);
    if (!flushedIds || flushedIds.size === 0) {
      // Never flushed — render as a single card
      appendStatic({ type: 'turn', id: turn.userMessage.id, turn });
    } else {
      // Partially flushed — append the tail (messages not yet in static)
      const agentName =
        'agentName' in turn.userMessage
          ? turn.userMessage.agentName
          : undefined;
      const agentBarColor = agentName
        ? getAgentColor(agentName, getColor).hex
        : undefined;
      const allMsgs = [turn.userMessage, ...turn.aiMessages];
      const tailMsgs = allMsgs.filter((msg) => !flushedIds.has(msg.id));
      tailMsgs.forEach((msg, i) => {
        appendStatic({
          type: 'msg',
          id: msg.id,
          msg,
          agentBarColor,
          isLast: i === tailMsgs.length - 1,
        });
        flushedIds.add(msg.id);
      });
    }
  });

  // Active turn: append divider + newly-flushed messages
  if (activeTurn && newlyFlushed.length > 0) {
    const turnId = activeTurn.userMessage.id;
    const flushedIds = flushedRef.current.get(turnId)!;
    // Divider only on first flush
    if (flushedIds.size === newlyFlushed.length) {
      appendStatic({ type: 'divider', id: `${turnId}__divider` });
    }
    newlyFlushed.forEach((msg) => {
      appendStatic({
        type: 'msg',
        id: msg.id,
        msg,
        agentBarColor: activeAgentBarColor,
        isLast: false,
      });
    });
  }

  // Spread into a new array each render so <Static>'s useMemo([items, index]) fires.
  // staticItemsRef holds the persistent contents; the new reference triggers the memo.
  const staticItems = [...staticItemsRef.current];

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
        tailMessages.length > 0 &&
        (flushedRef.current.has(activeTurn.userMessage.id) ? (
          // Flushing has started — render tail without Card (divider already in static)
          <CardContext.Provider value={{ active: true }}>
            <Box flexDirection="column" width="100%">
              <ActiveTurnTail
                tailMessages={tailMessages}
                agentBarColor={activeAgentBarColor}
                onReadyToFlush={handleReadyToFlush}
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
                onReadyToFlush={handleReadyToFlush}
              />
            </Card>
          </Box>
        ))}
    </Box>
  );
});
