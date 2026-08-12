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
import { useStore } from 'zustand';
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
import {
  computeActiveToolBatchIds,
  needsLeadingBlank,
  selectStaticEligible,
} from './static-flush.js';
import { previewLine } from './queue-preview.js';
import { buildUnifiedQueueEntries } from '../../../utils/queue-navigation.js';
import {
  renderMessageToText,
  buildRenderTheme,
  toolDisplayName,
  type SubagentStageSummary,
} from '../../../lite/render.js';
import { getVerboseDisplay, getVerboseFilters } from '../../../lite/verbose.js';
import { pickTip, formatTipLine } from '../../../tips/tips.js';
import { Question } from '../../ui/Question.js';
import { SpecDescriptionIntro } from '../../ui/SpecDescriptionIntro.js';
import { VoiceModelDownloadGate } from '../../ui/VoiceModelDownloadGate.js';
import type { VariantLayoutProps } from '../variant-layout.js';
import {
  formatSubagentRow,
  extractFooterToolDetail,
  isSubagentSummaryToolName,
  type SubagentRow,
} from './SubagentFooter.js';
import { shouldCancelApprovalForKilledStage } from './subagent-kill.js';
import { engineSupportsSubagentKill } from '../../../agent-engine.js';
import { sessionConversationsStore } from '../../../stores/session-conversations.js';
import {
  selectLiveWorkflowCount,
  workflowStore,
} from '../../../stores/workflow-store.js';
import {
  selectBootIndicatorPhase,
  formatBootIndicator,
} from './boot-indicator.js';
import { formatCloudStartupChecklist } from '../shared/cloud-startup-checklist.js';
import { cloudConnectStage } from '../shared/cloud-connect-stage.js';
import { getCliVersion } from '../../../utils/version.js';
import { getGitBranch, getGitBranchAsync } from '../../../utils/git.js';
import { PromptInput } from '../../chat/prompt-bar/PromptInput.js';
import { CommandMenu } from '../../ui/CommandMenu.js';
import { Divider } from '../../ui/divider/Divider.js';
import { useKeypress } from '../../../hooks/useKeypress.js';
import { useInteractionReady } from '../../../hooks/useInteractionReady.js';
import { usePlanModeToggle } from '../../../hooks/usePlanModeToggle.js';
import { useKeybindings } from '../../../hooks/useKeybindings.js';
import { useCheckpointAnswer } from '../../../hooks/useCheckpointAnswer.js';
import { useTheme } from '../../../hooks/useThemeContext.js';
import {
  useGlyphs,
  useSpinners,
  useAllowAsciiArt,
} from '../../../hooks/useGlyphs.js';
import { useAnimationPaused } from '../../../contexts/AnimationPausedContext.js';
import {
  getAgentColor,
  isAutonomousAgent,
} from '../../../utils/agentColors.js';
import { isParentSubagentTool } from '../../../types/agent-events.js';
import {
  collectSettledSubagentStagesByParent,
  collectSubagentSummariesByParent,
  markSubagentSummariesEmitted,
  renderPendingSubagentSummaryAppendices,
  selectPendingSubagentSummaryEntries,
  selectReadySubagentSummaries,
  shouldRenderSubagentResponseSummaries,
} from './subagent-summaries.js';
import { usePendingSwap } from './usePendingSwap.js';
import { logger } from '../../../utils/logger.js';
import { chalk } from '../../../utils/color.js';
import {
  BackendPanels,
  useBackendPanelVisibility,
} from '../shared/BackendPanels.js';
import { SourceProviderGate } from '../../ui/SourceProviderGate.js';
import { openUrlInBrowser } from '../../../utils/browser.js';
import { SOURCE_PROVIDER_SETUP_URL } from '../../../utils/cloud-urls.js';
import { getPlaceholder } from '../getPlaceholder.js';
import { useStatusSurfaceProps } from '../useStatusSurfaceProps.js';
import { useBackendPanelHandlers } from '../shared/useBackendPanelHandlers.js';
import { ArtifactGenerationCard } from '../../ui/ArtifactView/ArtifactGenerationCard.js';
import { SurveyPromptBar } from '../../ui/SurveyPromptBar.js';
import { useUIState } from '../../../stores/selectors.js';
import { useStatusBilling } from '../status-line/useStatusBilling.js';
import {
  selectActiveSubagentToolScopes,
  selectSubagentToolMessagesForScope,
  selectSubagentToolSessions,
  activePipelineGroupConstraint,
  selectScopeSeedSessions,
} from '../../ui/subagent-session-filter.js';

const TRIGGER_RULES = [
  { key: '/', type: 'start' as const },
  { key: '@', type: 'inline' as const },
];

type ActiveSubagentRow = SubagentRow & {
  key: string;
  sessionId?: string;
};

// Last `lite.scrollbackClearToken` observed. MODULE-LEVEL (not a per-mount ref)
// so the reset block below survives bare unmount/remount (Ctrl+G, session-view)
// WITHOUT re-running — twinki's monotonic cursor persists across that cycle, so
// re-resetting would duplicate/swallow scrollback. Only resetMessages/setUiMode
// bump the token. -1 fires the reset once on first mount.
let _liteLastObservedClearToken = -1;

export const LiteLayout: React.FC<VariantLayoutProps> = ({
  ApprovalPrompt,
  StatusLine,
  ActivityTray,
}) => {
  const store = useContext(AppStoreContext);
  const messages = useAppStore((s) => s.messages);
  const isProcessing = useAppStore((s) => s.isProcessing);
  // Shell-escape (`!command`) flag: in-flight turn is a PTY-backed bash
  // command, not agent inference. Purely visual mode here — swaps the prompt
  // glyph and suppresses agent-mode chrome while the user interacts with bash
  // (keystroke→PTY forwarding lives in AppContainer's always-armed handler).
  const isShellEscape = useAppStore((s) => s.isShellEscape);
  // Resume-history lower bound; mode changes reset it to zero.
  const liteStaticSkipBefore = useAppStore((s) => s.lite.staticSkipBefore);
  const isInitialized = useAppStore((s) => s.isInitialized);
  const agentError = useAppStore((s) => s.agentError);
  const handleUserInput = useAppStore((s) => s.handleUserInput);
  const pendingApproval = useAppStore((s) => s.pendingApproval);
  const respondToApproval = useAppStore((s) => s.respondToApproval);
  const pendingQuestion = useAppStore((s) => s.pendingQuestion);
  const specDescriptionFeature = useAppStore(
    (s) => s.pendingSpecDescription?.featureName ?? null
  );
  const { checkpointOptions, answerCheckpoint } = useCheckpointAnswer();
  const voiceDownloadConfirm = useAppStore((s) => s.voiceDownloadConfirm);
  const mainSessionId = useAppStore((s) => s.sessionId);
  const currentModel = useAppStore((s) => s.currentModel);
  const currentAgent = useAppStore((s) => s.currentAgent);
  // Shift+Tab toggles plan mode (shared with InlineLayout).
  usePlanModeToggle(!pendingQuestion);
  const contextUsagePercent = useAppStore((s) => s.contextUsagePercent);
  const queuedMessages = useAppStore((s) => s.queuedMessages);
  const editingQueueIndex = useAppStore((s) => s.editingQueueIndex);
  const pendingSteerContent = useAppStore((s) => s.pendingSteerContent);
  const editingSteerLineIndex = useAppStore((s) => s.editingSteerLineIndex);
  const activeInterruptMode = useAppStore((s) => s.activeInterruptMode);
  const tasks = useAppStore((s) => s.tasks);
  const activityTrayExpanded = useAppStore((s) => s.activityTrayExpanded);
  const toggleActivityTray = useAppStore((s) => s.toggleActivityTray);
  const liveWorkflowCount = useStore(workflowStore, selectLiveWorkflowCount);
  const retainedWorkflowCount = useStore(
    workflowStore,
    (state) => state.workflows.size
  );
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
  const cloudSessionActive = useAppStore((s) => s.cloudSessionActive);
  const statusSurface = useStatusSurfaceProps();
  const cloudProviderChecked = useAppStore((s) => s.cloudProviderChecked);
  const cloudProvider = useAppStore((s) => s.cloudProvider);
  const cloudRepoCount = useAppStore((s) => s.cloudRepoCount);
  // Resume vs create wording for the session checklist row: the boot path
  // records the origin (via beginKasSession) before the session RPC runs.
  const cloudSessionResumed = useAppStore(
    (s) => s.kas.sessionOrigin === 'resumed'
  );
  const cancelMessage = useAppStore((s) => s.cancelMessage);
  const resetExitSequence = useAppStore((s) => s.resetExitSequence);
  const wasCancelled = useAppStore((s) => s.wasCancelled);
  const exitSequence = useAppStore((s) => s.exitSequence);
  const transientAlert = useAppStore((s) => s.transientAlert);
  const dismissTransientAlert = useAppStore((s) => s.dismissTransientAlert);
  const loadingMessage = useAppStore((s) => s.loadingMessage);
  const { getColor, getUserPromptColor, getUserPromptBgHex } = useTheme();
  const keybindings = useKeybindings();
  // Accessibility wiring — kept 1:1 with the modern TUI.
  const glyphs = useGlyphs();
  const spinners = useSpinners();
  const { allowAsciiArt } = useAllowAsciiArt();
  const animationPaused = useAnimationPaused();

  const { tangentName, showSourceProviderGate, sourceProviderSetupUrl } =
    useUIState();
  const { any: anyPanelOpen } = useBackendPanelVisibility();
  const surveyPrompt = useAppStore((s) => s.surveyPrompt);
  const dismissSurveyPrompt = useAppStore((s) => s.dismissSurveyPrompt);
  // Goal-loop state (set by `/goal`). Lite surfaces it as a status-line segment
  // and a one-time scrollback confirmation; the panel is shared via BackendPanels.
  const goalStatus = useAppStore((s) => s.goalStatus);

  const handlers = useBackendPanelHandlers();

  // Tick every 60s while a goal is active so the elapsed time in the status
  // line advances even when idle (mirrors InlineLayout's goal chip).
  const [, setGoalTick] = useState(0);
  useEffect(() => {
    if (
      !goalStatus ||
      goalStatus.state === 'completed' ||
      goalStatus.state === 'exhausted'
    )
      return;
    const id = setInterval(() => setGoalTick((t) => t + 1), 60_000);
    return () => clearInterval(id);
  }, [goalStatus]);

  // The lite /verbosity menu renders via <CommandMenu> (not a backend panel)
  // for its live preview + truncation editor, but presents like other
  // settings: a breadcrumb header occupies the (hidden) input row. Lite-only;
  // TUI filters /verbosity out of the menu.
  const verbosityMenuActive =
    uiMode === 'lite' && activeCommand?.command.name === '/verbosity';

  const pendingSwap = usePendingSwap();
  const pendingAgentName = pendingSwap?.name ?? null;
  const unifiedQueueEntries = useMemo(
    () => buildUnifiedQueueEntries(pendingSteerContent, queuedMessages),
    [pendingSteerContent, queuedMessages]
  );
  const isEditingEntry = useCallback(
    () => editingQueueIndex != null || editingSteerLineIndex != null,
    [editingQueueIndex, editingSteerLineIndex]
  );

  // Subagent inline-trace panel (Ctrl+O). subagentOpenIndex = inspected stage
  // (null = closed); mirrored into app-store so dispatch stops Esc from also
  // firing a stream cancel.
  const sessions = useAppStore((s) => s.sessions);
  const subagentConversations = useStore(
    sessionConversationsStore,
    (s) => s.conversations
  );
  const setSubagentPanelOpen = useAppStore((s) => s.setSubagentPanelOpen);
  const [subagentOpenIndex, setSubagentOpenIndex] = useState<number | null>(
    null
  );
  const [subagentScrollOffset, setSubagentScrollOffset] = useState(0);
  // Git branch in the status footer: sync at mount, then async on each turn
  // boundary (catches branch changes the agent made). Async so a slow
  // `git rev-parse` can't stall a render.
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
  // Auto-follow: panel stays pinned to the latest trace line; disabled when the
  // user scrolls up, re-enabled at the floor (panel reports totalLines).
  const [subagentFollowBottom, setSubagentFollowBottom] = useState(true);
  const [subagentTotalLines, setSubagentTotalLines] = useState(0);
  useEffect(() => {
    setSubagentPanelOpen(subagentOpenIndex != null);
  }, [subagentOpenIndex, setSubagentPanelOpen]);

  // Per-session kill ladder: first Ctrl+X arms the focused subagent's
  // sessionId, second within 2s invokes terminateSession + cleanup. State lives
  // at the LAYOUT level (not the panel) because the kill side-effects need kiro,
  // sessionConversationsStore, and pendingApproval — all already here.
  const kiro = useAppStore((s) => s.kiro);
  const updateSession = useAppStore((s) => s.updateSession);
  const cleanupTerminatedSession = useAppStore(
    (s) => s.cleanupTerminatedSession
  );
  const cancelApproval = useAppStore((s) => s.cancelApproval);
  // Kill is V2-only; KAS (V3) session/terminate is a no-op, so don't offer it.
  const killSupported = engineSupportsSubagentKill(
    useAppStore((s) => s.agentEngine)
  );
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
  // cancelMessage (idempotent, so double-fire is safe) — belt-and-suspenders.
  const isProcessingRef = useRef(isProcessing);
  isProcessingRef.current = isProcessing;
  const pendingApprovalRef = useRef(pendingApproval);
  pendingApprovalRef.current = pendingApproval;
  const pendingQuestionRef = useRef(pendingQuestion);
  pendingQuestionRef.current = pendingQuestion;
  const activeCommandRef = useRef(activeCommand);
  activeCommandRef.current = activeCommand;
  const activeTriggerRef = useRef(activeTrigger);
  activeTriggerRef.current = activeTrigger;
  // Refs the always-armed cancel handler consults. Twinki fires every active
  // useKeypress on one keystroke (no ordering), so each surface that owns Esc
  // (subagent/backend panel, queue-edit, /prompts detail) must no-op Esc here
  // instead of cancelling the turn.
  const subagentOpenIndexRef = useRef(subagentOpenIndex);
  subagentOpenIndexRef.current = subagentOpenIndex;
  const anyPanelOpenRef = useRef(anyPanelOpen);
  anyPanelOpenRef.current = anyPanelOpen;
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
    // Backend panel / queue- or steer-edit own Esc — bail before the cancel
    // branch so Esc doesn't also abort the in-flight turn.
    if (anyPanelOpenRef.current && key.escape) {
      return;
    }
    if (isEditingEntry() && key.escape) {
      return;
    }
    if ((key.ctrl && input === 'c') || key.escape) {
      logger.debug('[lite] interrupt key', {
        ctrl: !!key.ctrl,
        esc: !!key.escape,
        input,
        isProcessing: isProcessingRef.current,
        pendingApproval: !!pendingApprovalRef.current,
        pendingQuestion: !!pendingQuestionRef.current,
      });
      if (
        isProcessingRef.current &&
        !pendingApprovalRef.current &&
        !pendingQuestionRef.current
      ) {
        cancelMessage();
        return;
      }
    }
    // Ctrl+C inside an open menu = Esc (CommandMenu closes it). Roll back
    // AppContainer's same-keystroke double-Ctrl+C exit increment so backing out
    // of a menu doesn't tick toward exit; queueMicrotask runs after it increments.
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

  // Ctrl+X expands the lite activity tray when it has workflow or task detail
  // to show. Keep retained terminal workflows collapsible until closing the
  // tray releases the workflow surface and pruning runs.
  useKeypress((input, key) => {
    if (!(key.ctrl && (input === 'x' || input === 'X'))) return;
    const hasVisibleTasks = getVerboseDisplay().showTasks && tasks.length > 0;
    const hasExpandableActivity =
      hasVisibleTasks ||
      liveWorkflowCount > 0 ||
      (activityTrayExpanded && retainedWorkflowCount > 0);
    if (!hasExpandableActivity) return;
    if (isEditingEntry()) return;
    if (pendingApprovalRef.current || pendingQuestionRef.current) return;
    if (anyPanelOpenRef.current) return;
    if (subagentOpenIndexRef.current != null) return;
    toggleActivityTray();
  });

  const interactionReady = useInteractionReady(
    pendingQuestion ?? pendingApproval
  );
  const showQuestion = interactionReady ? pendingQuestion : null;
  const showApproval =
    interactionReady && !pendingQuestion ? pendingApproval : null;
  const showInteraction =
    !!showApproval || !!showQuestion || !!voiceDownloadConfirm;
  const questionStageName =
    pendingQuestion?.sessionId && pendingQuestion.sessionId !== mainSessionId
      ? sessions.get(pendingQuestion.sessionId)?.name
      : undefined;

  // Emit "user interrupted" only once a cancelled turn has FULLY settled
  // (!isProcessing). wasCancelled flips true immediately, but appending while
  // the stream is still unwinding flushes a partial snapshot into <Static>.
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

  // Panel safety-net. Panel-type slash commands freeze the input (activeCommand
  // set + empty options → PromptInput bails). Lite renders only a curated subset
  // of panels; a command lite does NOT render would freeze the input dead.
  // Detect "unrenderable panel" = activeCommand set + empty options + no panel
  // open; the 600ms delay lets a real panel open first (which clears the timer),
  // else release the input.
  useEffect(() => {
    if (!activeCommand) return;
    if (activeCommand.options.length > 0) return; // selection picker — legit
    if (activeCommand.panel) return;
    if (anyPanelOpen) return; // a backend panel is up — legit freeze
    const t = setTimeout(() => {
      if (
        !activeCommandRef.current ||
        activeCommandRef.current.options.length > 0 ||
        activeCommandRef.current.panel ||
        anyPanelOpenRef.current
      )
        return;
      setActiveCommand(null);
      clearCommandInput();
    }, 600);
    return () => clearTimeout(t);
  }, [activeCommand, anyPanelOpen, setActiveCommand, clearCommandInput]);

  // Boot indicator: one dim row surfacing in-flight async setup (agent_connect >
  // session_create > MCP aggregate), hidden once nothing is 'loading'. Failure
  // detail lives elsewhere (McpServerInitFailure alert + /mcp); this only
  // answers "anything still loading?".
  const showBootIndicator = useMemo(() => {
    for (const info of bootProgress.values()) {
      if (info.status === 'loading') return true;
    }
    for (const info of mcpInitStatus.values()) {
      if (info.status === 'loading') return true;
    }
    return false;
  }, [bootProgress, mcpInitStatus]);

  // Cloud connect screen: while a cloud session is booting and the user has not
  // yet sent a prompt, show the milestone checklist in place of the single boot
  // row. False (inert) unless a cloud session is active — non-cloud startup
  // renders exactly as before. Rows are built at render time so the in-progress
  // spinner animates with bootFrame. Gated on "no user message yet" rather than
  // "no messages" so the cloud prefetch tool calls (fetch_cloud_config, repo
  // clone) that stream in during bring-up don't tear the checklist down before
  // the user has typed.
  const showCloudChecklist =
    cloudSessionActive &&
    bootProgress.has('agent_connect') &&
    !messages.some((m) => m.role === MessageRole.User);

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

  // KIRO welcome banner, shown while no real chat has happened this session.
  // `lite.welcomeEmitted`: cross-mount suppression flag, set true on unmount so a
  // lite→tui→lite swap doesn't re-flash the banner; resetMessages clears it.
  const liteWelcomeEmitted = useAppStore((s) => s.lite.welcomeEmitted);
  const setLiteWelcomeEmitted = useAppStore((s) => s.setLiteWelcomeEmitted);
  // Tip-eligibility signal for the shared startup-tip engine (tips/tips.ts).
  // The "Try Lite" tip is TUI-only and never appears here regardless of
  // recommendLiteUi.
  const tipRecommendLiteUi = useAppStore((s) => s.recommendLiteUi);
  const tipEngine = useAppStore((s) => s.agentEngine);
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
    // One rotating tip (weighted chance per launch) — see tips/tips.ts.
    const tip = pickTip({
      surface: 'lite',
      engine: tipEngine,
      recommendLiteUi: tipRecommendLiteUi,
    });
    const experimentalNotice = brand(
      '  Lite UI is currently an experimental feature.\n' +
        '  If you find any bugs or issues, please report it with /feedback'
    );
    let out = `${kiroArt}\n${chalk.dim(`  v${version} ${glyphs.smallDot} lite`)}\n${experimentalNotice}`;
    if (tip) out += `\n${formatTipLine(tip)}`;
    return out;
  }, [allowAsciiArt, glyphs, tipRecommendLiteUi, tipEngine]);
  // True until real chat content lands OR the banner already emitted in a prior
  // mount. "Real chat" = any NON-standalone-greeting message; gating on User
  // rows alone is too narrow (a System announcement before typing duplicated the
  // banner). Must mirror the visibleMessages filter exactly.
  const showWelcomeBanner = useMemo(
    () =>
      !liteWelcomeEmitted &&
      !messages.some((m) => !(m.role === MessageRole.Model && m.standalone)),
    [liteWelcomeEmitted, messages]
  );
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

  // Append-only <Static> invariant: Twinki's <Static> is a monotonic by-index
  // cursor that silently drops re-emissions for already-printed indices, so
  // staticItemsRef only ever grows (until lite.scrollbackClearToken resets it).
  // The delta walk clamps late eligibility shrinkage and dedups via
  // pushedStaticIdsRef for rows that flip eligible after a later row flushed.
  const staticItemsRef = useRef<Array<{ id: string; text: string }>>([]);
  // High-water mark into `eligible`: count already appended. Resets on clear.
  const lastFlushedEligibleCountRef = useRef(0);
  // Session ids the user explicitly killed via Ctrl+X — the backend fires
  // session_terminated for normal completion too, so a status check alone
  // would paint naturally-completed stages as red `✗ killed`.
  const userKilledSessionsRef = useRef<Set<string>>(new Set());
  // Ids already pushed: a Model row can flip eligible AFTER a later row was
  // flushed (shell-escape cancel race), re-pointing the delta walk at an
  // already-pushed row that would otherwise duplicate forever.
  const pushedStaticIdsRef = useRef<Set<string>>(new Set());
  const emittedSubagentSummaryKeysByParentRef = useRef<
    Map<string, Set<string>>
  >(new Map());
  // Last appended eligible msg — the next delta computes its leading blank
  // against this without re-walking the full eligible list.
  const lastAppendedEligibleMsgRef = useRef<MessageType | null>(null);

  // Session boundary (/chat new|<id>|load, /clear, /rewind, lite↔tui swap):
  // resetMessages/setUiMode bump lite.scrollbackClearToken. On a bump (or -1
  // first-mount init): (1) adjustStaticCursor(MAX) so the next paint lands at
  // index 0; (2) reset staticItemsRef + bookkeeping refs; (3) head-push the KIRO
  // banner when there's prior content (fresh sessions use the live banner so
  // resize reflows it).
  //
  // DO NOT write CSI 3J/2J — that wipes the whole terminal scrollback including
  // pre-kiro shell history. twinki's accumulatedStaticOutput is 10k-line bounded
  // so printing below is safe.
  //
  // Runs in the render body (not an effect): the staticItems memo reads these
  // refs synchronously, so a post-commit reset would re-commit the prior
  // session's rows on the first render after a bump.
  const liteScrollbackClearToken = useAppStore(
    (s) => s.lite.scrollbackClearToken
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
    emittedSubagentSummaryKeysByParentRef.current = new Map();
    lastAppendedEligibleMsgRef.current = null;
    // Two-armed gate for the swap-with-content anchor banner. User-arm:
    // tui→lite + /chat <id> load (messages carries prior chat).
    // hadPriorStaticContent-arm: /chat new (resetMessages empties messages
    // before the bump). Don't broaden User-arm to all non-greeting roles —
    // System slash announcements aren't chat and would re-emit the banner every
    // empty-session swap (pinned by integ `lite-welcome-banner-roundtrip`). The
    // id embeds the clear-token so each swap gets a fresh monotonic id.
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
      if (
        m.role === MessageRole.ToolUse &&
        isParentSubagentTool(m.name, m.origin)
      )
        return true;
    }
    return false;
  }, [messages]);

  // Static items: welcome + finalized messages, delta-appended (not rebuilt)
  // into the persistent staticItemsRef. Each item's text is baked once at flush
  // time.
  //
  // Resize caveat: this bakes `process.stdout.columns` into each row's text, so
  // a later resize leaves it stale — but the monotonic cursor ignores
  // re-emissions anyway. DO NOT add a resize redraw: it would be silently
  // dropped or scramble history. Terminal soft-wrap absorbs width changes.
  const staticItems = useMemo(() => {
    const items = staticItemsRef.current;

    // Slice the tui→lite bookmark out before selecting eligible rows (slice
    // preserves the tail for the "skip last streaming Model" rule).
    // Welcome-screen suppression: drop standalone-greeting rows (rendered
    // alongside the live banner) while showWelcomeBanner; they fall back into
    // eligible once the welcome screen ends.
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
    const display = getVerboseDisplay();
    const filtersOverride = getVerboseFilters();
    const hideThinkingContent = display.showThinkingContent === false;
    const eligible = selectStaticEligible(
      visibleMessages,
      isProcessing,
      activeToolBatchIds,
      agentName,
      hideThinkingContent
    );
    const subagentSummariesById: Map<string, SubagentStageSummary[]> =
      hasAnySubagentTool
        ? collectSubagentSummariesByParent(
            messages,
            sessions,
            subagentConversations,
            agentName
          )
        : new Map();
    const settledSubagentStagesById = hasAnySubagentTool
      ? collectSettledSubagentStagesByParent(messages, sessions, agentName)
      : new Map<string, Set<string>>();
    const staticSubagentSummariesById = new Map<
      string,
      SubagentStageSummary[]
    >();
    for (const [parentId, summaries] of subagentSummariesById) {
      const parent = messages.find(
        (
          msg
        ): msg is Extract<
          (typeof messages)[number],
          { role: MessageRole.ToolUse }
        > => msg.role === MessageRole.ToolUse && msg.id === parentId
      );
      staticSubagentSummariesById.set(
        parentId,
        parent
          ? selectReadySubagentSummaries(
              parent,
              summaries,
              settledSubagentStagesById.get(parentId)
            )
          : summaries
      );
    }

    // Guard against `eligible` shrinking below the high-water mark: the delta
    // walk assumes eligible only grows, but a message can flip OUT after being
    // counted (agentName reclassification, cleared content, thinking toggle),
    // stranding later rows. Clamp so the next walk starts valid (pushedStaticIds
    // dedups re-walked rows); warn for a breadcrumb.
    if (lastFlushedEligibleCountRef.current > eligible.length) {
      logger.warn('[lite] static high-water above eligible length — clamping', {
        highWater: lastFlushedEligibleCountRef.current,
        eligible: eligible.length,
        messages: messages.length,
        agentName,
      });
      lastFlushedEligibleCountRef.current = eligible.length;
    }
    // Hot-path bypass: nothing new to flush — skip the
    // renderCtx / subagent walk / theme build (most spinner re-renders land here).
    const haveNewEligible =
      eligible.length > lastFlushedEligibleCountRef.current;
    const pendingSubagentSummaryEntries = selectPendingSubagentSummaryEntries(
      messages,
      subagentSummariesById,
      pushedStaticIdsRef.current,
      emittedSubagentSummaryKeysByParentRef.current,
      display,
      filtersOverride,
      settledSubagentStagesById
    );
    const havePendingSubagentSummaryAppendix =
      pendingSubagentSummaryEntries.length > 0;
    if (!haveNewEligible && !havePendingSubagentSummaryAppendix) {
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

    const stageColor = (stageName: string) =>
      getAgentColor(stageName, getColor);
    const theme = buildRenderTheme(
      getColor,
      getUserPromptColor,
      getUserPromptBgHex
    );
    const renderCtx = {
      pendingApprovalToolCallId:
        pendingQuestion?.toolCallId ??
        pendingApproval?.toolCall.toolCallId ??
        null,
      termCols: process.stdout.columns ?? 80,
      subagentSummariesById: staticSubagentSummariesById,
      getStageInputColor: stageColor,
      getStageOutputColor: stageColor,
      // Same per-agent palette the footer uses for its agent chip — keeps
      // the scrollback role tag and the footer agent name in lockstep.
      getAgentTagColor: stageColor,
      theme,
      glyphs,
      display,
      filtersOverride,
      isStatic: true,
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
      const prefix =
        prevMsg !== null && needsLeadingBlank(prevMsg, msg) ? '\n' : '';
      const text =
        prefix + renderMessageToText(msg, agentName ?? undefined, renderCtx);
      items.push({ id: msg.id, text });
      pushedStaticIdsRef.current.add(msg.id);
      if (
        msg.role === MessageRole.ToolUse &&
        staticSubagentSummariesById.get(msg.id)?.length &&
        shouldRenderSubagentResponseSummaries(
          msg,
          display,
          staticSubagentSummariesById.get(msg.id) ?? [],
          filtersOverride
        )
      ) {
        markSubagentSummariesEmitted(
          msg.id,
          staticSubagentSummariesById.get(msg.id) ?? [],
          emittedSubagentSummaryKeysByParentRef.current
        );
      }
      prevMsg = msg;
    }
    lastFlushedEligibleCountRef.current = eligible.length;
    lastAppendedEligibleMsgRef.current = prevMsg;

    for (const appendix of renderPendingSubagentSummaryAppendices(
      pendingSubagentSummaryEntries,
      agentName,
      renderCtx
    )) {
      markSubagentSummariesEmitted(
        appendix.parentId,
        appendix.summaries,
        emittedSubagentSummaryKeysByParentRef.current
      );
      items.push({ id: appendix.id, text: appendix.text });
    }

    // Return a NEW array reference each render — twinki's <Static> compares
    // `items` by reference, so the same mutated array would be blind to the
    // newly appended entries. Shallow copy of pointers is cheap.
    return items.slice();
  }, [
    messages,
    isProcessing,
    agentName,
    activeToolBatchIds,
    pendingApproval,
    pendingQuestion,
    liteStaticSkipBefore,
    hasAnySubagentTool,
    sessions,
    subagentConversations,
    glyphs,
    getColor,
    getUserPromptColor,
    getUserPromptBgHex,
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

  // Flush a staged approval note to the model. ApprovalPrompt calls this just
  // BEFORE it sends the user's y/t/n disposition (see respondWithNote): the
  // note rides as a mid-turn steer, and the steer frame must reach the backend
  // ahead of the approval response so the deny path's drain consumes it on the
  // same request (otherwise it slips to end-of-turn — the "queued for
  // afterwards" bug). This must NOT cancel the approval (that would undo the
  // disposition and force a denial, the ORIGINAL bug). It only injects the
  // staged text as a user turn — the only channel that reaches the model, since
  // the backend approval response can't carry free text (v2
  // ApprovalResult.reason is ignored by handle_approval_result). The
  // empty-string guard lives in ApprovalPrompt (respondWithNote only calls this
  // when the trimmed note is non-empty).
  const handleNotesSubmit = useCallback(
    (value: string) => {
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

  const statusBilling = useStatusBilling('lite');
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

  const activeParentSubagentTools = useMemo(
    () => selectActiveSubagentToolScopes(messages),
    [messages]
  );

  const activePipelineGroupIds = useMemo(
    () => activePipelineGroupConstraint(activeParentSubagentTools),
    [activeParentSubagentTools]
  );

  const activeCrewSessions = useMemo(
    () =>
      selectSubagentToolSessions(sessions.values(), {
        pipelineGroupIds: activePipelineGroupIds,
      }),
    [sessions, activePipelineGroupIds]
  );

  const sessionGroupById = useMemo(
    () =>
      new Map(activeCrewSessions.map((session) => [session.id, session.group])),
    [activeCrewSessions]
  );

  // Active-subagents footer strip: one row per running stage in spawn order
  // with a per-stage phase (running → summarizing → complete). When every stage
  // is complete but the parent `subagent` tool is still in flight, a
  // "Summarizing N agents..." footnote shows the finalize phase.
  const { activeSubagents, summarizingPhase } = useMemo<{
    activeSubagents: ActiveSubagentRow[];
    summarizingPhase: boolean;
  }>(() => {
    if (!isProcessing) return { activeSubagents: [], summarizingPhase: false };
    // No pipeline this session — skip the walks (common short-turn case).
    if (!hasAnySubagentTool)
      return { activeSubagents: [], summarizingPhase: false };

    // Strip is empty unless a parent subagent is in flight — bail early.
    if (activeParentSubagentTools.length === 0)
      return { activeSubagents: [], summarizingPhase: false };

    // Toolcall id for the current approval (if any) — flips the requesting
    // stage's row to "blocked on user" instead of a misleading "running".
    const approvalToolCallId = pendingApproval?.toolCall.toolCallId ?? null;
    const rows: ActiveSubagentRow[] = [];

    // Build each invocation independently. Pipeline events can interleave, and
    // stage names can repeat across invocations, so neither message position
    // nor agent name alone is a safe ownership key.
    for (const scope of activeParentSubagentTools) {
      const byStage = new Map<string, ActiveSubagentRow>();
      const order: string[] = [];

      // Seed rows from sessions FIRST (in spawn order): the ACP session exists
      // at spawn, but a stage only emits ToolUse after its first tool call.
      for (const session of selectScopeSeedSessions(
        scope,
        activeCrewSessions
      )) {
        const stageName = session.name;
        if (!stageName || stageName === agentName) continue;
        if (byStage.has(stageName)) continue;
        order.push(stageName);
        byStage.set(stageName, {
          key: `${scope.key}::${stageName}`,
          name: stageName,
          sessionId: session.id,
          // `killed` reflects explicit user kills ONLY. The backend also
          // terminates sessions on normal completion.
          phase: userKilledSessionsRef.current.has(session.id)
            ? 'killed'
            : 'running',
          activeToolName: null,
          activeToolDetail: null,
          activeToolFinished: false,
        });
      }
      for (const message of selectSubagentToolMessagesForScope(
        messages,
        scope,
        sessionGroupById
      )) {
        if (!message.agentName || message.agentName === agentName) continue;
        if (!byStage.has(message.agentName)) {
          order.push(message.agentName);
          byStage.set(message.agentName, {
            key: `${scope.key}::${message.agentName}`,
            name: message.agentName,
            sessionId: message.sessionId,
            phase: 'running',
            activeToolName: null,
            activeToolDetail: null,
            activeToolFinished: false,
          });
        }
        const row = byStage.get(message.agentName)!;
        if (!row.sessionId && message.sessionId) {
          row.sessionId = message.sessionId;
        }
        // Complete + killed are terminal; later messages cannot downgrade them.
        if (row.phase === 'complete' || row.phase === 'killed') continue;
        if (isSubagentSummaryToolName(message.name)) {
          row.phase = message.isFinished ? 'complete' : 'summarizing';
          row.activeToolName = null;
          row.activeToolDetail = null;
          row.activeToolFinished = !!message.isFinished;
          continue;
        }
        if (row.phase === 'summarizing') continue;
        row.activeToolName = toolDisplayName(
          message.name,
          message.kind,
          message.origin
        );
        row.activeToolDetail = extractFooterToolDetail(
          message.name,
          message.content,
          message.kind,
          message.origin
        );
        row.activeToolFinished = !!message.isFinished;
        if (approvalToolCallId && message.id === approvalToolCallId) {
          row.phase = 'requesting-permission';
        }
      }

      rows.push(...order.map((name) => byStage.get(name)!));
    }

    // Parent finalize phase: every known stage is complete but the parent tool
    // is still concatenating their summaries. Surface one footnote.
    const summarizingPhase =
      rows.length > 0 && rows.every((r) => r.phase === 'complete');

    return { activeSubagents: rows, summarizingPhase };
  }, [
    messages,
    isProcessing,
    agentName,
    activeCrewSessions,
    activeParentSubagentTools,
    hasAnySubagentTool,
    pendingApproval,
    sessionGroupById,
  ]);

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
  const subagentRequestingTarget = useMemo<{
    name: string;
    sessionId?: string;
  } | null>(() => {
    if (!pendingApproval) return null;
    const id = pendingApproval.toolCall.toolCallId;
    if (!id) return null;
    const msg = messages.find(
      (m) => m.role === MessageRole.ToolUse && m.id === id
    );
    if (!msg || msg.role !== MessageRole.ToolUse) return null;
    if (!msg.agentName) return null;
    if (msg.agentName === agentName) return null;
    return { name: msg.agentName, sessionId: msg.sessionId };
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
    if (!subagentRequestingTarget) {
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
    // Prefer the owning session id because concurrent invocations may reuse a
    // stage name. Fall back to the name for the pre-registration race.
    const targetIdx = activeSubagents.findIndex(
      (subagent) =>
        (subagentRequestingTarget.sessionId !== undefined &&
          subagent.sessionId === subagentRequestingTarget.sessionId) ||
        (subagentRequestingTarget.sessionId === undefined &&
          subagent.name === subagentRequestingTarget.name)
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
  }, [subagentRequestingTarget, pendingApproval, activeSubagents]);

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
      if (!killSupported) return;
      const focused = activeSubagents[subagentOpenIndex];
      // Bail on terminal phases — re-firing terminate on a dead session would
      // race the prior kill's local cleanup.
      if (
        !focused ||
        focused.phase === 'complete' ||
        focused.phase === 'killed'
      )
        return;
      const sessionId = focused.sessionId;
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

  const cloudSessionCreated =
    bootProgress.get('session_create')?.status === 'ready';
  const cloudSessionFailed =
    bootProgress.get('session_create')?.status === 'failed';

  // A cloud session's chat UI is unusable until the session is created and
  // linked, so while it is still being created (or creation failed) render only
  // the connect screen — the milestone checklist plus any error — and suppress
  // the scrollback, prompt, and footer. Dark-safe: a non-cloud session never
  // enters this branch, so its startup is unchanged.
  // Source-provider gate takes the whole screen — no welcome/checklist behind
  // it — until the provider is verified (session not created yet).
  if (cloudSessionActive && showSourceProviderGate) {
    return (
      <SourceProviderGate
        setupUrl={sourceProviderSetupUrl ?? null}
        onOpenBrowser={() => {
          openUrlInBrowser(sourceProviderSetupUrl ?? SOURCE_PROVIDER_SETUP_URL);
        }}
        onRetry={handlers.handleSourceProviderRetry}
        onQuit={handlers.handleSourceProviderQuit}
      />
    );
  }

  if (cloudSessionActive && !cloudSessionCreated) {
    return (
      <Box flexDirection="column">
        {agentError && <Text>{chalk.red(`error: ${agentError}`)}</Text>}
        {/* Connecting phase shows only the spinner. Welcome + checklist
            wait for the source-provider probe (cloudProviderChecked) so neither
            flashes before the 2.1 gate can appear. */}
        {cloudProviderChecked &&
          bootProgress.get('agent_connect')?.status === 'ready' && (
            <Box marginBottom={1}>
              <Text>{welcomeBannerText}</Text>
            </Box>
          )}
        {cloudConnectStage(
          cloudProviderChecked,
          bootProgress.get('agent_connect')?.status
        ) === 'connecting' && (
          <Text>
            {chalk.dim(
              `  ${
                spinners.brailleRotate[
                  bootFrame % spinners.brailleRotate.length
                ]
              } Connecting to kiro.dev${glyphs.ellipsis}`
            )}
          </Text>
        )}
        {cloudConnectStage(
          cloudProviderChecked,
          bootProgress.get('agent_connect')?.status
        ) === 'failed' && (
          // Terminal failure row: a rejected connect leaves cloudProviderChecked
          // false, so without this the spinner above would spin forever beside
          // the error line. No spinner — the connect is not still in flight.
          <Text>
            {chalk.red(`  ${glyphs.cross} Couldn't connect to kiro.dev`)}
          </Text>
        )}
        {cloudProviderChecked &&
          formatCloudStartupChecklist(
            {
              connected: bootProgress.get('agent_connect')?.status === 'ready',
              sessionCreated: false,
              sessionFailed: cloudSessionFailed,
              resumed: cloudSessionResumed,
              provider: cloudProvider ?? undefined,
              repoCount: cloudRepoCount ?? undefined,
            },
            {
              check: glyphs.checkmark,
              cross: glyphs.cross,
              ellipsis: glyphs.ellipsis,
              spinner:
                spinners.brailleRotate[
                  bootFrame % spinners.brailleRotate.length
                ]!,
            }
          ).map((line, i) => <Text key={i}>{line}</Text>)}
      </Box>
    );
  }

  return (
    <Box flexDirection="column">
      {/* Scrollback: append-only. wrap="overflow" soft-wraps so copy-paste
          keeps logical lines (wideLines enabled at index.tsx). */}
      <Static items={staticItems}>
        {(item) => (
          <Text key={item.id} wrap="overflow">
            {item.text}
          </Text>
        )}
      </Static>

      {/* Outside <Static> so resize reflows it. The length === 0 gate keeps it
          exclusive with the swap-with-content static push above (no double
          KIRO art). */}
      {showWelcomeBanner && staticItemsRef.current.length === 0 && (
        <Text wrap="overflow">{welcomeBannerText}</Text>
      )}

      {/* Agent's standalone greeting alongside the banner; commits to <Static>
          via the fallback path once the welcome screen ends. */}
      {showWelcomeBanner && welcomeGreetingText && (
        <Text wrap="overflow">{welcomeGreetingText}</Text>
      )}

      {agentError && <Text>{chalk.red(`error: ${agentError}`)}</Text>}
      <LiteLiveRegion />

      <ArtifactGenerationCard />
      {surveyPrompt && (
        <SurveyPromptBar
          message={surveyPrompt.message}
          onDismiss={dismissSurveyPrompt}
        />
      )}

      {/* Queued messages — preview rows only (full text lives in the store).
          previewLine cap + truncate-end bound each row by width; rendering full
          text here hung the UI on multi-KB paste. Hidden during shell escape. */}
      {unifiedQueueEntries.length > 0 && !isShellEscape && (
        <Box flexDirection="column">
          {unifiedQueueEntries.map((entry, displayIndex) => {
            const editing =
              entry.kind === 'queue'
                ? editingQueueIndex === entry.queueIndex
                : editingSteerLineIndex === displayIndex;
            const marker = editing ? chalk.cyan(`${glyphs.chevron} `) : '  ';
            // Reserve cols for marker + index prefix so a wide preview can't
            // overrun the terminal edge before truncate-end kicks in.
            const cols = process.stdout.columns ?? 80;
            const previewWidth = Math.max(20, cols - 8);
            const preview = previewLine(entry.text, previewWidth);
            // Steer entries get a leading marker so they read as "in flight"
            // (injected mid-turn) vs. the plain queue order below.
            const label =
              entry.kind === 'steer'
                ? `${displayIndex + 1}. (steer) ${preview}`
                : `${displayIndex + 1}. ${preview}`;
            const body = editing ? chalk.cyan(label) : chalk.dim(label);
            return (
              <Text key={displayIndex} wrap="truncate-end">
                {marker}
                {body}
              </Text>
            );
          })}
        </Box>
      )}

      {loadingMessage && (
        <Text>
          {chalk.dim(
            `  ${spinners.brailleRotate[bootFrame % spinners.brailleRotate.length]} ${loadingMessage}`
          )}
        </Text>
      )}
      <ActivityTray />
      <Divider />

      {/* Status line (Divider → header → input order). Hidden while connecting
          and during shell escape (agent isn't running). Pending agent swap
          renders the requested name with a spinner. */}
      {isInitialized && !isShellEscape && (
        <StatusLine
          {...statusSurface}
          agentName={agentName}
          autonomousModeActive={isAutonomousAgent(agentName)}
          modelName={modelName}
          contextUsagePercent={ctxPct}
          gitBranch={gitBranch}
          pendingAgentName={pendingAgentName}
          animationFrame={bootFrame}
          usagePercent={statusBilling.usagePercent}
          creditsRemaining={statusBilling.creditsRemaining}
        />
      )}

      {/* Boot indicator — inline (not memoized) so it ticks every bootFrame.
          For a cloud session's connect screen, the milestone checklist takes
          its place. */}
      {showCloudChecklist ? (
        <Box flexDirection="column">
          {formatCloudStartupChecklist(
            {
              connected: bootProgress.get('agent_connect')?.status === 'ready',
              sessionCreated:
                bootProgress.get('session_create')?.status === 'ready',
              resumed: cloudSessionResumed,
              provider: cloudProvider ?? undefined,
              repoCount: cloudRepoCount ?? undefined,
            },
            {
              check: glyphs.checkmark,
              cross: glyphs.cross,
              ellipsis: glyphs.ellipsis,
              spinner:
                spinners.brailleRotate[
                  bootFrame % spinners.brailleRotate.length
                ]!,
            }
          ).map((line, i) => (
            <Text key={i}>{line}</Text>
          ))}
        </Box>
      ) : (
        showBootIndicator && (
          <Text>
            {formatBootIndicator(
              selectBootIndicatorPhase(bootProgress, mcpInitStatus),
              spinners.brailleRotate[
                bootFrame % spinners.brailleRotate.length
              ]!,
              glyphs.ellipsis
            )}
          </Text>
        )
      )}

      {specDescriptionFeature && (
        <SpecDescriptionIntro featureName={specDescriptionFeature} />
      )}

      {showQuestion && (
        <Question
          key={`${showQuestion.sessionId}:${showQuestion.toolCallId}`}
          question={showQuestion.question}
          options={checkpointOptions(showQuestion.options)}
          onAnswer={(answer, answerForAgent) =>
            answerCheckpoint(answer, answerForAgent, showQuestion)
          }
          onCancel={() => void cancelMessage()}
          titlePrefix={
            questionStageName ? `${questionStageName} > ` : undefined
          }
        />
      )}

      {/* Approval prompt — inside the input area. No marginTop so the divider
          and status line don't jump when it mounts/unmounts. */}
      {!showQuestion && showApproval && (
        <Box flexDirection="column">
          <ApprovalPrompt
            messages={messages}
            approval={showApproval}
            respondToApproval={respondToApproval}
            getStageInputColor={(stageName: string) =>
              getAgentColor(stageName, getColor)
            }
            mainAgentName={agentName}
            onInputSubmit={handleNotesSubmit}
          />
        </Box>
      )}

      {/* First-use voice model download confirm gate. Owns the keyboard (y/n,
          Enter/Esc) in place of the prompt input, same as Question/Approval. */}
      {!showQuestion && !showApproval && voiceDownloadConfirm && (
        <Box flexDirection="column">
          <VoiceModelDownloadGate
            info={voiceDownloadConfirm.info}
            onConfirm={voiceDownloadConfirm.onConfirm}
            onDecline={voiceDownloadConfirm.onDecline}
          />
        </Box>
      )}

      {/* Backend-driven panels (/context, /mcp, /help, ...) replace the input
          area while open. They own Esc via Panel.tsx; the always-armed handler
          short-circuits Esc when anyPanelOpen so it doesn't also cancel. */}
      {!showInteraction && anyPanelOpen && (
        <Box flexDirection="column">
          <BackendPanels handlers={handlers} surface="lite" />
        </Box>
      )}

      {!showInteraction && !anyPanelOpen && (
        <Box flexDirection="column">
          {isEditingEntry() && (
            <Text>
              {chalk.cyan(
                editingSteerLineIndex != null
                  ? `${glyphs.chevron} editing steer #${editingSteerLineIndex + 1}`
                  : `${glyphs.chevron} editing queued #${editingQueueIndex! + 1}`
              )}
              {chalk.dim(
                ` ${glyphs.smallDot} enter saves ${glyphs.smallDot} ctrl+x deletes ${glyphs.smallDot} esc cancels`
              )}
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
          {/* Input row. width="100%" + inner flexShrink={1} are load-bearing for
              multi-line wrap (else Yoga sizes the row to the `> ` glyph and Text
              has nothing to wrap against). Hidden while /verbosity is active. */}
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
                  placeholder={getPlaceholder({
                    glyphs,
                    editingQueueIndex,
                    pendingApproval: !!pendingApproval || !!pendingQuestion,
                    isShellEscape,
                    isProcessing,
                    isInitialized,
                    pendingSteerContent,
                    activeInterruptMode,
                    toggleHintLabel: keybindings.label('toggleInterruptMode'),
                    agentName: currentAgent?.name,
                    specDescriptionFeature,
                    goalStatus,
                    tangentName,
                    cancelLabel: keybindings.label('cancelStream'),
                  })}
                  suppressArrows={subagentOpenIndex != null}
                />
              </Box>
            </Box>
          )}
          {/* Slash-command dropdown + /settings menu render BELOW the input
              (matches the TUI). */}
          <CommandMenu />
          {exitSequence > 0 && (
            <Text>{chalk.dim('Press Ctrl+C or Ctrl+D again to exit')}</Text>
          )}
        </Box>
      )}

      {/* One row per active stage; Ctrl+O expands the focused row into a
          fixed-height trace panel. */}
      {activeSubagents.length > 0 &&
        !pendingQuestion &&
        (() => {
          const visible = activeSubagents;
          const cols = Math.max(40, process.stdout.columns ?? 80);
          const completedCount = activeSubagents.filter(
            (r) => r.phase === 'complete'
          ).length;
          const openIdx = subagentOpenIndex;
          const focused = openIdx != null ? activeSubagents[openIdx] : null;
          const focusedSessionId = focused?.sessionId ?? null;
          // PANEL_LINES here MUST match the keypress handler's constant so the
          // floor-detection math stays consistent.
          return (
            <Box flexDirection="column">
              <Text> </Text>
              {visible.map((sub, i) => {
                if (openIdx === i && focused && focusedSessionId) {
                  return (
                    <LiteSubagentPanel
                      key={`panel-${sub.key}`}
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
                      canKill={killSupported}
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
                    <Text key={`panel-${sub.key}`}>
                      {chalk.dim(
                        `${glyphs.cornerTopLeft}${glyphs.lineHorizontal} `
                      )}
                      {tag}
                      {chalk.dim(' (no trace yet)')}
                    </Text>
                  );
                }
                return (
                  <Text key={sub.key}>
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
