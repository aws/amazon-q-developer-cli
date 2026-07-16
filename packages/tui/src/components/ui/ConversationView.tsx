import React, { useMemo } from 'react';
import { Box, Static, Text as InkText } from './../../renderer.js';
import {
  MessageRole,
  type MessageType as StoreMessageType,
} from '../../stores/app-store';
import { useAppStore } from '../../stores/app-store';
import { useConversationState } from '../../stores/selectors';
import { Card, CardContext } from '../ui/card/Card';
import { Divider } from '../ui/divider/Divider';
import { Message, MessageType } from '../chat/message/Message';
import { StreamingMessage } from '../chat/message/StreamingMessage';
import { ShellOutputMessage } from '../chat/message/ShellOutputMessage';
import { ToolUseMessage } from './ToolUseMessage';
import { HideToolArgsContext } from './HideToolArgsContext.js';
import { SubagentToolPanel } from './SubagentToolPanel.js';
import { ThinkingMessage } from '../chat/message/ThinkingMessage';
import { ThinkingDisplay } from '../chat/message/ThinkingDisplay';
import { TurnUsageSummary } from '../chat/message/TurnUsageSummary';
import { StatusBar } from '../chat/status-bar/StatusBar';
import { Text } from '../ui/text/Text';
import { WelcomeScreen } from '../welcome-screen/index.js';
import { WelcomeMessageBar } from './WelcomeMessageBar.js';
import { pickTip } from '../../tips/tips.js';
import { getAgentColor } from '../../utils/agentColors.js';
import { Settings } from '../../constants/settings.js';
import { computeFlushSet } from '../../utils/turn-flush-machine.js';
import { trimStaticItems } from '../../utils/trim-static-items.js';
import { useTerminalSize } from '../../hooks/useTerminalSize.js';
import { useTheme } from '../../hooks/useThemeContext.js';
import { useTwinkiContext } from 'twinki';
import { useThinkingMode } from '../../hooks/useGlyphs.js';
import { SESSION_TOOL_NAMES } from '../../types/agent-events.js';
import type { ConversationTurn } from '../../stores/app-store.js';
import { groupMessagesIntoTurns } from '../../utils/group-turns.js';
import { leadingGap } from '../../utils/message-spacing.js';
import { CLEAR_SCREEN } from '../../utils/terminal-sequences.js';

const CLEAR_SCREEN_AND_HOME = `${CLEAR_SCREEN}\x1b[H`;

/**
 * Resolve prevRole for a message at `index` in a list.
 * Falls back to `fallback` when index is 0 (no previous message in the list).
 */
function resolvePrevRole(
  messages: StoreMessageType[],
  index: number,
  fallback?: MessageRole
): MessageRole | undefined {
  return index > 0 ? messages[index - 1]?.role : fallback;
}

/**
 * Returns true if a tool-use message belongs to a real subagent session.
 * Keyed off the stamped `isSubagentTool` flag (set when sessionId differs from
 * the main session) rather than an agentName comparison — a mid-turn mode
 * switch (plan→vibe) changes agentName without it being a subagent, and the
 * old heuristic wrongly hid that tool's card.
 */
function isSubagentToolCall(msg: StoreMessageType): boolean {
  return msg.role === MessageRole.ToolUse && !!msg.isSubagentTool;
}

const SystemMessage = React.memo(function SystemMessage({
  message,
}: {
  message: StoreMessageType & { role: MessageRole.System };
}) {
  return (
    <Box marginY={1}>
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
  prevRole,
  mainAgentName,
}: {
  message: StoreMessageType;
  agentBarColor: string | undefined;
  prevRole?: MessageRole;
  mainAgentName?: string;
}) {
  const { thinkingMode } = useThinkingMode();
  if (message.role === MessageRole.System) {
    return (
      <SystemMessage
        message={message as StoreMessageType & { role: MessageRole.System }}
      />
    );
  }
  if (message.role === MessageRole.User) {
    // Steered (mid-turn injected) messages get a blank line above so it's
    // clear where the steer landed within the agent's ongoing output. Regular
    // prompts render via <Message> directly (not here), so this only affects
    // injected steer bubbles in a turn body.
    return (
      <Box marginTop={leadingGap(message, prevRole)}>
        <Message
          content={message.content}
          type={MessageType.DEVELOPER}
          barColor={agentBarColor}
        />
      </Box>
    );
  }
  if (message.role === MessageRole.ToolUse) {
    // Skip subagent tool calls — they are rendered via SubagentToolPanel
    if (isSubagentToolCall(message)) return null;

    // Spec mode hides tool args/diff/output to keep the conversation clean.
    return (
      <HideToolArgsContext.Provider value={mainAgentName === 'spec'}>
        <ToolUseMessage
          id={message.id}
          name={message.name}
          kind={message.kind}
          content={message.content}
          diff={message.diff}
          isFinished={true}
          isStatic={true}
          status={message.status}
          result={message.result}
          locations={message.locations}
          barColor={agentBarColor}
        />
      </HideToolArgsContext.Provider>
    );
  }
  if (message.role === MessageRole.Model) {
    const thinkingText =
      thinkingMode !== 'off' && 'thinking' in message
        ? message.thinking
        : undefined;
    // Skip messages whose only content is hidden thinking — otherwise we'd
    // render an empty wrapping Box and leave a stray blank row in the
    // scrollback when `chat.showThinking` is off.
    if (!message.content && !thinkingText) return null;
    const isShell = 'shellOutput' in message && message.shellOutput;
    return (
      <Box flexDirection="column" marginTop={leadingGap(message, prevRole)}>
        {thinkingText && (
          <ThinkingDisplay
            text={thinkingText}
            mode={thinkingMode}
            thinkingMs={
              'thinkingMs' in message ? message.thinkingMs : undefined
            }
            isStatic
            barColor={agentBarColor}
          />
        )}
        {isShell ? (
          <ShellOutputMessage
            content={message.content}
            isStatic={true}
            barColor={agentBarColor}
          />
        ) : (
          <Message
            content={message.content}
            type={MessageType.AGENT}
            barColor={agentBarColor}
          />
        )}
      </Box>
    );
  }
  return null;
});

/** Active turn tail — renders the last N messages dynamically, no Card/Divider */
const ActiveTurnTail = React.memo(function ActiveTurnTail({
  tailMessages,
  agentBarColor,
  mainAgentName,
  onReadyToFlush,
  prevFlushedRole,
  turnId,
}: {
  tailMessages: StoreMessageType[];
  agentBarColor: string | undefined;
  mainAgentName: string | undefined;
  onReadyToFlush?: () => void;
  prevFlushedRole?: MessageRole;
  turnId: string;
}) {
  const { isProcessing } = useConversationState();
  const { height: termHeight } = useTerminalSize();
  const { thinkingMode } = useThinkingMode();
  const summaryText = useAppStore((s) => s.turnSummaries.get(turnId));
  // Live streaming content lives in its own slot (post-E1). The placeholder
  // Model row in `tailMessages` has empty content while streaming; the live
  // text is in `streamingContent` and the row that owns it is identified by
  // `streamingMessageId`. Subscribing to these here means per-chunk content
  // updates re-render only this component, not every memo with [messages]
  // deps elsewhere in the layout.
  const streamingContent = useAppStore((s) => s.streamingContent);
  const streamingMessageId = useAppStore((s) => s.streamingMessageId);

  // Find the last message that isn't a subagent tool call (those are hidden in rendering)
  const lastVisibleMsg = useMemo(() => {
    for (let i = tailMessages.length - 1; i >= 0; i--) {
      if (!isSubagentToolCall(tailMessages[i]!)) return tailMessages[i];
    }
    return undefined;
  }, [tailMessages]);
  const hasActiveContent = lastVisibleMsg
    ? (lastVisibleMsg.role === MessageRole.ToolUse &&
        !lastVisibleMsg.isFinished) ||
      (lastVisibleMsg.role === MessageRole.Model &&
        isProcessing &&
        // Treat the live streaming slot as "active content" so the thinking
        // indicator yields the moment text starts arriving — message.content
        // stays empty until commit.
        (!!lastVisibleMsg.content ||
          (lastVisibleMsg.id === streamingMessageId && !!streamingContent) ||
          ('shellOutput' in lastVisibleMsg && lastVisibleMsg.shellOutput) ||
          ('thinking' in lastVisibleMsg && !!lastVisibleMsg.thinking)))
    : false;
  const showThinking = isProcessing && !hasActiveContent;

  const lastModelIndex = tailMessages.findLastIndex(
    (msg) => msg.role === MessageRole.Model
  );

  return (
    <HideToolArgsContext.Provider value={mainAgentName === 'spec'}>
      {tailMessages.map((message, index) => {
        const prevRole = resolvePrevRole(tailMessages, index, prevFlushedRole);

        if (message.role === MessageRole.User) {
          return (
            <Box key={message.id} marginTop={leadingGap(message, prevRole)}>
              <Message
                content={message.content}
                type={MessageType.DEVELOPER}
                barColor={agentBarColor}
              />
            </Box>
          );
        }
        if (message.role === MessageRole.System) {
          return (
            <SystemMessage
              key={message.id}
              message={
                message as StoreMessageType & { role: MessageRole.System }
              }
            />
          );
        }
        if (message.role === MessageRole.ToolUse) {
          // Skip subagent tool calls — rendered via SubagentToolPanel
          if (isSubagentToolCall(message)) return null;
          const isSessionTool = SESSION_TOOL_NAMES.has(message.name);
          return (
            <React.Fragment key={message.id}>
              <ToolUseMessage
                id={message.id}
                name={message.name}
                kind={message.kind}
                content={message.content}
                diff={message.diff}
                isFinished={message.isFinished}
                status={message.status}
                result={message.result}
                locations={message.locations}
                barColor={agentBarColor}
              />
              {isSessionTool && !message.isFinished && <SubagentToolPanel />}
            </React.Fragment>
          );
        }
        const thinkingText =
          thinkingMode !== 'off' && 'thinking' in message
            ? message.thinking
            : undefined;
        // Pre-E1 the live streaming row mutated `message.content` per chunk;
        // post-E1 the row is the placeholder for streamingContent. Substitute
        // the live text whenever this row owns the streaming slot so the
        // empty-content drop and the wrap calc below see real content.
        const liveContent =
          message.id === streamingMessageId && isProcessing
            ? streamingContent
            : message.content;
        // Skip messages whose only content is hidden thinking — otherwise we'd
        // render an empty wrapping Box and leave a stray blank row in the
        // streaming scrollback when `chat.showThinking` is off.
        if (
          (!liveContent || liveContent === '') &&
          !('shellOutput' in message && message.shellOutput) &&
          !thinkingText
        )
          return null;

        const isLastModel = index === lastModelIndex;
        const isShell = 'shellOutput' in message && message.shellOutput;
        const useStreaming =
          isLastModel &&
          (isProcessing || liveContent.split('\n').length > termHeight - 13);

        // Determine inner content for Model messages
        let inner: React.ReactNode;
        if (isShell) {
          inner = (
            <ShellOutputMessage
              content={liveContent}
              isStatic={false}
              isRunning={isProcessing}
              barColor={agentBarColor}
            />
          );
        } else if (useStreaming) {
          inner = (
            <StreamingMessage
              content={liveContent}
              type={MessageType.AGENT}
              isStreaming={isProcessing}
              barColor={agentBarColor}
              onReadyToFlush={onReadyToFlush}
            />
          );
        } else {
          inner = (
            <Message
              content={liveContent}
              type={MessageType.AGENT}
              barColor={agentBarColor}
            />
          );
        }

        return (
          <Box
            key={message.id}
            flexDirection="column"
            marginTop={leadingGap(message, prevRole)}
          >
            {thinkingText && (
              <ThinkingDisplay
                text={thinkingText}
                mode={thinkingMode}
                thinkingMs={
                  'thinkingMs' in message ? message.thinkingMs : undefined
                }
                barColor={agentBarColor}
              />
            )}
            {inner}
          </Box>
        );
      })}
      {showThinking && <ThinkingMessage barColor={agentBarColor} />}
      {!isProcessing && summaryText && (
        <Box marginTop={1}>
          <TurnUsageSummary text={summaryText} />
        </Box>
      )}
    </HideToolArgsContext.Provider>
  );
});

/** Static turn card for completed turns that were never incrementally flushed */
const StaticTurnCard = React.memo(function StaticTurnCard({
  turn,
}: {
  turn: ConversationTurn;
}) {
  const { getColor } = useTheme();
  const summaryText = useAppStore((s) =>
    s.turnSummaries.get(turn.userMessage.id)
  );
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
        msg.role === MessageRole.ToolUse ||
        // Only assistant output counts as "content". Steered user bubbles and
        // system/status rows can live in the body, but must not mask a turn
        // that produced no actual response.
        (msg.role === MessageRole.Model && !!msg.content && msg.content !== '')
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
        {turn.aiMessages.map((message, index) => (
          <StaticMessage
            key={message.id}
            message={message}
            agentBarColor={agentBarColor}
            prevRole={resolvePrevRole(turn.aiMessages, index, MessageRole.User)}
            mainAgentName={agentName}
          />
        ))}
        {!hasAiContent && (
          <StatusBar status="error">
            <InkText dimColor italic>
              Cancelled
            </InkText>
          </StatusBar>
        )}
        {summaryText && (
          <Box marginTop={1}>
            <TurnUsageSummary text={summaryText} />
          </Box>
        )}
      </Card>
    </Box>
  );
});

// Module-level: survive component unmount/remount (e.g. ctrl+g crew monitor toggle)
let _hadMessages = false;
let _hadUserMessage = false;
let _welcomeInStatic = false;
let _hasAnimated = false;

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
      mainAgentName: string | undefined;
      isLast: boolean;
      prevRole?: MessageRole;
    }
  | { type: 'summary'; id: string; text: string };

// These must also be module-level so that <Static> items, emitted IDs, and
// per-turn flush tracking survive the unmount/remount cycle that happens when
// toggling alternate screen (Ctrl+G crew monitor). If these reset on remount,
// the twinki ReactBridge's monotonic totalStaticWritten cursor will skip the
// re-emitted items because their indices overlap with already-written ones.
const _staticItems: StaticItem[] = [];
const _emittedIds = new Set<string>();
const _flushedMap = new Map<string, Set<string>>();
let _lastObservedClearToken = -1;

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
 *     │  computeFlushSet() decides which messages are "done"
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

/** Helper to append messages to static items with correct prevRole tracking */
function appendMessagesToStatic(
  messages: StoreMessageType[],
  agentBarColor: string | undefined,
  appendStatic: (item: StaticItem) => void,
  opts: {
    isLast?: (i: number) => boolean;
    fallbackPrevRole?: MessageRole;
    mainAgentName?: string;
  }
) {
  messages.forEach((msg, i) => {
    const prevRole = resolvePrevRole(messages, i, opts.fallbackPrevRole);
    appendStatic({
      type: 'msg',
      id: msg.id,
      msg,
      agentBarColor,
      mainAgentName: opts.mainAgentName,
      isLast: opts.isLast ? opts.isLast(i) : false,
      prevRole,
    });
  });
}

export const ConversationView = React.memo(function ConversationView() {
  const { messages, isProcessing, settings } = useConversationState();
  const { getColor } = useTheme();
  const { adjustStaticCursor } = useTwinkiContext();
  // Subscribe to the full turnSummaries Map. This intentionally uses a broad
  // selector (not a focused .get(id)) because the summary for a completed turn
  // arrives asynchronously after the turn ends. We need the Map update to
  // trigger a re-render so the append logic runs. Low-impact: entries are added
  // only when a turn completes (~once per 10-60s).
  const turnSummaries = useAppStore((s) => s.turnSummaries);

  // Coordinated session/mode reset. The store bumps `liteScrollbackClearToken`
  // when (a) the user runs /chat new or /chat <id>, (b) they swap from lite
  // to tui. Both cases require the TUI's module-level singletons below to
  // start empty — otherwise the staticItemsRef.current points at an array
  // already filled with the prior session/mode's rows, and twinki's
  // monotonic <Static> cursor has already advanced past those indices.
  // Newly appended rows would then land at indices the cursor has already
  // skipped past, which is what made messages "flash and disappear" or
  // never reach scrollback during mode swaps.
  //
  // Why this runs in the render body (not a useEffect): the singletons feed
  // refs created on the next two lines (staticItemsRef, emittedIdsRef,
  // flushedRef). If we wiped them in an effect, the FIRST render after a
  // bump would already have committed appends against the stale arrays
  // and called twinki's writeStaticLines with cross-mode rows. By the time
  // the effect fired we'd be undoing damage. Synchronous wipe = safe.
  //
  // Token is read with a single store subscription so React re-renders this
  // component when the store dispatches the bump.
  const clearToken = useAppStore((s) => s.liteScrollbackClearToken);
  if (clearToken !== _lastObservedClearToken) {
    _lastObservedClearToken = clearToken;
    // Mutate in place — refs declared below already point at these arrays.
    _staticItems.length = 0;
    _emittedIds.clear();
    _flushedMap.clear();
    // Reset the "have we ever seen messages" trackers so the post-clear
    // welcome path treats this like a fresh session.
    _hadMessages = false;
    _hadUserMessage = false;
    _welcomeInStatic = false;
    // Wipe the visible terminal + scrollback + reset twinki's monotonic
    // write cursor. Mirrors LiteLayout's clear effect verbatim — the
    // sequence below makes twinki's stdout interceptor invoke
    // handleExternalClear(), which drops accumulatedStaticOutput too.
    process.stdout.write('\x1b[3J\x1b[H\x1b[2J');
    adjustStaticCursor?.(Number.MAX_SAFE_INTEGER);
  }

  const greetingEnabled =
    settings !== null && settings[Settings.CHAT_GREETING_ENABLED] !== false;

  // Rotating startup tip (surface-aware — see tips/tips.ts). Picked once per
  // mount via useMemo so it doesn't reshuffle on re-render. Passed to the
  // <Static> welcome render too, so it persists into scrollback past the first
  // message.
  const tipRecommendLiteUi = useAppStore((s) => s.recommendLiteUi);
  const tipEngine = useAppStore((s) => s.agentEngine);
  const welcomeTip = useMemo(
    () =>
      pickTip({
        surface: 'tui',
        engine: tipEngine,
        recommendLiteUi: tipRecommendLiteUi,
      }),
    [tipRecommendLiteUi, tipEngine]
  );
  // Hold the welcome screen for a cloud session until the session is actually
  // created and linked; on a connect/createSession failure it must not render.
  // `session_create` reaches 'ready' only on success (and 'failed' on error),
  // so it is the reliable gate even when KAS pushes no live roster status.
  // Local sessions are unaffected — `welcomeAllowed` is always true for them.
  const cloudSessionActive = useAppStore((s) => s.cloudSessionActive);
  const cloudSessionCreated = useAppStore(
    (s) => s.bootProgress.get('session_create')?.status === 'ready'
  );
  const welcomeAllowed = !cloudSessionActive || cloudSessionCreated;

  // Track if we've ever had messages (to know if this is initial load or post-clear)
  const hadMessagesRef = React.useRef(_hadMessages);
  // Track if welcome was already added to Static
  const welcomeInStaticRef = React.useRef(_welcomeInStatic);
  const [flushTurnId, setFlushTurnId] = React.useState<string | undefined>(
    undefined
  );
  const activeTurnIdRef = React.useRef<string | undefined>(undefined);

  const handleReadyToFlush = React.useCallback(() => {
    setFlushTurnId(activeTurnIdRef.current);
  }, []);
  // Per-turn set of message IDs already flushed to <Static>
  const flushedRef = React.useRef(_flushedMap);
  // Persistent, append-only array of static items — never shrinks.
  // <Static> uses array length as its index, so items must stay at stable positions.
  const staticItemsRef = React.useRef(_staticItems);
  const emittedIdsRef = React.useRef(_emittedIds);
  const prevStaticLenRef = React.useRef(0);
  const staticItemsSnapshotRef = React.useRef<StaticItem[]>([]);

  const liteScrollbackClearToken = useAppStore(
    (s) => s.liteScrollbackClearToken
  );
  let resetStaticThisRender = false;
  if (liteScrollbackClearToken !== _lastObservedClearToken) {
    const isInitialColdTuiObservation =
      _lastObservedClearToken === -1 && liteScrollbackClearToken === 0;
    _lastObservedClearToken = liteScrollbackClearToken;
    if (!isInitialColdTuiObservation) {
      resetStaticThisRender = true;
      adjustStaticCursor?.(Number.MAX_SAFE_INTEGER);
      // Twinki observes CSI 2J and drops its accumulated static prefix without
      // erasing terminal scrollback, so destination-mode history is not duplicated.
      process.stdout.write(CLEAR_SCREEN_AND_HOME);
      staticItemsRef.current.length = 0;
      emittedIdsRef.current.clear();
      flushedRef.current.clear();
      staticItemsSnapshotRef.current = [];
      prevStaticLenRef.current = -1;
      hadMessagesRef.current = false;
      welcomeInStaticRef.current = false;
      _welcomeInStatic = false;
      _hadMessages = false;
      _hadUserMessage = false;
    }
  }

  if (messages.length > 0) {
    hadMessagesRef.current = true;
    _hadMessages = true;
  }
  if (!_hadUserMessage && messages.some((m) => m.role === MessageRole.User)) {
    _hadUserMessage = true;
  }

  // Only animate the wordmark on the very first mount ever, not on remounts
  const shouldAnimate = !_hasAnimated;
  if (shouldAnimate) _hasAnimated = true;

  const hasMessages = messages.length > 0;
  const isInitialLoad = !hasMessages && !hadMessagesRef.current;

  const conversationMessages = useMemo(() => {
    const conv: StoreMessageType[] = [];
    messages.forEach((msg) => {
      if (msg.role !== MessageRole.System) {
        conv.push(msg);
      }
    });
    return conv;
  }, [messages]);

  // Incremental turn reconstruction: during streaming, only the active
  // turn grows (new AI messages appended). Completed turns are stable.
  // Cache them and only rebuild the active turn's tail on each flush.
  const turnCacheRef = React.useRef<{
    completedTurns: ConversationTurn[];
    activeTurn: ConversationTurn | undefined;
    // Index into conversationMessages where the active turn starts
    activeTurnStart: number;
    // Last known User message id that started the active turn
    activeTurnUserId: string | undefined;
    // Total message count at last full rebuild
    lastFullRebuildLength: number;
  }>({
    completedTurns: [],
    activeTurn: undefined,
    activeTurnStart: 0,
    activeTurnUserId: undefined,
    lastFullRebuildLength: 0,
  });

  const {
    completedTurns: groupedCompletedTurns,
    activeTurn: groupedActiveTurn,
  } = useMemo(() => {
    const cache = turnCacheRef.current;
    const msgs = conversationMessages;

    // Find the last User *prompt* to detect if a new turn started. Steered
    // (mid-turn injected) user messages don't anchor a turn — they belong to
    // the prompt already in flight — so skip them. Keeping the anchor on the
    // prompt also means consuming a steer stays on the fast path (the steer is
    // picked up by the slice below) instead of forcing a full rebuild.
    let lastUserIdx = -1;
    let lastUserMsgId: string | undefined;
    for (let i = msgs.length - 1; i >= 0; i--) {
      const m = msgs[i];
      if (
        m?.role === MessageRole.User &&
        (m as { steered?: boolean }).steered !== true
      ) {
        lastUserIdx = i;
        lastUserMsgId = m.id;
        break;
      }
    }

    // Fast path: same active turn, just more AI messages appended.
    // Reuse cached completed turns and rebuild only the active turn.
    const sameTurn =
      msgs.length >= cache.lastFullRebuildLength &&
      lastUserMsgId !== undefined &&
      lastUserMsgId === cache.activeTurnUserId;

    if (sameTurn && cache.completedTurns.length > 0) {
      const userMsg = msgs[cache.activeTurnStart]!;
      const aiMessages = msgs.slice(cache.activeTurnStart + 1);
      const activeTurn: ConversationTurn = {
        userMessage: userMsg,
        aiMessages,
        isActive: true,
      };
      cache.activeTurn = activeTurn;
      return { completedTurns: cache.completedTurns, activeTurn };
    }

    // Full rebuild: new turn started, messages were cleared, or first render.
    // Steered (mid-turn injected) user messages are folded into their turn's
    // body rather than opening new turns — see groupMessagesIntoTurns.
    const t = groupMessagesIntoTurns(msgs);

    const completed = t.filter((turn) => !turn.isActive);
    const active = t.find((turn) => turn.isActive);

    // Update cache
    cache.completedTurns = completed;
    cache.activeTurn = active;
    cache.activeTurnStart = lastUserIdx >= 0 ? lastUserIdx : 0;
    cache.activeTurnUserId = lastUserMsgId;
    cache.lastFullRebuildLength = msgs.length;

    return { completedTurns: completed, activeTurn: active };
  }, [conversationMessages]);

  const messageIndexById = new Map(
    messages.map((message, index) => [message.id, index])
  );
  const nextPromptIndexAfter = (startIndex: number): number | undefined => {
    for (let index = startIndex + 1; index < messages.length; index++) {
      const message = messages[index];
      if (
        message?.role === MessageRole.User &&
        (message as { steered?: boolean }).steered !== true
      ) {
        return index;
      }
    }
    return undefined;
  };
  const includeInterleavedSystemRows = (
    turn: ConversationTurn
  ): ConversationTurn => {
    const startIndex = messageIndexById.get(turn.userMessage.id);
    if (startIndex === undefined) return turn;

    const turnBodyIds = new Set(turn.aiMessages.map((message) => message.id));
    const nextPromptIndex = nextPromptIndexAfter(startIndex);
    const endIndex = turn.isActive
      ? messages.length - 1
      : nextPromptIndex === undefined
        ? messages.length - 1
        : nextPromptIndex - 1;
    const hasLaterTurnBody = (index: number): boolean => {
      for (let laterIndex = index + 1; laterIndex <= endIndex; laterIndex++) {
        const laterMessage = messages[laterIndex];
        if (laterMessage && turnBodyIds.has(laterMessage.id)) return true;
      }
      return false;
    };

    let sawSystemRow = false;
    const orderedBody: StoreMessageType[] = [];
    for (let index = startIndex + 1; index <= endIndex; index++) {
      const message = messages[index];
      if (!message) continue;
      if (
        message.role === MessageRole.System &&
        ((message as { turnOwned?: boolean }).turnOwned === true ||
          hasLaterTurnBody(index))
      ) {
        sawSystemRow = true;
        orderedBody.push(message);
      } else if (turnBodyIds.has(message.id)) {
        orderedBody.push(message);
      }
    }

    return sawSystemRow ? { ...turn, aiMessages: orderedBody } : turn;
  };
  const completedTurnsWithSystems = groupedCompletedTurns.map(
    includeInterleavedSystemRows
  );
  const groupedActiveTurnWithSystems = groupedActiveTurn
    ? includeInterleavedSystemRows(groupedActiveTurn)
    : undefined;

  const replayIdleActiveTurn =
    resetStaticThisRender &&
    !isProcessing &&
    groupedActiveTurnWithSystems !== undefined;
  const completedTurns =
    replayIdleActiveTurn && groupedActiveTurnWithSystems
      ? [
          ...completedTurnsWithSystems,
          { ...groupedActiveTurnWithSystems, isActive: false },
        ]
      : completedTurnsWithSystems;
  const activeTurnCommittedAsTurn =
    !isProcessing &&
    groupedActiveTurnWithSystems !== undefined &&
    staticItemsRef.current.some(
      (item) =>
        item.type === 'turn' &&
        item.id === groupedActiveTurnWithSystems.userMessage.id
    );
  const activeTurn =
    replayIdleActiveTurn || activeTurnCommittedAsTurn
      ? undefined
      : groupedActiveTurnWithSystems;

  // Reset tailOverride when active turn changes (new user message)
  const activeTurnId = activeTurn?.userMessage.id;
  activeTurnIdRef.current = activeTurnId;
  const effectiveTailOverride = activeTurnId === flushTurnId ? 0 : null;

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
  const flushSet = computeFlushSet(
    activeAllMessages,
    isProcessing,
    effectiveTailOverride ?? TAIL_SIZE
  );
  const toFlush = activeAllMessages.filter((msg) => flushSet.has(msg.id));
  const tailMessages = activeAllMessages.filter((msg) => !flushSet.has(msg.id));

  // Role of the message just before the tail — needed for spacing logic
  const prevFlushedRole =
    toFlush.length > 0 ? toFlush[toFlush.length - 1]?.role : undefined;

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
  const emittedIds = emittedIdsRef.current;

  const appendStatic = (item: StaticItem) => {
    if (!emittedIds.has(item.id)) {
      emittedIds.add(item.id);
      staticItemsRef.current.push(item);
    }
  };

  // Welcome screen — emitted once when messages first appear
  if (
    hasMessages &&
    !welcomeInStaticRef.current &&
    greetingEnabled &&
    welcomeAllowed
  ) {
    welcomeInStaticRef.current = true;
    _welcomeInStatic = true;
    appendStatic({ type: 'welcome', id: '__welcome__' });
  }

  // Completed turns that were never incrementally flushed → StaticTurnCard.
  // Turns that WERE incrementally flushed → append only the unflushed tail messages.
  const turnOwnedSystemIds = new Set<string>();
  const trackTurnOwnedSystemRows = (turn: ConversationTurn | undefined) => {
    turn?.aiMessages.forEach((message) => {
      if (message.role === MessageRole.System) {
        turnOwnedSystemIds.add(message.id);
      }
    });
  };
  completedTurns.forEach(trackTurnOwnedSystemRows);
  trackTurnOwnedSystemRows(activeTurn);
  const completedTurnByAnchorId = new Map(
    completedTurns.map((turn) => [turn.userMessage.id, turn])
  );
  const appendCompletedTurn = (turn: ConversationTurn) => {
    const flushedIds = flushedRef.current.get(turn.userMessage.id);
    if (!flushedIds || flushedIds.size === 0) {
      appendStatic({ type: 'turn', id: turn.userMessage.id, turn });
      turn.aiMessages.forEach((message) => {
        if (message.role === MessageRole.System) emittedIds.add(message.id);
      });
    } else {
      const agentName =
        'agentName' in turn.userMessage
          ? turn.userMessage.agentName
          : undefined;
      const barColor = agentName
        ? getAgentColor(agentName, getColor).hex
        : undefined;
      const allMsgs = [turn.userMessage, ...turn.aiMessages];
      const tailMsgs = allMsgs.filter((msg) => !flushedIds.has(msg.id));
      const flushedMsgs = allMsgs.filter((msg) => flushedIds.has(msg.id));
      const lastFlushedRole =
        flushedMsgs.length > 0
          ? flushedMsgs[flushedMsgs.length - 1]?.role
          : undefined;
      appendMessagesToStatic(tailMsgs, barColor, appendStatic, {
        isLast: (i) => i === tailMsgs.length - 1,
        fallbackPrevRole: lastFlushedRole,
        mainAgentName: agentName,
      });
      tailMsgs.forEach((msg) => flushedIds.add(msg.id));
      // Append turn summary so it survives the transition to <Static>
      const summaryText = turnSummaries.get(turn.userMessage.id);
      if (summaryText) {
        appendStatic({
          type: 'summary',
          id: `${turn.userMessage.id}__summary`,
          text: summaryText,
        });
      }
    }
  };

  messages.forEach((msg) => {
    if (msg.role === MessageRole.System) {
      if (!turnOwnedSystemIds.has(msg.id)) {
        appendStatic({ type: 'system', id: msg.id, message: msg });
      }
      return;
    }

    const completedTurn = completedTurnByAnchorId.get(msg.id);
    if (completedTurn) appendCompletedTurn(completedTurn);
  });

  // Active turn: append divider + newly-flushed messages
  if (activeTurn && newlyFlushed.length > 0) {
    const turnId = activeTurn.userMessage.id;
    const flushedIds = flushedRef.current.get(turnId)!;
    if (flushedIds.size === newlyFlushed.length) {
      appendStatic({ type: 'divider', id: `${turnId}__divider` });
    }
    // Find the role of the message just before the first newly-flushed message
    const firstNewIdx = activeAllMessages.findIndex(
      (m) => m.id === newlyFlushed[0]?.id
    );
    const prevOfFirstNew =
      firstNewIdx > 0 ? activeAllMessages[firstNewIdx - 1]?.role : undefined;

    appendMessagesToStatic(newlyFlushed, activeAgentBarColor, appendStatic, {
      fallbackPrevRole: prevOfFirstNew,
      mainAgentName: activeAgentName,
    });
  }

  // Cap static items to bound accumulated Yoga nodes.
  // When items are spliced from the front, the renderer's monotonic write
  // cursor must be adjusted down by the same amount — otherwise new items
  // are silently skipped because slice(cursor) on a shorter array is empty.
  const trimmed = trimStaticItems(
    staticItemsRef.current,
    emittedIdsRef.current
  );
  if (trimmed > 0) {
    adjustStaticCursor?.(trimmed);
  }

  // Only create a new array ref when items were actually added, so <Static>'s
  // useMemo([items]) fires only when needed — not on every render.
  if (staticItemsRef.current.length !== prevStaticLenRef.current) {
    prevStaticLenRef.current = staticItemsRef.current.length;
    staticItemsSnapshotRef.current = [...staticItemsRef.current];
  }
  const staticItems = staticItemsSnapshotRef.current;

  return (
    <Box flexDirection="column">
      {isInitialLoad && greetingEnabled && welcomeAllowed && (
        <Box marginBottom={1}>
          <WelcomeScreen
            agent="kiro"
            mcpServers={[]}
            animate={shouldAnimate}
            tip={welcomeTip}
          />
        </Box>
      )}
      {/* Terminal banner: display-only env var message at session start */}
      {isInitialLoad && process.env.ASBX_KIRO_TERMINAL_BANNER && (
        <Box marginY={1}>
          <StatusBar status="info">
            <Text>{process.env.ASBX_KIRO_TERMINAL_BANNER}</Text>
          </StatusBar>
        </Box>
      )}
      {/* Announcement: dynamic (Ctrl+O works) until first user message, then unmounted */}
      {!_hadUserMessage && <WelcomeMessageBar />}

      {staticItems.length > 0 && (
        <Static items={staticItems}>
          {(item) => {
            if (item.type === 'welcome') {
              return (
                <Box key={item.id} flexDirection="column">
                  <Box marginBottom={1}>
                    <WelcomeScreen
                      agent="kiro"
                      mcpServers={[]}
                      animate={false}
                      tip={welcomeTip}
                    />
                  </Box>
                  {process.env.ASBX_KIRO_TERMINAL_BANNER && (
                    <Box marginY={1}>
                      <StatusBar status="info">
                        <Text>{process.env.ASBX_KIRO_TERMINAL_BANNER}</Text>
                      </StatusBar>
                    </Box>
                  )}
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
                    prevRole={item.prevRole}
                    mainAgentName={item.mainAgentName}
                  />
                </Box>
              );
            }
            if (item.type === 'summary') {
              return (
                <Box key={item.id} flexDirection="column">
                  <InkText> </InkText>
                  <TurnUsageSummary text={item.text} />
                </Box>
              );
            }
            return null;
          }}
        </Static>
      )}

      {/* Active turn tail: last TAIL_SIZE messages, wrapped in CardContext for the left bar.
          No Card/Divider — those are already in <Static> once flushing starts.
          Before any flushing (short turns), use full Card for correct divider. */}
      {activeTurn &&
        tailMessages.length > 0 &&
        (flushedRef.current.has(activeTurn.userMessage.id) ? (
          <CardContext.Provider value={{ active: true }}>
            <Box flexDirection="column" width="100%">
              <ActiveTurnTail
                tailMessages={tailMessages}
                agentBarColor={activeAgentBarColor}
                mainAgentName={activeAgentName}
                onReadyToFlush={handleReadyToFlush}
                prevFlushedRole={prevFlushedRole}
                turnId={activeTurn.userMessage.id}
              />
            </Box>
          </CardContext.Provider>
        ) : (
          <Box marginBottom={0}>
            <Card active={true}>
              <ActiveTurnTail
                tailMessages={tailMessages}
                agentBarColor={activeAgentBarColor}
                mainAgentName={activeAgentName}
                onReadyToFlush={handleReadyToFlush}
                prevFlushedRole={prevFlushedRole}
                turnId={activeTurn.userMessage.id}
              />
            </Card>
          </Box>
        ))}
    </Box>
  );
});
