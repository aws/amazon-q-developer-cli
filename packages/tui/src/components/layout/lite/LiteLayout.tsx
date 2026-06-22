/**
 * LiteLayout: minimal append-only terminal UI with a pinned footer. <Static>
 * holds finalized scrollback; the live region holds streaming + footer; shared
 * PromptInput + CommandMenu (rendered below, matching TUI).
 *
 * Append-only contract: each `staticItems` entry has a stable id and text
 * computed once. <Static> is a monotonic by-index cursor — an in-place edit to
 * an existing line is dropped silently. So we (1) hold a tool-call batch out of
 * static until the whole run settles (rendered in <LiteLiveRegion> meanwhile),
 * and (2) bake any leading `'\n'` separator into the text at flush time, never
 * as a function of neighbors at render time.
 */
import React, {
  useCallback,
  useContext,
  useMemo,
  useState,
  useEffect,
  useLayoutEffect,
  useRef,
} from 'react';
import { Box, Text, Static } from '../../../renderer.js';
import { useTwinkiContext } from 'twinki';
import {
  useAppStore,
  AppStoreContext,
  MessageRole,
  type MessageType,
} from '../../../stores/app-store.js';
import { LiteLiveRegion } from './LiteLiveRegion.js';
import { LiteSubagentPanel } from './LiteSubagentPanel.js';
import { LiteTaskTray } from './LiteTaskTray.js';
import {
  computeActiveToolBatchIds,
  formatTurnSummaryRow,
  needsLeadingBlank,
  selectStaticEligible,
} from './static-flush.js';
import { previewLine } from './queue-preview.js';
import { renderMessageToText, buildRenderTheme } from '../../../lite/render.js';
import { getVerboseDisplay } from '../../../lite/verbose.js';
import { pickTip, formatTipLine } from '../../../lite/tips.js';
import { ApprovalPrompt } from './ApprovalPrompt.js';
import {
  formatSubagentRow,
  extractFooterToolDetail,
  type SubagentRow,
} from './SubagentFooter.js';
import { shouldCancelApprovalForKilledStage } from './subagent-kill.js';
import { sessionConversationsStore } from '../../../stores/session-conversations.js';
import { renderPendingAgent } from './ConnectingPanel.js';
import {
  selectBootIndicatorPhase,
  formatBootIndicator,
} from './boot-indicator.js';
import { getCliVersion } from '../../../utils/version.js';
import { getGitBranch, getGitBranchAsync } from '../../../utils/git.js';
import { PromptInput } from '../../chat/prompt-bar/PromptInput.js';
import { CommandMenu } from '../../ui/CommandMenu.js';
import { Divider } from '../../ui/divider/Divider.js';
import { useKeypress } from '../../../hooks/useKeypress.js';
import { useTheme } from '../../../hooks/useThemeContext.js';
import {
  useGlyphs,
  useSpinners,
  useAllowAsciiArt,
} from '../../../hooks/useGlyphs.js';
import { useAnimationPaused } from '../../../contexts/AnimationPausedContext.js';
import { getAgentColor } from '../../../utils/agentColors.js';
import { isParentSubagentTool } from '../../../types/agent-events.js';
import { usePendingSwap } from './usePendingSwap.js';
import { logger } from '../../../utils/logger.js';
import chalk from 'chalk';
import { BackendPanels } from '../shared/BackendPanels.js';
import { useBackendPanelHandlers } from '../shared/useBackendPanelHandlers.js';
import { ArtifactGenerationCard } from '../../ui/ArtifactView/ArtifactGenerationCard.js';
import { SurveyPromptBar } from '../../ui/SurveyPromptBar.js';
import { useUIState } from '../../../stores/selectors.js';
import { formatEffort } from '../../../utils/string.js';
import { packStatusSegments } from './status-segments.js';

const TRIGGER_RULES = [
  { key: '/', type: 'start' as const },
  { key: '@', type: 'inline' as const },
];

const APPROVAL_IDLE_MS = 2000;

// Last `liteScrollbackClearToken` observed. MODULE-LEVEL (not a per-mount ref)
// so the clear-token reset block below survives the bare unmount/remount that
// `mode` toggles (Ctrl+G crew monitor, expanded, session-view) cause WITHOUT a
// real reset: the twinki bridge's monotonic cursor + accumulatedStaticOutput
// persist across that cycle, so re-running the reset would duplicate or swallow
// scrollback. Only resetMessages/setUiMode bump the token. ConversationView
// uses the same `_lastObservedClearToken` pattern. -1 fires it once on first
// mount of the process.
let _liteLastObservedClearToken = -1;

export const LiteLayout: React.FC = () => {
  const store = useContext(AppStoreContext);
  const messages = useAppStore((s) => s.messages);
  const isProcessing = useAppStore((s) => s.isProcessing);
  // Shell-escape (`!command`) flag: in-flight turn is a PTY-backed bash
  // command, not agent inference. Purely visual mode here — swaps the prompt
  // glyph and suppresses agent-mode chrome while the user interacts with bash
  // (keystroke→PTY forwarding lives in AppContainer's always-armed handler).
  const isShellEscape = useAppStore((s) => s.isShellEscape);
  // Render-skip bookmark (full contract in app-store): tui→lite swap sets it
  // to messages.length so TUI scrollback isn't re-rendered in lite; resume
  // sets it to max(0, len - LITE_HISTORY_RENDER_CAP). 0 on cold boot.
  const liteStaticSkipBefore = useAppStore((s) => s.liteStaticSkipBefore);
  const isInitialized = useAppStore((s) => s.isInitialized);
  const agentError = useAppStore((s) => s.agentError);
  const handleUserInput = useAppStore((s) => s.handleUserInput);
  const pendingApproval = useAppStore((s) => s.pendingApproval);
  const respondToApproval = useAppStore((s) => s.respondToApproval);
  const currentModel = useAppStore((s) => s.currentModel);
  const currentAgent = useAppStore((s) => s.currentAgent);
  const contextUsagePercent = useAppStore((s) => s.contextUsagePercent);
  const turnSummaries = useAppStore((s) => s.turnSummaries);
  const queuedMessages = useAppStore((s) => s.queuedMessages);
  const editingQueueIndex = useAppStore((s) => s.editingQueueIndex);
  // Tasks tray (Ctrl+X) — same store data the TUI's <ActivityTray /> consumes.
  const tasks = useAppStore((s) => s.tasks);
  const toggleActivityTray = useAppStore((s) => s.toggleActivityTray);
  const setActiveTrigger = useAppStore((s) => s.setActiveTrigger);
  const activeTrigger = useAppStore((s) => s.activeTrigger);
  const activeCommand = useAppStore((s) => s.activeCommand);
  const setActiveCommand = useAppStore((s) => s.setActiveCommand);
  const clearCommandInput = useAppStore((s) => s.clearCommandInput);
  const uiMode = useAppStore((s) => s.uiMode);
  const queuedInputRestore = useAppStore((s) => s.queuedInputRestore);
  const applyQueuedInputRestore = useAppStore((s) => s.applyQueuedInputRestore);
  const mcpInitStatus = useAppStore((s) => s.mcpInitStatus);
  const bootProgress = useAppStore((s) => s.bootProgress);
  const cancelMessage = useAppStore((s) => s.cancelMessage);
  const resetExitSequence = useAppStore((s) => s.resetExitSequence);
  const wasCancelled = useAppStore((s) => s.wasCancelled);
  const exitSequence = useAppStore((s) => s.exitSequence);
  const transientAlert = useAppStore((s) => s.transientAlert);
  const dismissTransientAlert = useAppStore((s) => s.dismissTransientAlert);
  const loadingMessage = useAppStore((s) => s.loadingMessage);
  const { getColor, getUserPromptColor, getUserPromptBgHex } = useTheme();
  // Accessibility wiring (1:1 with modern TUI): glyph/spinner Unicode↔ASCII,
  // ASCII-art banner gate, and animation-paused (skips the boot-frame interval
  // so motion-sensitive setups don't see the spinner cycle).
  const glyphs = useGlyphs();
  const spinners = useSpinners();
  const { allowAsciiArt } = useAllowAsciiArt();
  const animationPaused = useAnimationPaused();

  // Panel show-flags (flipped by backend slash commands via commands/effects).
  // Only needed here to build anyPanelOpen, which drives the input-area swap
  // and gates the always-armed Esc/Ctrl+C handler so Esc closing a panel
  // doesn't also fire cancelMessage.
  const {
    showContextBreakdown,
    showHelpPanel,
    showUsagePanel,
    showMcpPanel,
    showToolsPanel,
    showStatsPanel,
    showHooksPanel,
    showKnowledgePanel,
    showCodePanel,
    showChangelogPanel,
    showRewindExplorer,
    showKeybindingsPanel,
    showDisplaySettingsPanel,
    showThemePanel,
    showSettingsPanel,
    artifactViewOpen,
  } = useUIState();
  const showSurveyPanel = useAppStore((s) => s.showSurveyPanel);
  const surveyPrompt = useAppStore((s) => s.surveyPrompt);
  const dismissSurveyPrompt = useAppStore((s) => s.dismissSurveyPrompt);
  const currentEffort = useAppStore((s) => s.currentEffort);
  // Goal-loop state (set by `/goal`). Lite surfaces it three ways: a
  // status-line segment, a one-time scrollback confirmation, and a transient
  // alert on bare `/goal` (which has no lite panel).
  const goalStatus = useAppStore((s) => s.goalStatus);
  const setShowGoalPanel = useAppStore((s) => s.setShowGoalPanel);
  const showTransientAlert = useAppStore((s) => s.showTransientAlert);

  const handlers = useBackendPanelHandlers();

  const anyPanelOpen =
    showContextBreakdown ||
    showHelpPanel ||
    showUsagePanel ||
    showMcpPanel ||
    showToolsPanel ||
    showStatsPanel ||
    showHooksPanel ||
    showKnowledgePanel ||
    showCodePanel ||
    showChangelogPanel ||
    showRewindExplorer ||
    showKeybindingsPanel ||
    showDisplaySettingsPanel ||
    showThemePanel ||
    showSettingsPanel ||
    !!artifactViewOpen ||
    showSurveyPanel;

  // The lite /verbosity menu renders via <CommandMenu> (not a backend panel)
  // for its live preview + truncation editor, but presents like other
  // settings: a breadcrumb header occupies the (hidden) input row. Lite-only;
  // TUI filters /verbosity out of the menu.
  const verbosityMenuActive =
    uiMode === 'lite' && activeCommand?.command.name === '/verbosity';

  const pendingSwap = usePendingSwap();
  const pendingAgentName = pendingSwap?.name ?? null;

  // Subagent inline-trace panel (Ctrl+O). subagentOpenIndex = which subagent
  // in `activeSubagents` is inspected; null = closed. The open boolean is
  // mirrored into app-store so AppContainer's dispatch stops Esc from also
  // firing a stream cancel.
  const sessions = useAppStore((s) => s.sessions);
  const setSubagentPanelOpen = useAppStore((s) => s.setSubagentPanelOpen);
  const [subagentOpenIndex, setSubagentOpenIndex] = useState<number | null>(
    null
  );
  const [subagentScrollOffset, setSubagentScrollOffset] = useState(0);
  // Git branch in the status footer. Sync at mount for first paint, then async
  // on every turn boundary (isProcessing true→false) — catches branch changes
  // the agent just made. Async so a slow `git rev-parse` can't stall a render;
  // no mid-turn polling (footer is hidden while the live region paints).
  const [gitBranch, setGitBranch] = useState<string | null>(getGitBranch);
  const prevIsProcessingRef = useRef(isProcessing);
  useEffect(() => {
    const wasProcessing = prevIsProcessingRef.current;
    prevIsProcessingRef.current = isProcessing;
    if (!(wasProcessing && !isProcessing)) return;
    let cancelled = false;
    getGitBranchAsync().then((branch) => {
      if (!cancelled) setGitBranch(branch);
    });
    return () => {
      cancelled = true;
    };
  }, [isProcessing]);
  // Auto-follow: when true, the panel ignores scrollOffset and stays pinned
  // to the latest trace line. Disabled when the user scrolls up; re-enabled
  // when they scroll back to the floor (the panel reports totalLines via
  // onLinesChange so we know where the floor is).
  const [subagentFollowBottom, setSubagentFollowBottom] = useState(true);
  const [subagentTotalLines, setSubagentTotalLines] = useState(0);
  useEffect(() => {
    setSubagentPanelOpen(subagentOpenIndex != null);
  }, [subagentOpenIndex, setSubagentPanelOpen]);

  // Per-session kill ladder (mirrors CrewMonitorLayout): first Ctrl+X arms the
  // focused subagent's sessionId, second within 2s invokes terminateSession +
  // cleanup. State lives at the LAYOUT level (not LiteSubagentPanel) because
  // the kill side-effects need kiro, sessionConversationsStore, and
  // pendingApproval — all already here; the panel just gets `armedToKill`.
  const kiro = useAppStore((s) => s.kiro);
  const updateSession = useAppStore((s) => s.updateSession);
  const cleanupTerminatedSession = useAppStore(
    (s) => s.cleanupTerminatedSession
  );
  const cancelApproval = useAppStore((s) => s.cancelApproval);
  const [armedKillSessionId, setArmedKillSessionId] = useState<string | null>(
    null
  );
  const armedKillTimerRef = useRef<ReturnType<typeof setTimeout> | null>(null);
  // Disarm + clear timer (second press, 2s timeout, panel close, cycle).
  const disarmKill = useCallback(() => {
    if (armedKillTimerRef.current) {
      clearTimeout(armedKillTimerRef.current);
      armedKillTimerRef.current = null;
    }
    setArmedKillSessionId(null);
  }, []);
  // Clear timer on unmount so a delayed setTimeout can't setState after the
  // layout is swapped out (lite→tui swap mid-arm).
  useEffect(
    () => () => {
      if (armedKillTimerRef.current) clearTimeout(armedKillTimerRef.current);
    },
    []
  );

  // Always-armed Ctrl+C / Escape interrupt. AppContainer's handler ALSO calls
  // cancelMessage; the store's cancelMessage is idempotent so double-fire is
  // safe. Belt-and-suspenders: lite has missed this keypress at the wrong
  // moment before.
  const isProcessingRef = useRef(isProcessing);
  isProcessingRef.current = isProcessing;
  const pendingApprovalRef = useRef(pendingApproval);
  pendingApprovalRef.current = pendingApproval;
  const activeCommandRef = useRef(activeCommand);
  activeCommandRef.current = activeCommand;
  const activeTriggerRef = useRef(activeTrigger);
  activeTriggerRef.current = activeTrigger;
  // These refs mirror state the always-armed cancel handler must consult.
  // Twinki fires every active useKeypress on the same keystroke (no handler
  // order), so for each surface that owns Esc — subagent panel, backend
  // panel, queue-edit, /prompts detail — the handler must no-op Esc instead
  // of cancelling the in-flight turn. goalStatusRef lets the panel safety-net
  // timeout read the latest goal without re-arming on every tick.
  const subagentOpenIndexRef = useRef(subagentOpenIndex);
  subagentOpenIndexRef.current = subagentOpenIndex;
  const anyPanelOpenRef = useRef(anyPanelOpen);
  anyPanelOpenRef.current = anyPanelOpen;
  const goalStatusRef = useRef(goalStatus);
  goalStatusRef.current = goalStatus;
  const editingQueueIndexRef = useRef(editingQueueIndex);
  editingQueueIndexRef.current = editingQueueIndex;
  const promptDetailOpen = useAppStore((s) => s.promptDetailOpen);
  const promptDetailOpenRef = useRef(promptDetailOpen);
  promptDetailOpenRef.current = promptDetailOpen;

  useKeypress((input, key) => {
    // Subagent panel open: Esc + Ctrl+O belong to the panel. Ctrl+C still
    // cancels the agent (panel doesn't claim it) so users aren't trapped.
    if (subagentOpenIndexRef.current != null) {
      if (key.escape) return;
      if (key.ctrl && (input === 'o' || input === 'O')) return;
    }
    // Backend panel / queue-edit own Esc — bail before the cancel branch so
    // Esc doesn't also abort the in-flight turn.
    if (anyPanelOpenRef.current && key.escape) {
      return;
    }
    if (editingQueueIndexRef.current != null && key.escape) {
      return;
    }
    if ((key.ctrl && input === 'c') || key.escape) {
      logger.debug('[lite] interrupt key', {
        ctrl: !!key.ctrl,
        esc: !!key.escape,
        input,
        isProcessing: isProcessingRef.current,
        pendingApproval: !!pendingApprovalRef.current,
      });
      if (isProcessingRef.current && !pendingApprovalRef.current) {
        cancelMessage();
        return;
      }
    }
    // Ctrl+C inside any open menu surface = Esc. CommandMenu's own keypress
    // handler closes the menu (with /settings + /verbose return-on-escape
    // stash handling); our job here is to roll back AppContainer's
    // double-Ctrl+C exit increment — that handler runs on the same keystroke
    // and would otherwise tick toward `process.exit(0)` for a user just
    // backing out of a menu. queueMicrotask waits until after AppContainer
    // increments, then we reset to zero.
    const inMenu = activeCommandRef.current || activeTriggerRef.current;
    if (key.ctrl && input === 'c' && inMenu) {
      queueMicrotask(() => resetExitSequence());
      return;
    }
    if (key.escape && activeCommandRef.current) {
      // /prompts in detail view: PromptDetails handles its own Esc-back
      // (picker↔detail). Don't drop the whole overlay too.
      if (promptDetailOpenRef.current) return;
      setActiveCommand(null);
      clearCommandInput();
      return;
    }
    if (key.escape && activeTriggerRef.current) {
      clearCommandInput();
    }
  });

  // Ctrl+X — toggle the lite task tray (mirrors <ActivityTray />). Gated on
  // tasks.length > 0; bails for queue-edit / approval / panel / subagent
  // modes that own Ctrl+X. The editingQueueIndex bail mirrors PromptInput's
  // own Ctrl+X gate so they don't both fire.
  useKeypress((input, key) => {
    if (!(key.ctrl && (input === 'x' || input === 'X'))) return;
    if (tasks.length === 0) return;
    if (editingQueueIndexRef.current != null) return;
    if (pendingApprovalRef.current) return;
    if (anyPanelOpenRef.current) return;
    if (subagentOpenIndexRef.current != null) return;
    toggleActivityTray();
  });

  // Approval typing guard: defer showing approval until the user is idle for
  // APPROVAL_IDLE_MS, so an approval landing mid-typing doesn't intercept the
  // user's next char (often y/t/n) as a response. The tracker is always active
  // so pre-arrival keystrokes count. Once visible the prompt never re-hides on
  // keystroke — only the boundary effect below (fresh pendingApproval) hides it.
  const lastKeypressRef = useRef(0);
  const [approvalReady, setApprovalReady] = useState(true);
  const showApprovalRef = useRef(false);

  useKeypress(() => {
    // Skip while the prompt is visible — that keystroke is the y/n/t response;
    // counting it would delay each sequential approval by APPROVAL_IDLE_MS.
    if (showApprovalRef.current) return;
    lastKeypressRef.current = Date.now();
  });

  useEffect(() => {
    if (!pendingApproval) return;
    if (approvalReady) return;
    if (Date.now() - lastKeypressRef.current >= APPROVAL_IDLE_MS) {
      setApprovalReady(true);
      return;
    }
    const timer = setInterval(() => {
      if (Date.now() - lastKeypressRef.current >= APPROVAL_IDLE_MS) {
        setApprovalReady(true);
        clearInterval(timer);
      }
    }, 300);
    return () => clearInterval(timer);
  }, [pendingApproval, approvalReady]);

  useEffect(() => {
    if (!pendingApproval) {
      setApprovalReady(true);
      return;
    }
    if (Date.now() - lastKeypressRef.current < APPROVAL_IDLE_MS) {
      setApprovalReady(false);
    }
  }, [pendingApproval]);

  const showApproval = pendingApproval && approvalReady;
  showApprovalRef.current = !!showApproval;

  // Emit a "user interrupted" line once a cancelled turn has FULLY settled.
  // wasCancelled flips true the instant cancelMessage() starts, but appending
  // a System row while the agent stream is still unwinding would flush a
  // partial snapshot of the live agent text into <Static> (rendered twice).
  // Waiting for !isProcessing lets the agent message finalize first.
  const cancelArmedRef = useRef(false);
  useEffect(() => {
    if (wasCancelled) {
      cancelArmedRef.current = true;
      return;
    }
    cancelArmedRef.current = false;
  }, [wasCancelled]);
  useEffect(() => {
    if (!cancelArmedRef.current) return;
    if (isProcessing) return;
    if (!store) return;
    cancelArmedRef.current = false;
    store.setState((s) => ({
      messages: [
        ...s.messages,
        {
          id: crypto.randomUUID(),
          role: MessageRole.System,
          content: chalk.dim('user interrupted'),
          success: true,
        },
      ],
    }));
  }, [isProcessing, wasCancelled, store]);

  // Auto-dismiss the transient alert (autoHideMs from caller, 4s fallback so a
  // no-timer alert isn't stranded forever).
  useEffect(() => {
    if (!transientAlert) return;
    const ms = transientAlert.autoHideMs ?? 4000;
    const t = setTimeout(() => dismissTransientAlert(), ms);
    return () => clearTimeout(t);
  }, [transientAlert, dismissTransientAlert]);

  // Panel safety-net. Panel-type slash commands freeze the input by design
  // (activeCommand set with empty options → PromptInput bails). Lite renders
  // only a curated subset of panels; a command lite does NOT render (today
  // `/goal`) would otherwise freeze the input dead until the user mashes Esc.
  // Signal for "unrenderable panel": activeCommand set, empty options (pickers
  // like /model set non-empty), no panel open. The 600ms delay lets a real
  // panel open first (flipping anyPanelOpen re-runs this and clears the timer);
  // if it elapses with no panel, release the input.
  useEffect(() => {
    if (!activeCommand) return;
    if (activeCommand.options.length > 0) return; // selection picker — legit
    if (anyPanelOpen) return; // a backend panel is up — legit freeze
    const cmdName = activeCommand.command.name;
    const t = setTimeout(() => {
      if (
        !activeCommandRef.current ||
        activeCommandRef.current.options.length > 0 ||
        anyPanelOpenRef.current
      )
        return;
      // `/goal` has no lite panel — surface its status as a transient alert so
      // bare `/goal` still gives feedback. Drop the orphaned showGoalPanel flag
      // so a later lite→tui swap doesn't auto-open the modern GoalPanel.
      if (cmdName === '/goal') {
        const g = goalStatusRef.current;
        setShowGoalPanel(false);
        showTransientAlert({
          message: g
            ? `goal ${g.state} [${g.iteration + 1}/${g.maxIterations}]${g.message ? ` · ${g.message}` : ''}`
            : 'no active goal · use /goal <description> to set one',
          status: 'info',
          autoHideMs: 6000,
        });
      }
      setActiveCommand(null);
      clearCommandInput();
    }, 600);
    return () => clearTimeout(t);
  }, [
    activeCommand,
    anyPanelOpen,
    setActiveCommand,
    clearCommandInput,
    setShowGoalPanel,
    showTransientAlert,
  ]);

  // One-time scrollback confirmation when a NEW goal is set (lite has no goal
  // chip/panel). Dedup by goal text in a ref — goalStatus also fires every
  // iteration with the same message. Partial-snapshot guard (same class as the
  // cancel appender): defer the append past a live streaming Model row so we
  // don't flush a partial snapshot into <Static>.
  const announcedGoalRef = useRef<string | null>(null);
  useEffect(() => {
    const msg = goalStatus?.message ?? null;
    if (!goalStatus || !msg) {
      // Goal cleared — reset so the next goal set re-announces.
      if (!goalStatus) announcedGoalRef.current = null;
      return;
    }
    if (msg === announcedGoalRef.current) return; // already confirmed
    // Terminal states aren't a "set" — don't emit a confirmation for them.
    if (goalStatus.state === 'completed' || goalStatus.state === 'exhausted')
      return;
    if (!store) return;
    const tail = messages[messages.length - 1];
    if (
      isProcessing &&
      tail &&
      tail.role === MessageRole.Model &&
      !tail.standalone
    )
      return; // defer past a live streaming model row
    announcedGoalRef.current = msg;
    store.setState((s) => ({
      messages: [
        ...s.messages,
        {
          id: crypto.randomUUID(),
          role: MessageRole.System,
          content: chalk.dim(
            `goal set · ${msg} · looping up to ${goalStatus.maxIterations} iterations · /goal clear to cancel`
          ),
          success: true,
        },
      ],
    }));
  }, [goalStatus, messages, isProcessing, store]);

  // Boot indicator: one dim row near the status line surfacing in-flight async
  // setup (agent_connect > session_create > MCP aggregate), hidden the moment
  // nothing is 'loading'. Replaces a multi-line connecting panel that had two
  // bugs (outlived its grace window; a misleading per-stage timer). Failure
  // detail isn't lost — McpServerInitFailure fires a transient alert and /mcp
  // shows per-server status; this row only answers "anything still loading?".
  const showBootIndicator = useMemo(() => {
    for (const info of bootProgress.values()) {
      if (info.status === 'loading') return true;
    }
    for (const info of mcpInitStatus.values()) {
      if (info.status === 'loading') return true;
    }
    return false;
  }, [bootProgress, mcpInitStatus]);

  // Boot tick — cycles the spinner glyph (150ms, matches LiteLiveRegion). One
  // interval shared by the boot indicator + pending-agent footer chip.
  const [bootFrame, setBootFrame] = useState(0);
  useEffect(() => {
    if (!showBootIndicator && !pendingAgentName && !loadingMessage) return;
    // Animation-paused: hold the last frame instead of cycling.
    if (animationPaused) return;
    const t = setInterval(() => setBootFrame((f) => f + 1), 150);
    return () => clearInterval(t);
  }, [showBootIndicator, pendingAgentName, loadingMessage, animationPaused]);

  // MCP failures surface once via the McpServerInitFailure transient alert (NOT
  // also a scrollback line — that left a duplicate warning pinned forever).

  // KIRO welcome banner — a live-region <Text> while no messages sent yet AND
  // the banner hasn't shown this session. The agent's standalone greeting lands
  // in `messages` before user input, so it's excluded from the "still relevant"
  // check. Computed once per allowAsciiArt toggle so live-region re-renders
  // don't re-pick a tip. ASCII mode degrades to single-line "KIRO".
  //
  // `liteWelcomeEmitted`: cross-mount suppression flag, set true on unmount so
  // a lite→tui→lite swap doesn't re-flash the banner; resetMessages clears it.
  const liteWelcomeEmitted = useAppStore((s) => s.liteWelcomeEmitted);
  const setLiteWelcomeEmitted = useAppStore((s) => s.setLiteWelcomeEmitted);
  // Flip on UNMOUNT, not first paint: flipping mid-mount would re-run the
  // showWelcomeBanner memo and unmount the live banner immediately. On unmount,
  // only the NEXT remount sees the new value and skips the banner.
  useEffect(() => {
    return () => {
      setLiteWelcomeEmitted(true);
    };
  }, [setLiteWelcomeEmitted]);

  const welcomeBannerText = useMemo(() => {
    const version = getCliVersion();
    const brand = chalk.hex('#C19AFF');
    const kiroArt = allowAsciiArt
      ? brand(
          `  _  _____ ____   ___
 | |/ /_ _|  _ \\ / _ \\
 | ' / | || |_) | | | |
 | . \\ | ||  _ <| |_| |
 |_|\\_\\___|_| \\_\\\\___/`
        )
      : brand('  KIRO');
    // One rotating tip, picked deterministically per day (no restart flicker).
    const tipLine = formatTipLine(pickTip());
    return `${kiroArt}\n${chalk.dim(`  v${version} · lite`)}\n${tipLine}`;
  }, [allowAsciiArt]);
  // True until real chat content lands this session OR the banner already
  // emitted in a prior mount. "Real chat content" = any NON-standalone-greeting
  // message: gating on User rows alone is too narrow (a /agent or /model System
  // announcement arrives before the user types, the static memo anchors a
  // banner against it, and the still-mounted live banner showed a DUPLICATE).
  // Must mirror the visibleMessages filter exactly.
  const showWelcomeBanner = useMemo(
    () =>
      !liteWelcomeEmitted &&
      !messages.some((m) => !(m.role === MessageRole.Model && m.standalone)),
    [liteWelcomeEmitted, messages]
  );
  // Agent/model names for display
  const agentName = currentAgent?.name || null;
  const modelName = currentModel?.name || currentModel?.id || null;

  // Welcome-screen greeting. The agent's standalone greeting is filtered out of
  // the static-eligible set while the welcome screen is up (so the banner stays
  // anchored above it); render it inline here so the user sees it before their
  // first message. Once they type, the JSX gate unmounts this and the greeting
  // commits to <Static> via the eligible-set fallback.
  const welcomeGreetingText = useMemo(() => {
    if (!showWelcomeBanner) return '';
    // Walk from the tail so multi-swap sessions (pick A, then B, no typing)
    // show B's greeting — matching the agent name in the footer.
    let greeting: Extract<MessageType, { role: MessageRole.Model }> | null =
      null;
    for (let i = messages.length - 1; i >= 0; i--) {
      const m = messages[i]!;
      if (m.role === MessageRole.Model && m.standalone) {
        greeting = m;
        break;
      }
    }
    if (!greeting) return '';
    const theme = buildRenderTheme(
      getColor,
      getUserPromptColor,
      getUserPromptBgHex
    );
    const stageColor = (stageName: string) =>
      getAgentColor(stageName, getColor);
    return renderMessageToText(greeting, agentName ?? undefined, {
      pendingApprovalToolCallId: null,
      termCols: process.stdout.columns ?? 80,
      theme,
      glyphs,
      getStageInputColor: stageColor,
      getStageOutputColor: stageColor,
      getAgentTagColor: stageColor,
    });
  }, [
    showWelcomeBanner,
    messages,
    agentName,
    glyphs,
    getColor,
    getUserPromptColor,
    getUserPromptBgHex,
  ]);

  // Persistent <Static> items array, built incrementally — each render only
  // APPENDS, never mutates prior entries. Twinki's <Static> is a monotonic
  // by-index cursor that silently drops re-emissions for already-printed
  // indices, so the prefix is load-bearing forever. (A fresh array per chunk
  // was both wasteful and a Zustand-equality footgun.)
  const staticItemsRef = useRef<Array<{ id: string; text: string }>>([]);
  // High-water mark into `eligible`: count already appended. Resets on clear.
  const lastFlushedEligibleCountRef = useRef(0);
  // Session ids the user explicitly killed via Ctrl+X. Tracked locally because
  // the backend fires session_terminated for normal completion too, so a status
  // check alone would paint naturally-completed stages as red `✗ killed`.
  const userKilledSessionsRef = useRef<Set<string>>(new Set());
  // Ids already pushed to staticItemsRef. Belt-and-suspenders dedup: the
  // index-based delta walk assumes the eligible PREFIX is stable, but a Model
  // row can flip ineligible→eligible AFTER a later System/User row was flushed
  // (the shell-escape cancel race), re-pointing the walk at an already-pushed
  // row. Without this skip that row duplicates forever (static is append-only).
  const pushedStaticIdsRef = useRef<Set<string>>(new Set());
  // Turn-summary trailers already committed — never re-emit (preserves the
  // monotonic cursor across mode swaps + slash-command boundaries).
  const committedTurnSummariesRef = useRef<Set<string>>(new Set());
  // User id opening the in-flight (or last) turn; the prior turn's trailer is
  // emitted at this boundary.
  const openTurnUserIdRef = useRef<string | null>(null);
  // Last appended eligible msg — so the next delta can compute the leading
  // blank against it without re-walking the full eligible list.
  const lastAppendedEligibleMsgRef = useRef<MessageType | null>(null);

  // Session boundary (/chat new|<id>|load, /clear, /rewind, lite↔tui swap).
  // resetMessages/setUiMode bump liteScrollbackClearToken; on a bump (or the -1
  // first-mount init) we: (1) adjustStaticCursor(MAX) to drop the monotonic
  // cursor so the next paint lands at index 0; (2) reset staticItemsRef + the
  // bookkeeping refs; (3) push the KIRO banner at the head when there's prior
  // content (fresh sessions use the live-region banner instead so resize
  // reflows it).
  //
  // We deliberately do NOT write CSI 3J/2J — that wipes the whole terminal
  // scrollback including pre-kiro shell history. Treating /chat new like a
  // normal command (print below) preserves context; twinki's
  // accumulatedStaticOutput is 10k-line bounded so growth is safe.
  //
  // Runs in the render body (not an effect) because the staticItems memo reads
  // these refs synchronously during render — a post-commit reset would let the
  // first render after a bump re-commit the prior session's rows.
  // (The observed token is module-level for the bare-remount reason documented
  // on `_liteLastObservedClearToken`.) Cost: one integer compare per render.
  const liteScrollbackClearToken = useAppStore(
    (s) => s.liteScrollbackClearToken
  );
  const { adjustStaticCursor } = useTwinkiContext();
  if (liteScrollbackClearToken !== _liteLastObservedClearToken) {
    _liteLastObservedClearToken = liteScrollbackClearToken;
    // Snapshot "did this layout commit any <Static> rows in the prior session?"
    // BEFORE wiping. Decides swap-with-content (static anchor banner) vs truly
    // fresh (live banner). Without it, /chat new mid-session falls through both
    // gates (messages emptied → no User; live banner re-renders over preserved
    // scrollback) and the user sees KIRO art twice.
    const hadPriorStaticContent = staticItemsRef.current.length > 0;
    adjustStaticCursor?.(Number.MAX_SAFE_INTEGER);
    staticItemsRef.current = [];
    lastFlushedEligibleCountRef.current = 0;
    pushedStaticIdsRef.current = new Set();
    committedTurnSummariesRef.current = new Set();
    openTurnUserIdRef.current = null;
    lastAppendedEligibleMsgRef.current = null;
    // Two-armed gate for the swap-with-content session anchor banner. User-arm:
    // tui→lite mid-session + /chat <id> load (messages carries prior chat).
    // hadPriorStaticContent-arm: /chat new mid-session (resetMessages empties
    // messages BEFORE the bump). We deliberately DON'T broaden the User-arm to
    // all non-greeting roles — System slash-command announcements on an empty
    // session aren't chat and would re-emit the banner on every empty-session
    // swap (pinned by integ test `lite-welcome-banner-roundtrip`). The id
    // includes the clear-token so each swap gets a fresh monotonic id (a fixed
    // id would make the by-index cursor drop the second emission).
    if (
      hadPriorStaticContent ||
      messages.some((m) => m.role === MessageRole.User)
    ) {
      staticItemsRef.current.push({
        id: `lite-mode-banner-${liteScrollbackClearToken}`,
        text: welcomeBannerText,
      });
    }
  }

  // Cursor realignment on a real boundary is handled by the clear-token block
  // above; we don't realign on bare remounts (see _liteLastObservedClearToken).

  // Queue-drain input restore. A queued slash command that opens a picker
  // stashes the user's pre-drain input in `queuedInputRestore` (see app-store);
  // this fires on the picker-close edge (activeCommand non-null → null) and
  // applies the restore. Catches both close paths (Esc-dismiss and selection);
  // restoring on the local edge rather than awaiting the dispatch RPC shows the
  // text back immediately. useLayoutEffect (not useEffect) so it commits in the
  // same frame and the empty input never paints (visible flicker). The ref
  // gates to the non-null → null edge only.
  const prevActiveCommandRef = useRef(activeCommand);
  useLayoutEffect(() => {
    const prev = prevActiveCommandRef.current;
    prevActiveCommandRef.current = activeCommand;
    if (prev != null && activeCommand == null && queuedInputRestore != null) {
      applyQueuedInputRestore();
    }
  }, [activeCommand, queuedInputRestore, applyQueuedInputRestore]);

  const activeToolBatchIds = useMemo(
    () => computeActiveToolBatchIds(messages, agentName),
    [messages, agentName]
  );

  // Cheap gate: has any subagent tool ever appeared? Lets the per-message
  // subagent walks below short-circuit on the common no-pipeline path.
  const hasAnySubagentTool = useMemo(() => {
    for (const m of messages) {
      if (m.role === MessageRole.ToolUse && isParentSubagentTool(m.name))
        return true;
    }
    return false;
  }, [messages]);

  // Static items: welcome + finalized messages, append-only. Each item's text
  // is baked once at flush time; prior items are never touched, so <Static>'s
  // by-index cursor stays valid. Delta-append (not rebuild-per-render):
  // staticItemsRef is a persistent array, mutated in place, only appending rows
  // that joined `eligible` since the last walk.
  //
  // Resize caveat: this bakes `process.stdout.columns` into each row's text. If
  // the terminal resizes after a row lands, its text is stale — but the
  // monotonic cursor ignores re-emissions anyway. DO NOT add a resize redraw:
  // it would be silently dropped, or scramble history if Static ever honored
  // in-place edits. Terminal soft-wrap absorbs width changes for live writes.
  const staticItems = useMemo(() => {
    const items = staticItemsRef.current;

    // Welcome banner placement is hybrid: (1) the live-region <Text> below
    // (while showWelcomeBanner) handles the fresh-session resize case (a
    // static-only banner vanishes on resize since doResize drops
    // accumulatedStaticOutput); (2) the index-0 static push below (gated on
    // items.length === 0) persists it as a session delimiter once chat lands.
    // Both, because static-only regressed resize UX (b72df6cd8) and live-only
    // regressed scrollback persistence. Mutually exclusive with the
    // swap-with-content push above via the items.length === 0 gate.

    // Slice the tui→lite bookmark out before selecting eligible rows; slice
    // preserves the tail for the "skip last streaming Model" rule.
    // activeToolBatchIds is computed from FULL messages so the trailing batch
    // is right even across the bookmark. Welcome-screen suppression: while
    // showWelcomeBanner, drop standalone-greeting rows (rendered alongside the
    // live banner so the banner sits above them); they fall back into eligible
    // and commit to <Static> once the welcome screen ends.
    const sliced =
      liteStaticSkipBefore > 0 && liteStaticSkipBefore <= messages.length
        ? messages.slice(liteStaticSkipBefore)
        : messages;
    const visibleMessages = showWelcomeBanner
      ? sliced.filter((m) => !(m.role === MessageRole.Model && m.standalone))
      : sliced;
    // /verbosity showThinkingContent off → drop empty-content+thinking-only
    // Model rows at eligibility time, else their leading-blank prefix pins
    // phantom rows into <Static> on every Thought-only round (see static-flush).
    const hideThinkingContent =
      getVerboseDisplay().showThinkingContent === false;
    const eligible = selectStaticEligible(
      visibleMessages,
      isProcessing,
      activeToolBatchIds,
      agentName,
      hideThinkingContent
    );

    // Guard against `eligible` shrinking below the high-water mark. The
    // index-based delta walk assumes eligible only grows, but a message can
    // flip OUT of eligible after being counted (agentName reclassification,
    // cleared Model content, /verbose thinking toggle). Left above
    // eligible.length, the walk never runs and later rows are stranded
    // (disappear-until-reprompt). Clamp down so the next walk starts valid;
    // pushedStaticIdsRef dedups so re-walking emitted rows is a no-op. Warn on
    // the anomaly for a breadcrumb.
    if (lastFlushedEligibleCountRef.current > eligible.length) {
      logger.warn('[lite] static high-water above eligible length — clamping', {
        highWater: lastFlushedEligibleCountRef.current,
        eligible: eligible.length,
        messages: messages.length,
        agentName,
      });
      lastFlushedEligibleCountRef.current = eligible.length;
    }
    // Hot-path bypass: nothing new to flush AND no trailer ready — skip the
    // renderCtx / subagent walk / theme build (most spinner re-renders land here).
    const haveNewEligible =
      eligible.length > lastFlushedEligibleCountRef.current;
    const openTurnId = openTurnUserIdRef.current;
    const havePendingTrailerForOpenTurn =
      !isProcessing &&
      openTurnId != null &&
      turnSummaries.has(openTurnId) &&
      !committedTurnSummariesRef.current.has(openTurnId);
    if (!haveNewEligible && !havePendingTrailerForOpenTurn) {
      return items;
    }

    // First-content banner: anchor the welcome banner at index 0 of <Static>
    // the first time chat content lands (the live-region banner unmounts the
    // same render). The items.length === 0 gate keeps this exclusive with the
    // swap-with-content push above — one banner row per session start.
    if (items.length === 0) {
      items.push({
        id: '__lite_welcome__',
        text: welcomeBannerText,
      });
    }

    // Per-render theme + renderCtx. /theme re-runs this memo with new accessors
    // so future rows pick up the swap; already-flushed rows keep their frozen
    // text. Subagent walk gated on hasAnySubagentTool.
    const subagentSummariesById = new Map<
      string,
      Array<{ stageName: string; contextSummary: string; taskResult: string }>
    >();
    if (hasAnySubagentTool) {
      let activeParentId: string | null = null;
      for (const m of messages) {
        if (m.role !== MessageRole.ToolUse) continue;
        const isParentSubagent =
          isParentSubagentTool(m.name) &&
          (!m.agentName || m.agentName === agentName);
        if (isParentSubagent) {
          activeParentId = m.id;
          if (!subagentSummariesById.has(m.id))
            subagentSummariesById.set(m.id, []);
          continue;
        }
        if (!activeParentId) continue;
        if (m.name !== 'summary') continue;
        if (!m.agentName || m.agentName === agentName) continue;
        try {
          const args = JSON.parse(m.content);
          const ctx =
            typeof args.contextSummary === 'string' ? args.contextSummary : '';
          const tr = typeof args.taskResult === 'string' ? args.taskResult : '';
          if (!ctx && !tr) continue;
          subagentSummariesById.get(activeParentId)!.push({
            stageName: m.agentName,
            contextSummary: ctx,
            taskResult: tr,
          });
        } catch {
          // ignore unparsable summary args
        }
      }
    }

    const stageColor = (stageName: string) =>
      getAgentColor(stageName, getColor);
    const theme = buildRenderTheme(
      getColor,
      getUserPromptColor,
      getUserPromptBgHex
    );
    const renderCtx = {
      pendingApprovalToolCallId: pendingApproval?.toolCall.toolCallId ?? null,
      termCols: process.stdout.columns ?? 80,
      subagentSummariesById,
      getStageInputColor: stageColor,
      getStageOutputColor: stageColor,
      // Same per-agent palette the footer uses for its agent chip — keeps
      // the scrollback role tag and the footer agent name in lockstep.
      getAgentTagColor: stageColor,
      theme,
      glyphs,
    };

    /**
     * Emit the trailer for `userId` if its summary is available and we
     * haven't already pushed it. Trailer placement is locked at FIRST
     * emission — re-rendering the items array would race twinki's
     * monotonic cursor and re-emit the trailer at a different index.
     */
    const commitTrailer = (userId: string) => {
      if (committedTurnSummariesRef.current.has(userId)) return;
      const summary = turnSummaries.get(userId);
      if (!summary) return;
      committedTurnSummariesRef.current.add(userId);
      const summaryId = `${userId}__summary`;
      items.push({
        id: summaryId,
        text: formatTurnSummaryRow(chalk.dim(`  ${summary}`)),
      });
    };

    // Walk only the delta — eligible messages that haven't been appended
    // yet. The prior segment is already in `items` and must not change.
    const start = lastFlushedEligibleCountRef.current;
    let prevMsg = lastAppendedEligibleMsgRef.current;
    for (let i = start; i < eligible.length; i++) {
      const msg = eligible[i]!;
      // Belt-and-suspenders dedup against the stable-prefix assumption (see
      // pushedStaticIdsRef): a Model row flipping eligible after a later row
      // was flushed would otherwise re-push that row forever.
      if (pushedStaticIdsRef.current.has(msg.id)) continue;
      // Trailer flush rule (1): a User OR System row closes the open turn —
      // push its trailer BEFORE this boundary row (System included so /verbose
      // status announcements don't shove the trailer into a later turn).
      if (
        openTurnUserIdRef.current &&
        (msg.role === MessageRole.User || msg.role === MessageRole.System)
      ) {
        commitTrailer(openTurnUserIdRef.current);
      }
      const prefix =
        prevMsg !== null && needsLeadingBlank(prevMsg, msg) ? '\n' : '';
      const text =
        prefix + renderMessageToText(msg, agentName ?? undefined, renderCtx);
      items.push({ id: msg.id, text });
      pushedStaticIdsRef.current.add(msg.id);
      prevMsg = msg;
      // A new User message opens a new turn; remember its id so we know
      // which trailer to flush at the next User/System or at turn end.
      if (msg.role === MessageRole.User) {
        openTurnUserIdRef.current = msg.id;
      }
    }
    lastFlushedEligibleCountRef.current = eligible.length;
    lastAppendedEligibleMsgRef.current = prevMsg;

    // Trailer flush rule (2): the turn is fully settled (`!isProcessing`)
    // and its summary is available — emit the trailer at the tail.
    if (
      !isProcessing &&
      openTurnUserIdRef.current &&
      turnSummaries.has(openTurnUserIdRef.current)
    ) {
      commitTrailer(openTurnUserIdRef.current);
    }

    // Return a NEW array reference each render — twinki's <Static> compares
    // `items` by reference, so the same mutated array would be blind to the
    // newly appended entries. Shallow copy of pointers is cheap.
    return items.slice();
  }, [
    messages,
    isProcessing,
    turnSummaries,
    agentName,
    activeToolBatchIds,
    pendingApproval,
    liteStaticSkipBefore,
    glyphs,
    // For the first-content banner push + welcome-screen greeting filter; both
    // stable across renders, so a /settings allowAsciiArt toggle reflows the
    // about-to-commit banner row.
    welcomeBannerText,
    showWelcomeBanner,
  ]);

  const handleSubmit = useCallback(
    (value: string) => {
      const trimmed = value.trim();
      if (!trimmed) return;
      handleUserInput(value);
    },
    [handleUserInput]
  );

  const handleTriggerDetected = useCallback(
    (
      trigger: {
        key: string;
        position: number;
        type: 'start' | 'inline';
      } | null
    ) => {
      setActiveTrigger(trigger);
    },
    [setActiveTrigger]
  );

  // Context % estimate during streaming. Read the streamingContent slot
  // directly (not via `messages`) so a per-chunk update doesn't invalidate
  // every [messages]-dep memo; selector returns '' when idle.
  const streamingContent = useAppStore((s) =>
    s.isProcessing ? s.streamingContent : ''
  );

  const ctxPct = useMemo(() => {
    const base =
      contextUsagePercent != null ? Math.round(contextUsagePercent) : 0;
    if (!isProcessing || !streamingContent) return base;
    // Rough estimate: ~4 chars per token, 200k context window typical
    // Each 800 chars ≈ 200 tokens ≈ 0.1% of context
    const estimatedNewTokens = Math.floor(streamingContent.length / 4);
    const estimatedPctIncrease = Math.floor(estimatedNewTokens / 200); // ~0.5% per 1000 chars
    return Math.min(99, base + estimatedPctIncrease);
  }, [contextUsagePercent, isProcessing, streamingContent]);
  const ctxColor = gradientCtxColor(ctxPct);

  // Active-subagents footer strip: one row per running stage in spawn order,
  // with a per-stage phase (running → summarizing → complete). When every
  // known stage is complete and the parent `subagent` tool is still in flight,
  // a "Summarizing N agents..." footnote shows the parent finalize phase. No
  // overflow collapse — every stage gets a row (the old 3-row cap lied about
  // pipeline state past 3 stages).
  const { activeSubagents, summarizingPhase } = useMemo<{
    activeSubagents: SubagentRow[];
    summarizingPhase: boolean;
  }>(() => {
    if (!isProcessing) return { activeSubagents: [], summarizingPhase: false };
    // No pipeline this session — skip the walks (common short-turn case).
    if (!hasAnySubagentTool)
      return { activeSubagents: [], summarizingPhase: false };

    const anyParentSubagentRunning = messages.some(
      (m) =>
        m.role === MessageRole.ToolUse &&
        isParentSubagentTool(m.name) &&
        !m.isFinished
    );
    // Strip is empty unless a parent subagent is in flight — bail early.
    if (!anyParentSubagentRunning)
      return { activeSubagents: [], summarizingPhase: false };

    const byStage = new Map<string, SubagentRow>();
    const order: string[] = [];

    // Seed rows from sessions FIRST: the ACP session exists at spawn, but a
    // stage only emits ToolUse after its first tool call, so without this seed
    // a thinking/streaming stage wouldn't appear until its summary fires.
    // Include terminated stages too — sessions.values() is spawn order, so
    // seeding them here lets the message walk update in place rather than
    // reshuffling the row to the tail mid-run when the stage finishes.
    for (const session of sessions.values()) {
      const stageName = session.name;
      if (!stageName || stageName === agentName) continue;
      if (byStage.has(stageName)) continue;
      order.push(stageName);
      byStage.set(stageName, {
        name: stageName,
        // `killed` phase reflects explicit user kills ONLY: the backend drops a
        // session on normal completion too (index.tsx → status 'terminated'),
        // so a status check would paint completed stages as red `✗ killed`.
        // Only the Ctrl+X handler adds to userKilledSessionsRef.
        phase: userKilledSessionsRef.current.has(session.id)
          ? 'killed'
          : 'running',
        activeToolName: null,
        activeToolDetail: null,
        activeToolFinished: false,
      });
    }

    // Toolcall id for the current approval (if any) — flips the requesting
    // stage's row to "blocked on user" instead of a misleading "running".
    const approvalToolCallId = pendingApproval?.toolCall.toolCallId ?? null;

    for (const m of messages) {
      if (m.role !== MessageRole.ToolUse) continue;
      if (!m.agentName || m.agentName === agentName) continue;
      if (!byStage.has(m.agentName)) {
        order.push(m.agentName);
        byStage.set(m.agentName, {
          name: m.agentName,
          phase: 'running',
          activeToolName: null,
          activeToolDetail: null,
          activeToolFinished: false,
        });
      }
      const row = byStage.get(m.agentName)!;
      // Phase transitions: running → (requesting-permission ↔ running) →
      // summarizing → complete. The permission phase is reversible — when
      // the user answers, pendingApproval clears and the row falls back to
      // running on the next render. Complete and killed are terminal — both
      // get an early-continue so the message walk can't downgrade them back
      // to running on the next render.
      if (row.phase === 'complete' || row.phase === 'killed') continue;
      if (m.name === 'summary') {
        row.phase = m.isFinished ? 'complete' : 'summarizing';
        row.activeToolName = null;
        row.activeToolDetail = null;
        row.activeToolFinished = !!m.isFinished;
        continue;
      }
      if (row.phase === 'summarizing') continue;
      row.activeToolName = m.name;
      row.activeToolDetail = extractFooterToolDetail(m.name, m.content);
      row.activeToolFinished = !!m.isFinished;
      // If THIS message is the one currently waiting on user approval, flip
      // the row's phase. Done after activeToolName/Detail are set so the
      // chip can include the tool name verbatim.
      if (approvalToolCallId && m.id === approvalToolCallId) {
        row.phase = 'requesting-permission';
      }
    }

    // Surfacing subagent rows is gated on a parent subagent tool being in
    // flight (early-returned above). Without that gate, completed rows from
    // a prior subagent run would resurrect on the next processing turn:
    // user sends a new message → isProcessing flips on → this memo re-runs
    // over a message list that still contains the old subagent's tool
    // entries → byStage repopulates them at phase: 'complete'.
    const rows: SubagentRow[] = order.map((name) => byStage.get(name)!);

    // Parent finalize phase: every stage we know about is complete but
    // the parent subagent tool itself is still working (concatenating all
    // stage summaries into the combined response). Surface one footnote.
    const summarizingPhase =
      rows.length > 0 && rows.every((r) => r.phase === 'complete');

    return { activeSubagents: rows, summarizingPhase };
  }, [
    messages,
    isProcessing,
    agentName,
    sessions,
    hasAnySubagentTool,
    pendingApproval,
  ]);

  // Map subagent display names to sessionIds so the open panel can subscribe
  // to the right session-conversation slice. The lookup is name-based — the
  // backend doesn't expose stage→session in the strip row data — but stage
  // names are unique within a single subagent invocation so this is reliable.
  const subagentSessionIdByName = useMemo(() => {
    const out = new Map<string, string>();
    for (const [id, s] of sessions) {
      if (!out.has(s.name)) out.set(s.name, id);
    }
    return out;
  }, [sessions]);

  // Auto-clamp / close the panel when the focused subagent disappears (e.g.
  // the parent subagent tool finished and activeSubagents drained). Without
  // this the panel would render nothing and trap arrow keys.
  useEffect(() => {
    if (subagentOpenIndex == null) return;
    if (activeSubagents.length === 0) {
      setSubagentOpenIndex(null);
      setSubagentScrollOffset(0);
      return;
    }
    if (subagentOpenIndex >= activeSubagents.length) {
      setSubagentOpenIndex(activeSubagents.length - 1);
      setSubagentScrollOffset(0);
    }
  }, [activeSubagents, subagentOpenIndex]);

  // Auto-expand the requesting subagent's panel while its permission is pending
  // (the chat log hides inner subagent activity). Snapshot prior panel state on
  // request, restore on clear — keyed by toolCallId (via a ref) so consecutive
  // requests don't re-snapshot from an already-overridden state.
  const subagentRequestingName = useMemo<string | null>(() => {
    if (!pendingApproval) return null;
    const id = pendingApproval.toolCall.toolCallId;
    if (!id) return null;
    const msg = messages.find(
      (m) => m.role === MessageRole.ToolUse && m.id === id
    );
    if (!msg || msg.role !== MessageRole.ToolUse) return null;
    if (!msg.agentName) return null;
    if (msg.agentName === agentName) return null;
    return msg.agentName;
  }, [pendingApproval, messages, agentName]);
  const autoExpandSnapshotRef = useRef<{
    toolCallId: string;
    openIndex: number | null;
    scrollOffset: number;
    followBottom: boolean;
  } | null>(null);
  // Mirror panel state into refs so the effect reads them without listing
  // them as deps (otherwise the setSubagentOpenIndex call below would
  // re-run the effect and overwrite the snapshot).
  const subagentScrollOffsetRef = useRef(subagentScrollOffset);
  subagentScrollOffsetRef.current = subagentScrollOffset;
  const subagentFollowBottomRef = useRef(subagentFollowBottom);
  subagentFollowBottomRef.current = subagentFollowBottom;
  useEffect(() => {
    const id = pendingApproval?.toolCall.toolCallId ?? null;
    // Approval cleared (or moved to a non-subagent tool) — restore prior state.
    if (!subagentRequestingName) {
      const snap = autoExpandSnapshotRef.current;
      if (snap) {
        setSubagentOpenIndex(snap.openIndex);
        setSubagentScrollOffset(snap.scrollOffset);
        setSubagentFollowBottom(snap.followBottom);
        autoExpandSnapshotRef.current = null;
      }
      return;
    }
    // Same approval as before — leave panel state alone (user may have
    // scrolled / cycled within the auto-opened panel).
    if (autoExpandSnapshotRef.current?.toolCallId === id) return;
    if (!id) return;
    // Map the requesting stage name to its index in the current strip.
    const targetIdx = activeSubagents.findIndex(
      (s) => s.name === subagentRequestingName
    );
    if (targetIdx < 0) return;
    autoExpandSnapshotRef.current = {
      toolCallId: id,
      openIndex: subagentOpenIndexRef.current,
      scrollOffset: subagentScrollOffsetRef.current,
      followBottom: subagentFollowBottomRef.current,
    };
    setSubagentOpenIndex(targetIdx);
    setSubagentScrollOffset(0);
    setSubagentFollowBottom(true);
  }, [subagentRequestingName, pendingApproval, activeSubagents]);

  // Panel keypress handler: ctrl+o toggle, esc close, shift+←/→ cycle (←/→ stay
  // free for the prompt cursor), ↑/↓ scroll, pgup/pgdn page, ctrl+a top, ctrl+z
  // bottom+follow. We avoid ctrl+1/0 (terminal tab-nav); ctrl+z normally
  // suspends, so the global dispatcher skips suspend when subagentPanelOpen.
  const SUBAGENT_SCROLL_STEP = 3;
  const SUBAGENT_PAGE_STEP = 8;
  const PANEL_LINES = 16;
  // Floor offset for follow disengagement / re-engagement.
  const subagentMaxOffset = Math.max(0, subagentTotalLines - PANEL_LINES);
  useKeypress((input, key) => {
    // Ctrl+X — kill ladder for the focused subagent: first press arms (yellow
    // chip + 2s window), second within the window kills. This useKeypress runs
    // unconditionally, so every branch gates on subagentOpenIndex itself.
    if (key.ctrl && (input === 'x' || input === 'X')) {
      if (subagentOpenIndex == null) return;
      const focused = activeSubagents[subagentOpenIndex];
      // Bail on terminal phases — re-firing terminate on a dead session would
      // race the prior kill's local cleanup.
      if (
        !focused ||
        focused.phase === 'complete' ||
        focused.phase === 'killed'
      )
        return;
      const sessionId = subagentSessionIdByName.get(focused.name);
      if (!sessionId) return;
      if (armedKillSessionId === sessionId) {
        // Second press — kill. Logged at info for trace correlation. (Backend
        // session_manager.rs writes "[Cancelled by user]" to the stage result
        // so the parent doesn't read None-as-failure and re-spawn the work.)
        logger.info('[lite] killing subagent stage', {
          sessionId,
          stageName: focused.name,
          phase: focused.phase,
        });
        disarmKill();
        // Mark user-killed BEFORE updateSession/terminate so the next byStage
        // run sees the kill flag at the same time as status: 'terminated'.
        userKilledSessionsRef.current.add(sessionId);
        updateSession(sessionId, { status: 'terminated' });
        kiro?.terminateSession(sessionId).catch(() => {});
        cleanupTerminatedSession(sessionId);
        // Stamp in-flight tool calls finished locally (the terminate
        // notification doesn't replay them as cancelled) so the trace has no
        // forever-spinning row.
        const convStore = sessionConversationsStore.getState();
        const msgs = convStore.conversations.get(sessionId);
        if (
          msgs?.some((m) => m.role === MessageRole.ToolUse && !m.isFinished)
        ) {
          sessionConversationsStore.setState((s) => {
            const m = new Map(s.conversations);
            m.set(
              sessionId,
              msgs.map((msg) =>
                msg.role === MessageRole.ToolUse && !msg.isFinished
                  ? { ...msg, isFinished: true }
                  : msg
              )
            );
            return { conversations: m };
          });
        }
        // Drop the pending approval if it belonged to the killed stage — its
        // process is gone, any answer would fail silently. (Unit-tested helper.)
        if (
          shouldCancelApprovalForKilledStage(
            pendingApproval,
            messages,
            focused.name
          )
        ) {
          cancelApproval();
        }
        return;
      }
      // First press — arm. Replace any existing arm timer (re-arming on a
      // different session shifts the target with a fresh 2s window).
      if (armedKillTimerRef.current) clearTimeout(armedKillTimerRef.current);
      setArmedKillSessionId(sessionId);
      armedKillTimerRef.current = setTimeout(() => {
        setArmedKillSessionId(null);
        armedKillTimerRef.current = null;
      }, 2000);
      return;
    }
    if (key.ctrl && (input === 'o' || input === 'O')) {
      if (subagentOpenIndex != null) {
        // Closing the panel — disarm so a stale armed state doesn't
        // survive into the next open.
        disarmKill();
        setSubagentOpenIndex(null);
        setSubagentScrollOffset(0);
        setSubagentFollowBottom(true);
        return;
      }
      if (activeSubagents.length === 0) return;
      setSubagentOpenIndex(0);
      setSubagentScrollOffset(0);
      setSubagentFollowBottom(true);
      return;
    }
    if (subagentOpenIndex == null) return;
    if (key.escape) {
      // While armed, Esc CANCELS the arm rather than closing the panel.
      // Matches the hint-row text ("ctrl+x KILL · esc cancel") and lets
      // users bail out of an accidental arm without losing the panel
      // they were inspecting.
      if (armedKillSessionId != null) {
        disarmKill();
        return;
      }
      // Esc closes the panel.
      setSubagentOpenIndex(null);
      setSubagentScrollOffset(0);
      setSubagentFollowBottom(true);
      return;
    }
    // Cycle through every active subagent — every stage now renders in the
    // strip, so the cycle wraps over the full list. Only fires with shift
    // modifier so unmodified ←/→ stay free for the prompt input cursor.
    if (key.shift && key.leftArrow) {
      // Cycling away from the armed stage — disarm so the yellow chip
      // doesn't visually follow to a different stage.
      disarmKill();
      const total = activeSubagents.length;
      const next = subagentOpenIndex - 1;
      setSubagentOpenIndex(next < 0 ? total - 1 : next);
      setSubagentScrollOffset(0);
      setSubagentFollowBottom(true);
      return;
    }
    if (key.shift && key.rightArrow) {
      disarmKill();
      const total = activeSubagents.length;
      const next = subagentOpenIndex + 1;
      setSubagentOpenIndex(next >= total ? 0 : next);
      setSubagentScrollOffset(0);
      setSubagentFollowBottom(true);
      return;
    }
    // Ctrl+A → jump to top of trace (mnemonic: A = above / first letter).
    if (key.ctrl && (input === 'a' || input === 'A')) {
      setSubagentFollowBottom(false);
      setSubagentScrollOffset(0);
      return;
    }
    // Ctrl+Z → jump to bottom and re-engage follow (Z = last letter / below).
    // The global Ctrl+Z suspend in app-keypress-dispatch is gated on
    // !subagentPanelOpen so the panel reliably claims it here.
    if (key.ctrl && (input === 'z' || input === 'Z')) {
      setSubagentFollowBottom(true);
      setSubagentScrollOffset(subagentMaxOffset);
      return;
    }
    // ↑ disables follow and steps up SUBAGENT_SCROLL_STEP rows. If we were
    // following, seed the offset at "step rows above floor" so the first
    // press starts the user just above the floor rather than jumping to the
    // top of a long trace.
    if (key.upArrow) {
      setSubagentFollowBottom(false);
      setSubagentScrollOffset((o) => {
        const base = subagentFollowBottom ? subagentMaxOffset : o;
        return Math.max(0, base - SUBAGENT_SCROLL_STEP);
      });
      return;
    }
    // ↓ steps down SUBAGENT_SCROLL_STEP rows. When the new offset reaches
    // the floor, re-engage follow so subsequent agent activity stays pinned
    // to the bottom of the panel.
    if (key.downArrow) {
      if (subagentFollowBottom) return; // already at floor
      setSubagentScrollOffset((o) => {
        const next = o + SUBAGENT_SCROLL_STEP;
        if (next >= subagentMaxOffset) {
          setSubagentFollowBottom(true);
          return subagentMaxOffset;
        }
        return next;
      });
      return;
    }
    if (key.pageUp) {
      setSubagentFollowBottom(false);
      setSubagentScrollOffset((o) => {
        const base = subagentFollowBottom ? subagentMaxOffset : o;
        return Math.max(0, base - SUBAGENT_PAGE_STEP);
      });
      return;
    }
    if (key.pageDown) {
      if (subagentFollowBottom) return;
      setSubagentScrollOffset((o) => {
        const next = o + SUBAGENT_PAGE_STEP;
        if (next >= subagentMaxOffset) {
          setSubagentFollowBottom(true);
          return subagentMaxOffset;
        }
        return next;
      });
    }
  });

  // Resolve the user's prompt preset into chalk wrappers for the input row —
  // glyph color + box bg, so /theme highlights the input itself, not just
  // scrollback user messages.
  const promptGlyph = useMemo(() => {
    try {
      const fn = getUserPromptColor();
      const probe = fn('');
      if (typeof probe !== 'string') return chalk.cyan;
      return (s: string) => fn(s);
    } catch {
      return chalk.cyan;
    }
  }, [getUserPromptColor]);
  const promptBgHex = useMemo(() => {
    try {
      const hex = getUserPromptBgHex();
      return hex && hex !== 'inherit' ? hex : undefined;
    } catch {
      return undefined;
    }
  }, [getUserPromptBgHex]);

  return (
    <Box flexDirection="column">
      {/* Scrollback: finalized messages, append-only. wrap="overflow" writes
          each item.text as-is and lets the terminal soft-wrap — pairs with
          lite/render.ts's policy of \n only at structural breaks so copy-paste
          preserves logical lines (wideLines enabled at index.tsx). */}
      <Static items={staticItems}>
        {(item) => (
          <Text key={item.id} wrap="overflow">
            {item.text}
          </Text>
        )}
      </Static>

      {/* Welcome banner — live-region row while the welcome screen is active.
          Lives outside <Static> so a resize (which clears
          accumulatedStaticOutput) reflows it instead of an empty viewport. The
          staticItemsRef.current.length === 0 gate keeps it mutually exclusive
          with the swap-with-content static push above, so the user never sees
          two KIRO arts at once. */}
      {showWelcomeBanner && staticItemsRef.current.length === 0 && (
        <Text wrap="overflow">{welcomeBannerText}</Text>
      )}

      {/* Agent's standalone greeting, alongside the banner. Filtered out of the
          static eligible set while showWelcomeBanner; commits to <Static> via
          the fallback path once the welcome screen ends. */}
      {showWelcomeBanner && welcomeGreetingText && (
        <Text wrap="overflow">{welcomeGreetingText}</Text>
      )}

      {/* Live region: streaming + tools */}
      {agentError && <Text>{chalk.red(`error: ${agentError}`)}</Text>}
      <LiteLiveRegion />

      {/* Spec-artifact generation banner — self-renders null when no
          generation is in flight, so it's safe to mount unconditionally. */}
      <ArtifactGenerationCard />
      {surveyPrompt && (
        <SurveyPromptBar
          message={surveyPrompt.message}
          onDismiss={dismissSurveyPrompt}
        />
      )}

      {/* Queued messages — preview rows only (full text lives in the store,
          restored verbatim on pull-back). previewLine cap + wrap="truncate-end"
          bound each row's cost by width, not length — rendering full text here
          hung the UI on multi-KB paste. Hidden during shell escape (queue is
          for the agent, not bash). */}
      {queuedMessages.length > 0 && !isShellEscape && (
        <Box flexDirection="column">
          {queuedMessages.map((msg, i) => {
            const editing = editingQueueIndex === i;
            const marker = editing ? chalk.cyan(`${glyphs.chevron} `) : '  ';
            // Reserve cols for marker + index prefix so a wide preview can't
            // overrun the terminal edge before truncate-end kicks in.
            const cols = process.stdout.columns ?? 80;
            const previewWidth = Math.max(20, cols - 8);
            const preview = previewLine(msg, previewWidth);
            const body = editing
              ? chalk.cyan(`${i + 1}. ${preview}`)
              : chalk.dim(`${i + 1}. ${preview}`);
            return (
              <Text key={i} wrap="truncate-end">
                {marker}
                {body}
              </Text>
            );
          })}
          <Text>{chalk.dim(`  (${queuedMessages.length} queued)`)}</Text>
        </Box>
      )}

      {/* Footer: divider + input area */}
      {loadingMessage && (
        <Text>
          {chalk.dim(
            `  ${spinners.brailleRotate[bootFrame % spinners.brailleRotate.length]} ${loadingMessage}`
          )}
        </Text>
      )}
      {/* Task tray — mirrors <ActivityTray />; self-renders null when empty.
          Toggle Ctrl+X (handled at the layout level above). */}
      <LiteTaskTray />
      <Divider />

      {/* Status line (Divider → header → input order). Hidden while connecting
          and during shell escape (agent isn't running). Pending agent swap
          renders the requested name with a spinner. */}
      {isInitialized &&
        !isShellEscape &&
        (() => {
          // Discrete colored segments packed into width-bounded lines so the
          // row wraps as the terminal narrows (mirrors ContextBar flex-wrap).
          // Empty goal segment is dropped; floor 20 cols.
          const cols = Math.max(20, process.stdout.columns ?? 80);
          const agentSeg = pendingAgentName
            ? renderPendingAgent(
                pendingAgentName,
                bootFrame,
                getColor,
                spinners.brailleRotate
              )
            : colorAgentName(agentName, getColor);
          const segments = [
            agentSeg,
            modelName ? chalk.dim(modelName) : '',
            currentEffort ? chalk.dim(formatEffort(currentEffort)) : '',
            `${ctxColor(`${ctxPct}%`)} ${chalk.dim('ctx')}`,
            gitBranch
              ? chalk.dim(
                  gitBranch.length > 24
                    ? gitBranch.slice(0, 23) + '…'
                    : gitBranch
                )
              : '',
            formatGoalStatusSegment(goalStatus),
          ];
          const lines = packStatusSegments(segments, cols, chalk.dim(' · '));
          return (
            <Box flexDirection="column">
              {lines.map((line, i) => (
                <Text key={i}>{line}</Text>
              ))}
            </Box>
          );
        })()}

      {/* Boot indicator — single dim row for in-flight async setup. Rationale
          on the showBootIndicator memo above. Inline (not memoized) so elapsed
          updates every bootFrame tick. */}
      {showBootIndicator && (
        <Text>
          {formatBootIndicator(
            selectBootIndicatorPhase(bootProgress, mcpInitStatus),
            spinners.brailleRotate[bootFrame % spinners.brailleRotate.length]!
          )}
        </Text>
      )}

      {/* Approval prompt — inside the input area. No marginTop so the divider
          and status line don't jump when it mounts/unmounts. */}
      {showApproval && (
        <Box flexDirection="column">
          <ApprovalPrompt
            messages={messages}
            approval={pendingApproval}
            respondToApproval={respondToApproval}
            getStageInputColor={(stageName: string) =>
              getAgentColor(stageName, getColor)
            }
            mainAgentName={agentName}
          />
        </Box>
      )}

      {/* Backend-driven panels (/context, /mcp, /help, ...). Replace the input
          area while open (same components InlineLayout mounts). They own Esc
          via Panel.tsx; the always-armed handler short-circuits Esc when
          anyPanelOpen so it doesn't also cancel the turn. */}
      {!showApproval && anyPanelOpen && (
        <Box flexDirection="column">
          <BackendPanels handlers={handlers} />
        </Box>
      )}

      {!showApproval && !anyPanelOpen && (
        <Box flexDirection="column">
          {/* Editing-queue header — shows which slot is being edited. */}
          {editingQueueIndex != null && (
            <Text>
              {chalk.cyan(
                `${glyphs.chevron} editing queued #${editingQueueIndex + 1}`
              )}
              {chalk.dim(' · enter saves · ctrl+x deletes · esc cancels')}
            </Text>
          )}
          {transientAlert && (
            <Text>
              {colorTransientAlert(
                transientAlert.message,
                transientAlert.status
              )}
            </Text>
          )}
          {/* Input row — glyph + box pick up the user's prompt preset colors so
              /theme re-skins the lite input. width="100%" on the row +
              flexShrink={1} on the inner box are load-bearing for multi-line
              wrap: without an explicit row width, Yoga sizes the row to the `> `
              glyph and the Text has nothing to wrap against (PromptBar uses the
              same pattern). Hidden while the /verbosity menu is active (its
              breadcrumb header takes the input's place). */}
          {!verbosityMenuActive && (
            <Box flexDirection="row" width="100%" backgroundColor={promptBgHex}>
              <Text>
                {isShellEscape ? getColor('brand')('! ') : promptGlyph('> ')}
              </Text>
              <Box flexGrow={1} flexShrink={1}>
                <PromptInput
                  onSubmit={handleSubmit}
                  isProcessing={isProcessing}
                  triggerRules={TRIGGER_RULES}
                  onTriggerDetected={handleTriggerDetected}
                  placeholder={
                    isShellEscape
                      ? 'bash is waiting for input · ctrl+c to interrupt'
                      : 'ask a question, or type / for commands'
                  }
                  suppressArrows={subagentOpenIndex != null}
                />
              </Box>
            </Box>
          )}
          {/* Slash-command dropdown + /settings menu render BELOW the input
              (matches the TUI). Self-renders null when inactive. */}
          <CommandMenu />
          {exitSequence > 0 && (
            <Text>{chalk.dim('Press Ctrl+C or Ctrl+D again to exit')}</Text>
          )}
        </Box>
      )}

      {/* Subagent activity strip — pinned at the bottom, one row per stage.
          Ctrl+O replaces the focused row with a fixed-height trace panel. */}
      {activeSubagents.length > 0 &&
        (() => {
          const visible = activeSubagents;
          const cols = Math.max(40, process.stdout.columns ?? 80);
          const completedCount = activeSubagents.filter(
            (r) => r.phase === 'complete'
          ).length;
          const openIdx = subagentOpenIndex;
          const focused = openIdx != null ? activeSubagents[openIdx] : null;
          const focusedSessionId = focused
            ? (subagentSessionIdByName.get(focused.name) ?? null)
            : null;
          // PANEL_LINES here MUST match the keypress handler's constant so the
          // floor-detection math stays consistent.
          return (
            <Box flexDirection="column">
              {/* Top breathing room. */}
              <Text> </Text>
              {visible.map((sub, i) => {
                if (openIdx === i && focused && focusedSessionId) {
                  return (
                    <LiteSubagentPanel
                      key={`panel-${sub.name}`}
                      sessionId={focusedSessionId}
                      name={focused.name}
                      position={openIdx + 1}
                      total={activeSubagents.length}
                      visibleLines={PANEL_LINES}
                      scrollOffset={subagentScrollOffset}
                      followBottom={subagentFollowBottom}
                      onLinesChange={setSubagentTotalLines}
                      phaseLabel={focused.phase}
                      armedToKill={focusedSessionId === armedKillSessionId}
                    />
                  );
                }
                if (openIdx === i && focused && !focusedSessionId) {
                  // No sessionId yet (rare race when the panel opens right after
                  // spawn) — show a placeholder.
                  const tag = getAgentColor(
                    focused.name,
                    getColor
                  )(`[${focused.name}]`);
                  return (
                    <Text key={`panel-${sub.name}`}>
                      {chalk.dim(
                        `${glyphs.cornerTopLeft}${glyphs.lineHorizontal} `
                      )}
                      {tag}
                      {chalk.dim(' (no trace yet)')}
                    </Text>
                  );
                }
                return (
                  <Text key={sub.name}>
                    {formatSubagentRow(
                      sub,
                      cols,
                      getAgentColor(sub.name, getColor),
                      glyphs
                    )}
                  </Text>
                );
              })}
              {summarizingPhase && (
                <Text>
                  {chalk.dim(
                    `Summarizing ${completedCount} agent${completedCount === 1 ? '' : 's'}...`
                  )}
                </Text>
              )}
              {/* Discoverability hint — only when the panel is closed. */}
              {openIdx == null && activeSubagents.length > 0 && (
                <Text>{chalk.dim('  press ctrl+o to expand')}</Text>
              )}
            </Box>
          );
        })()}
    </Box>
  );
};

// Color the transient alert by status (no chip/icon — lite runs lean).
function colorTransientAlert(message: string, status: string): string {
  switch (status) {
    case 'error':
      return chalk.red(message);
    case 'warning':
      return chalk.yellow(message);
    case 'success':
      return chalk.green(message);
    default:
      return chalk.dim(message);
  }
}

// Goal-loop status segment (icon + state + iteration). '' when no goal so
// packStatusSegments drops it. Goal TEXT isn't shown here (too long); bare
// `/goal` surfaces it via a transient alert + scrollback confirmation.
function formatGoalStatusSegment(
  goalStatus: {
    state: string;
    iteration: number;
    maxIterations: number;
  } | null
): string {
  if (!goalStatus) return '';
  const iter = `[${goalStatus.iteration + 1}/${goalStatus.maxIterations}]`;
  switch (goalStatus.state) {
    case 'completed':
      return chalk.green('✓ goal done');
    case 'exhausted':
      return chalk.red('✗ goal exhausted');
    case 'paused':
      return chalk.yellow(`⏸ goal paused ${iter}`);
    default:
      return chalk.dim(`⟳ goal ${iter}`);
  }
}

// Color the agent name with its stable agentColors.ts color (same as the V2
// InlineLayout chip). Name shown verbatim; only the color is mapped.
function colorAgentName(
  agentName: string | null,
  getColor: (path: string) => any
): string {
  const raw = agentName || 'kiro';
  const color = getAgentColor(raw, getColor);
  return color(raw);
}

// Smooth RGB gradient for the ctx-usage indicator (replaces step thresholds
// that flipped yellow too late on large-window models). Piecewise-linear:
// 0-20% flat green, 20-30% green→yellow, 30-100% yellow→red. chalk.rgb
// degrades to nearest 256-color without truecolor support.
function gradientCtxColor(pct: number): (s: string) => string {
  const p = Math.max(0, Math.min(100, pct));
  const lerp = (a: number, b: number, t: number) => Math.round(a + (b - a) * t);
  if (p <= 20) {
    return chalk.rgb(80, 200, 80);
  }
  if (p <= 30) {
    const t = (p - 20) / 10;
    return chalk.rgb(lerp(80, 220, t), lerp(200, 220, t), lerp(80, 0, t));
  }
  const t = (p - 30) / 70;
  return chalk.rgb(220, lerp(220, 60, t), lerp(0, 60, t));
}
