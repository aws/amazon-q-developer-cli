/**
 * LiteLayout: Minimal append-only terminal UI with pinned footer.
 *
 * Architecture:
 * - <Static> for finalized messages (append-only scrollback)
 * - Live region = streaming + footer pinned at bottom
 * - Uses shared PromptInput for full keybinding/history/shell-escape support
 * - CommandMenu renders ABOVE input for slash command dropdown
 * - Compact tool call rendering with error tree
 *
 * Append-only contract: every entry pushed into `staticItems` is keyed by a
 * stable id and its text is computed once. The text is never recomputed for
 * an item that is already in `staticItems` — twinki's <Static> is a monotonic
 * by-index cursor, so any in-place edit to an existing line drops the line
 * silently. To keep that invariant we:
 *   1. Hold a tool-call BATCH (run of consecutive ToolUse messages) out of
 *      static until the whole batch is settled (every tool finished or the
 *      next non-tool message has arrived). While in flight, the batch renders
 *      in <LiteLiveRegion> in creation order with only the spinner mutating.
 *   2. Bake any leading separator (`'\n'`) into the item text once at flush
 *      time, never as a function of neighboring items computed at render time.
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

// Last `liteScrollbackClearToken` this module observed. MODULE-LEVEL (not a
// per-mount ref) so it survives the unmount/remount cycle that happens when
// `mode` toggles (Ctrl+G crew monitor, expanded, session-view) WITHOUT a
// session/mode reset. Twinki's ReactBridge owns a single monotonic
// `totalStaticWritten` cursor AND its `accumulatedStaticOutput` buffer, both
// of which persist across LiteLayout remounts. If a bare remount re-ran the
// clear-token reset block below, it would call adjustStaticCursor(MAX) and
// wipe staticItemsRef while the bridge buffer still held the prior rows —
// re-emitting every row on top of the preserved buffer (duplicate
// scrollback) or, with the cursor reset ahead of the rebuilt array, swallowing
// newly-appended rows (they flash in the live region then never persist).
// Only genuine resets (resetMessages, setUiMode) bump the token, so keying the
// reset on a module-level value fires it exactly when the array is truly
// cleared and skips it on bare remounts. ConversationView uses the same
// module-level pattern (`_lastObservedClearToken`) for the same reason. The -1
// init still fires the block once on the first mount of the process.
let _liteLastObservedClearToken = -1;

export const LiteLayout: React.FC = () => {
  const store = useContext(AppStoreContext);
  const messages = useAppStore((s) => s.messages);
  const isProcessing = useAppStore((s) => s.isProcessing);
  // Shell-escape (`!command`) flag. When true, the in-flight turn is a PTY-
  // backed bash command, not agent inference. Used to swap the `> ` prompt
  // glyph for a brand-purple `! ` and suppress agent-mode chrome (status
  // line, queued-messages strip, boot indicator) while the user interacts
  // with bash. The live region also collapses to a single shell-output row
  // — see LiteLiveRegion's shell-escape branch. Keystroke forwarding to
  // the PTY is wired up at AppContainer's always-armed handler; this flag
  // is purely for visual mode.
  const isShellEscape = useAppStore((s) => s.isShellEscape);
  // Bookmark used for two skip-on-render cases (see app-store comment for
  // the full contract):
  //   1. tui→lite swap → set to messages.length so the modern TUI's
  //      already-rendered scrollback isn't duplicated in lite style below.
  //   2. Session resume → set to max(0, messages.length -
  //      LITE_HISTORY_RENDER_CAP) so a long resumed history doesn't dump
  //      hundreds of replay rows into <Static>.
  // 0 on fresh cold boot and lite→tui→lite cycles.
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
  // Tasks tray (Ctrl+X). `tasks` is populated by the agent's todo_list/task
  // tool calls via `extractTaskState` in app-store regardless of UI mode, so
  // lite gets the same data the modern TUI's <ActivityTray /> consumes.
  const tasks = useAppStore((s) => s.tasks);
  const toggleActivityTray = useAppStore((s) => s.toggleActivityTray);
  const setActiveTrigger = useAppStore((s) => s.setActiveTrigger);
  const activeTrigger = useAppStore((s) => s.activeTrigger);
  const activeCommand = useAppStore((s) => s.activeCommand);
  const setActiveCommand = useAppStore((s) => s.setActiveCommand);
  const clearCommandInput = useAppStore((s) => s.clearCommandInput);
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
  // Accessibility wiring (1:1 with modern TUI):
  //   - useGlyphs / useSpinners — switch box-drawing chars + spinner frame
  //     sets between Unicode and ASCII based on /settings allowAsciiArt.
  //   - useAllowAsciiArt — gates the KIRO ASCII banner. When false, the
  //     banner is suppressed entirely (replaced by a single-line text
  //     marker), matching how the modern TUI's <Wordmark/> handles its
  //     braille art in ASCII mode.
  //   - useAnimationPaused — true when /settings allowAnimations is off.
  //     Used to skip the boot-frame setInterval so users with motion-
  //     sensitive setups don't see the spinner cycling.
  const glyphs = useGlyphs();
  const spinners = useSpinners();
  const { allowAsciiArt } = useAllowAsciiArt();
  const animationPaused = useAnimationPaused();

  // Panel state — backend-driven slash commands flip these flags via effects
  // (see commands/effects.ts). The shared <BackendPanels /> component reads
  // its own data from the store; we only need the show-flags here so we can
  // build anyPanelOpen, which drives the input-area swap below and gates the
  // always-armed Esc/Ctrl+C handler (Esc closes the panel via its own
  // useInput; we must not also fire cancelMessage).
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
    artifactViewOpen,
  } = useUIState();
  const showSurveyPanel = useAppStore((s) => s.showSurveyPanel);
  const surveyPrompt = useAppStore((s) => s.surveyPrompt);
  const dismissSurveyPrompt = useAppStore((s) => s.dismissSurveyPrompt);
  const currentEffort = useAppStore((s) => s.currentEffort);
  // Goal-driven loop state (set by `/goal`). Lite surfaces it three ways: a
  // status-line segment (below), a one-time scrollback confirmation when a
  // goal is set, and a transient alert on bare `/goal` (which has no lite
  // panel). The store is updated from the GoalStatus ACP event regardless of
  // UI mode, so the data is already here — lite just never read it before.
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
    !!artifactViewOpen ||
    showSurveyPanel;

  const pendingSwap = usePendingSwap();
  const pendingAgentName = pendingSwap?.name ?? null;

  // Subagent inline-trace panel (Ctrl+O). subagentOpenIndex tracks which
  // subagent in `activeSubagents` is being inspected; null = panel closed.
  // Scroll offset is reset whenever we cycle to a different subagent.
  // The boolean is mirrored into app-store so AppContainer's top-level
  // dispatch can stop Esc from also firing as a stream cancel.
  const sessions = useAppStore((s) => s.sessions);
  const setSubagentPanelOpen = useAppStore((s) => s.setSubagentPanelOpen);
  const [subagentOpenIndex, setSubagentOpenIndex] = useState<number | null>(
    null
  );
  const [subagentScrollOffset, setSubagentScrollOffset] = useState(0);
  // Git branch in the status footer. Captured sync at mount so first paint
  // has the right value, then refreshed async on every turn boundary
  // (`isProcessing` transitions from true → false). The turn boundary is
  // the moment the user comes back to the prompt to plan the next step,
  // and it naturally catches branch changes the agent itself just made
  // (the most common case in this CLI). Async refresh so a slow
  // `git rev-parse` (NFS home, large repo, cold fs cache) can't stall a
  // render for up to a second. We deliberately don't poll mid-turn — the
  // user can't see the footer while the live region is busy painting, and
  // bouncing `isProcessing` mid-turn is rare enough that a single refresh
  // per turn-end transition is correct.
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

  // Per-session kill ladder. First Ctrl+X press while the panel is open
  // arms the kill against the focused subagent's sessionId; second press
  // within 2s actually invokes terminateSession + cleanup. Mirrors the
  // modern TUI CrewMonitorLayout pattern so users carry the same muscle
  // memory across modes. State lives at the LAYOUT level (not inside
  // LiteSubagentPanel) because the kill side-effects need kiro,
  // sessionConversationsStore, and pendingApproval — all of which already
  // live here. The panel just receives a presentational `armedToKill`
  // prop and reflects it in its header/hint.
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
  // Disarm + clear timer. Called on the second press (after kill fires),
  // on the 2s timeout, on panel close, on cycling to a different subagent,
  // and when the focused stage transitions out of the killable set.
  const disarmKill = useCallback(() => {
    if (armedKillTimerRef.current) {
      clearTimeout(armedKillTimerRef.current);
      armedKillTimerRef.current = null;
    }
    setArmedKillSessionId(null);
  }, []);
  // Clear the timer on unmount so a delayed setTimeout can't resolve into
  // setState after this layout has been swapped out (e.g. lite→tui swap
  // mid-arm).
  useEffect(
    () => () => {
      if (armedKillTimerRef.current) clearTimeout(armedKillTimerRef.current);
    },
    []
  );

  // Always-armed Ctrl+C / Escape interrupt. We *also* call cancelMessage from
  // AppContainer's handler — the store's cancelMessage is now idempotent
  // (early-returns when cancelInProgress is already set), so double-fire is
  // safe. Belt and suspenders: lite has had a string of issues where one
  // handler or the other misses the keypress at exactly the wrong moment.
  const isProcessingRef = useRef(isProcessing);
  isProcessingRef.current = isProcessing;
  const pendingApprovalRef = useRef(pendingApproval);
  pendingApprovalRef.current = pendingApproval;
  const activeCommandRef = useRef(activeCommand);
  activeCommandRef.current = activeCommand;
  const activeTriggerRef = useRef(activeTrigger);
  activeTriggerRef.current = activeTrigger;
  // Mirror the subagent panel state so the always-armed cancel handler can
  // skip Esc when the panel handler claimed it (Esc closes the panel, not
  // the agent). Twinki fires every active useKeypress on the same keystroke
  // so we can't rely on handler order — both handlers see the keypress.
  const subagentOpenIndexRef = useRef(subagentOpenIndex);
  subagentOpenIndexRef.current = subagentOpenIndex;
  // Same problem with backend-panel Esc: Panel.tsx's useInput closes itself
  // on Esc, but our always-armed handler also sees the same keystroke and
  // would call cancelMessage() if a turn was in flight. Mirror the open-
  // state into a ref so the handler can no-op Esc while a panel is up.
  const anyPanelOpenRef = useRef(anyPanelOpen);
  anyPanelOpenRef.current = anyPanelOpen;
  // Mirror goalStatus so the debounced panel safety-net timeout reads the
  // latest value without re-arming the timer on every iteration tick.
  const goalStatusRef = useRef(goalStatus);
  goalStatusRef.current = goalStatus;
  // Same idea for queue-restore editing: PromptInput's Esc handler exits
  // restore mode (it returns its own buffer to empty), but the always-armed
  // handler below would also see Esc and call cancelMessage() if a turn was
  // in flight — interrupting the agent the user didn't ask to interrupt.
  const editingQueueIndexRef = useRef(editingQueueIndex);
  editingQueueIndexRef.current = editingQueueIndex;
  // /prompts has its own picker→detail toggle inside PromptsMenu. PromptDetails'
  // Esc collapses the detail back to the picker; without this guard the
  // always-armed handler also fires setActiveCommand(null) on the same keystroke
  // and the user falls out of /prompts entirely.
  const promptDetailOpen = useAppStore((s) => s.promptDetailOpen);
  const promptDetailOpenRef = useRef(promptDetailOpen);
  promptDetailOpenRef.current = promptDetailOpen;

  useKeypress((input, key) => {
    // While the subagent panel is open, Esc and Ctrl+O belong to the panel.
    // Ctrl+C still cancels the agent (the panel doesn't claim it) so users
    // aren't trapped if they actually want to interrupt.
    if (subagentOpenIndexRef.current != null) {
      if (key.escape) return;
      if (key.ctrl && (input === 'o' || input === 'O')) return;
    }
    // While a backend panel is open, Esc closes the panel (handled by
    // Panel.tsx). Bail before the cancel branch so we don't also abort
    // the in-flight agent turn that originally produced the panel data.
    if (anyPanelOpenRef.current && key.escape) {
      return;
    }
    // Editing a queued message: Esc only abandons the edit, never the
    // running session. PromptInput's own Esc handler clears the input and
    // unsets editingQueueIndex.
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

  // Ctrl+X — toggle the lite task tray expanded/collapsed. Mirrors the
  // modern TUI's <ActivityTray /> binding. Gated on tasks.length > 0 so we
  // don't claim the keystroke when there's nothing to show; bails out for
  // queue-edit / approval / panel modes that have their own meaning for
  // Ctrl+X (or for keystrokes the user expects to land on a different
  // surface). PromptInput's own Ctrl+X handler is gated on
  // queueRestoreRef.current, so this guard mirrors that condition exactly
  // — when editingQueueIndex is set, PromptInput owns Ctrl+X and we no-op.
  useKeypress((input, key) => {
    if (!(key.ctrl && (input === 'x' || input === 'X'))) return;
    if (tasks.length === 0) return;
    if (editingQueueIndexRef.current != null) return;
    if (pendingApprovalRef.current) return;
    if (anyPanelOpenRef.current) return;
    if (subagentOpenIndexRef.current != null) return;
    toggleActivityTray();
  });

  // Approval typing guard: defer showing approval until the user has been
  // idle for APPROVAL_IDLE_MS. The tracker is always active so a keystroke
  // typed *before* an approval arrives still counts — otherwise an approval
  // landing mid-typing would render immediately and the user's next char
  // (often y/t/n) would be intercepted as a response.
  //
  // Once the prompt is visible we deliberately do NOT flip it back to hidden
  // on further keystrokes — the user must respond to it. Only the boundary
  // effect below (on a fresh pendingApproval) can hide the prompt.
  const lastKeypressRef = useRef(0);
  const [approvalReady, setApprovalReady] = useState(true);
  const showApprovalRef = useRef(false);

  useKeypress(() => {
    // Skip while the approval prompt is visible — that keystroke is the user's
    // y/n/t response, not typing into the input. Counting it would treat
    // sequential approvals as "user is typing" and delay each follow-up by
    // APPROVAL_IDLE_MS.
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

  // Emit a "user interrupted" line into scrollback once a cancelled turn has
  // fully settled. wasCancelled flips true the instant cancelMessage() starts,
  // but the in-flight agent stream takes a beat to unwind — appending a System
  // message during that window inserts a non-tool entry after the still-live
  // agent message, which makes selectStaticEligible flush a snapshot of the
  // partial agent text. The live region keeps streaming until cancel lands,
  // and the partial message ends up rendered twice. Waiting for !isProcessing
  // means the agent message has finalized via its normal path first.
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

  // Auto-dismiss the transient alert. The store fires alerts (e.g. "Slash
  // commands can't be queued") via showTransientAlert; without an auto-hide
  // timer they'd stay pinned forever in lite. autoHideMs comes from the
  // caller — fall back to 4s if missing so we don't strand a no-timer alert.
  useEffect(() => {
    if (!transientAlert) return;
    const ms = transientAlert.autoHideMs ?? 4000;
    const t = setTimeout(() => dismissTransientAlert(), ms);
    return () => clearTimeout(t);
  }, [transientAlert, dismissTransientAlert]);

  // Panel safety-net. Panel-type slash commands freeze the input BY DESIGN:
  // the dispatcher sets `activeCommand` with empty `options` and PromptInput
  // bails (`if (activeCommand) return`) while it's set, so the panel can take
  // over the input area. Lite only renders a CURATED subset of panels (see
  // anyPanelOpen + BackendPanels); a panel command lite does NOT render —
  // today `/goal` — leaves `activeCommand` set with nothing on screen, so the
  // input freezes dead until the user mashes Esc. This releases it.
  //
  // Signal for "unrenderable panel": activeCommand set, empty options, and no
  // panel open. Selection pickers (/model, /agent) set NON-EMPTY options and
  // render a CommandMenu, so they're legitimately blocking and excluded by the
  // options check. The 600ms delay lets the executeCommand RPC + its effect
  // open a real panel first — handled panels flip anyPanelOpen well within the
  // window, which re-runs this effect and clears the timer via cleanup. If the
  // window elapses with still no panel, the command is unrenderable in lite →
  // release the input. (A pathologically slow handled-panel RPC that trips the
  // timer only clears activeCommand harmlessly; anyPanelOpen still gates the
  // input the moment the panel opens.)
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
      // bare `/goal` still gives feedback (the status-line segment only shows
      // the iteration, not the goal text). Also drop the orphaned showGoalPanel
      // flag so a later lite→tui swap doesn't auto-open the modern GoalPanel.
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

  // Scrollback confirmation when a goal is set. The modern TUI surfaces the
  // goal via its status-bar chip + GoalPanel; lite has neither, so without
  // this a `/goal <description>` just runs as a normal turn with no
  // acknowledgment that a goal loop is now active. Append a one-time System
  // line when a NEW goal appears. Dedup is by goal text (`message`) held in a
  // ref: goalStatus also updates every iteration (the count climbs) carrying
  // the same message, so keying on message fires the confirmation exactly once
  // per distinct goal.
  //
  // Partial-snapshot guard: appending a System row while the tail message is a
  // live streaming Model row would shove that row out of the "skip last
  // streaming model" carve-out in selectStaticEligible, flushing a partial
  // snapshot into <Static> (the same class of bug the cancel appender above
  // avoids). In the normal `/goal` flow the GoalStatus event lands before the
  // model starts streaming, so we append immediately; if a goal ever arrives
  // mid-stream we defer — the effect re-runs on the next messages change and
  // appends once the streaming row has settled.
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

  // Boot indicator visibility: a single dim row near the status line that
  // surfaces in-flight async setup — agent_connect, session_create, and
  // per-MCP load. The row picks the most relevant phase to display
  // (agent_connect > session_create > MCP aggregate) and disappears the
  // moment nothing is in 'loading' state.
  //
  // Why this instead of a multi-line connecting panel: the prior panel was
  // a blocker-shaped UI element (full bordered list with per-stage timers)
  // that conveyed "the session isn't ready yet" — but input has always been
  // queue-able pre-init via `handleUserInput`'s `!isInitialized` branch,
  // and MCPs load in parallel after `kiro.createSession()` resolves. The
  // panel also had two real bugs: (1) it stuck around for the duration of
  // the slowest MCP rather than its 5s grace, because `anyMcpLoading` kept
  // it alive past the grace window; and (2) `session_create`'s elapsed
  // timer measured only the createSession RPC, so the panel could show a
  // green ✓ "initializing workspace (10.7s)" while a 13s MCP was still
  // ticking below it — visually contradictory. A single-line dim row
  // avoids both: no per-stage timer to be misleading, and the row hides
  // automatically when its own phase settles.
  //
  // Failure detail isn't lost — `McpServerInitFailure` already fires a
  // transient alert (see app-store.ts handler) and `/mcp` shows the full
  // per-server status. The indicator's job is "is anything still loading?"
  // not "what failed?".
  const showBootIndicator = useMemo(() => {
    for (const info of bootProgress.values()) {
      if (info.status === 'loading') return true;
    }
    for (const info of mcpInitStatus.values()) {
      if (info.status === 'loading') return true;
    }
    return false;
  }, [bootProgress, mcpInitStatus]);

  // Boot tick — drives the cycling spinner glyph and elapsed (n.ns) counters.
  // 150ms matches LiteLiveRegion's spinner cadence. Also re-used by the
  // pending-agent footer chip; ticking while EITHER signal is live keeps a
  // single interval and avoids a second hook.
  const [bootFrame, setBootFrame] = useState(0);
  useEffect(() => {
    if (!showBootIndicator && !pendingAgentName && !loadingMessage) return;
    // Animation-paused: hold whatever frame the spinner currently shows
    // (last interval write) instead of cycling. Mirrors the modern TUI
    // Spinner's last-frame freeze pattern.
    if (animationPaused) return;
    const t = setInterval(() => setBootFrame((f) => f + 1), 150);
    return () => clearInterval(t);
  }, [showBootIndicator, pendingAgentName, loadingMessage, animationPaused]);

  // MCP failure surface: lite shows a single transient alert above the input
  // (from showTransientAlert in the McpServerInitFailure handler). We used to
  // also emit a scrollback line here, but that meant the same warning landed
  // twice — once near the input, once in scrollback — and the scrollback copy
  // stuck around forever even after the user opened /mcp. The transient alert
  // covers it once and auto-hides; the connecting panel still shows the
  // per-MCP failed status during init for users who want detail.

  // KIRO welcome banner — rendered as a live-region <Text> when the user
  // hasn't sent any messages yet AND the banner hasn't already been shown
  // in this session. The agent's greeting (a `standalone: true` Model
  // message added by `setCurrentAgent`) lands in `messages` before any
  // user input, so we explicitly skip those rows when deciding whether
  // the banner is "still relevant" — otherwise the banner would hide the
  // moment the agent registers, which is before the user has even seen
  // the welcome screen. The text is computed once per (allowAsciiArt)
  // toggle so the spinner-driven re-renders inside the live region don't
  // re-pick a new tip mid-session. ASCII mode degrades to single-line
  // "KIRO" matching how the modern TUI's <Wordmark/> handles its braille
  // art when /settings allowAsciiArt is off (or KIRO_ASCII_MODE=1) — same
  // accessibility contract.
  //
  // `liteWelcomeEmitted` is the cross-mount suppression flag — set to
  // true on unmount so a plain lite→tui→lite swap doesn't re-flash the
  // banner over a session the user has already greeted into. /chat new
  // (and any other resetMessages caller) flips it back to false so the
  // next mount can show the banner again as a session-boundary marker.
  const liteWelcomeEmitted = useAppStore((s) => s.liteWelcomeEmitted);
  const setLiteWelcomeEmitted = useAppStore((s) => s.setLiteWelcomeEmitted);
  // Flip the welcome flag on UNMOUNT, not first paint. Flipping mid-mount
  // would re-run the showWelcomeBanner memo and unmount the live banner
  // immediately. Doing it on unmount means the current mount keeps the
  // banner visible until the layout itself goes away, and only the *next*
  // remount of LiteLayout sees the new flag value and skips the banner.
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
    // One rotating "Did you know" tip — picked deterministically per day
    // so frequent restarts don't flicker between nudges. Surfaces features
    // that aren't obvious from the input prompt (truncation, /theme, trust
    // scoping, etc.).
    const tipLine = formatTipLine(pickTip());
    return `${kiroArt}\n${chalk.dim(`  v${version} · lite`)}\n${tipLine}`;
  }, [allowAsciiArt]);
  // True until either real chat content lands in `messages` this session
  // OR the banner has already been emitted in a prior mount of this
  // session. Drives the live-region welcome banner above (and gates the
  // standalone-greeting suppression in the staticItems memo). Recomputed
  // only when the inputs shift; the common case of "no chat yet" exits
  // the .some() at index 0.
  //
  // "Real chat content" = any message that is NOT a standalone agent
  // greeting (the `Model + standalone` row added by `setCurrentAgent`).
  // We can't gate on User messages alone: slash commands like /agent and
  // /model emit a System "Switched to agent: X" announcement before the
  // user has typed. Treating that System row as "user has chatted" would
  // be wrong, but treating it as "still on the welcome screen" is also
  // wrong — the static items memo flushes it to scrollback and pushes
  // the welcome banner above it as a session anchor (see the
  // `items.length === 0` push). With the live banner still mounted (its
  // gate was the now-too-narrow User-only check), the user saw TWO
  // banners — one from the live region, one freshly anchored in static.
  // Mirror the visibleMessages filter exactly: any non-greeting message
  // ends the welcome screen, the live banner unmounts, and the static
  // push owns the banner from there on.
  const showWelcomeBanner = useMemo(
    () =>
      !liteWelcomeEmitted &&
      !messages.some((m) => !(m.role === MessageRole.Model && m.standalone)),
    [liteWelcomeEmitted, messages]
  );
  // Agent/model names for display
  const agentName = currentAgent?.name || null;
  const modelName = currentModel?.name || currentModel?.id || null;

  // Welcome-screen greeting render. The agent's standalone welcome message
  // (added by `setCurrentAgent` when an `agent.welcomeMessage` is configured)
  // is filtered out of the static-eligible set while `showWelcomeBanner`
  // is true (see staticItems memo below) so the banner above it can stay
  // anchored at the top of the welcome screen. Without rendering the
  // greeting somewhere, the user wouldn't see it until they sent their
  // first message — at which point the filter releases and the greeting
  // commits to <Static>. Render it inline next to the banner so the user
  // sees the agent's "Hi, I'm <agent>..." text from the start.
  //
  // Memoized on the first standalone Model row + agentName + theming so
  // spinner-driven re-renders don't re-walk markdown / re-build the theme
  // each tick. Once the user types, `showWelcomeBanner` flips false and
  // the JSX gate below unmounts this row; the same greeting then commits
  // to static via the eligible-set fallback path on the next memo run.
  const welcomeGreetingText = useMemo(() => {
    if (!showWelcomeBanner) return '';
    // Walk from the tail so multi-swap sessions (user opens picker, picks A,
    // changes mind, picks B, all without typing) show B's greeting — not A's.
    // The static path renders each standalone in creation order once the user
    // types, so this is purely a welcome-screen disambiguation: pick the most
    // recent agent's greeting to match the agent name the status footer is
    // already showing.
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

  // Persistent <Static> items array. Built incrementally — each render only
  // APPENDS new items, never mutates prior entries. Twinki's <Static> is a
  // monotonic by-index cursor that silently drops re-emissions for already-
  // printed indices, so the array's prefix stays load-bearing forever.
  // Allocating a fresh array per chunk (the prior shape) was both wasteful
  // (500-msg session × per-chunk = 500 object allocations every 16ms) and
  // a Zustand-equality footgun: it forced every memo with deps including
  // staticItems to invalidate even when no row had changed.
  const staticItemsRef = useRef<Array<{ id: string; text: string }>>([]);
  // High-water mark into the `eligible` list — the count of finalized
  // messages already appended to staticItemsRef. Bumped after each delta
  // walk; resets to 0 on a clear-token bump.
  const lastFlushedEligibleCountRef = useRef(0);
  // Set of message ids that have already been pushed into staticItemsRef.
  // Belt-and-suspenders dedup against the delta-walk's index-based
  // assumption that the eligible PREFIX never changes between renders —
  // which can break when a Model message flips ineligible→eligible after
  // a later message (System, User) has already been flushed. Concrete
  // case: a `!` shell-escape command's empty-buffer Model row updates
  // its content AFTER cancelMessage flipped isProcessing=false and the
  // cancelArmedRef effect appended a `user interrupted` System row.
  // The eligible list grows by inserting the Model in the middle, but
  // the loop walks `eligible[lastFlushed..length)` which now points at
  // the System row again — pushing it a second time and giving React
  // a duplicate-key children pair forever (static is append-only).
  // Set membership is O(1) and bounded by lifetime message count.
  // Session ids the user explicitly killed via Ctrl+X. We track this locally
  // because the backend's session_terminated signal (and the resulting
  // status: 'terminated' flip in index.tsx) is also fired for normal
  // completion — when the orchestrated session task ends successfully, the
  // session falls out of subagent_list_update and the TUI marks it
  // terminated. The footer's "killed" phase needs to distinguish "user
  // killed it" from "stage finished naturally". Without this, normally-
  // completed stages render as red `✗ killed` chips even though the user
  // never pressed Ctrl+X — see the bug context in the byStage seed below.
  const userKilledSessionsRef = useRef<Set<string>>(new Set());
  const pushedStaticIdsRef = useRef<Set<string>>(new Set());
  // Per-turn summary trailers that have already been committed to
  // staticItemsRef. Once a trailer's turn-id is in this set, we never
  // re-emit it — preserves the monotonic <Static> cursor invariant even
  // across mode swaps and slash-command boundaries.
  const committedTurnSummariesRef = useRef<Set<string>>(new Set());
  // The User message id that opens the in-flight (or last) turn. Bumped
  // every time a User row is appended; the trailer for the previous turn
  // gets emitted at that boundary.
  const openTurnUserIdRef = useRef<string | null>(null);
  // Last message appended into staticItemsRef from the eligible walk.
  // Needed so the next delta append can compute the leading-blank prefix
  // against the prior row without re-walking the full eligible list.
  const lastAppendedEligibleMsgRef = useRef<MessageType | null>(null);

  // Session boundary on /chat new, /chat <id>, /chat load, /clear, /rewind,
  // fresh mount, and lite↔tui swaps. The store bumps
  // liteScrollbackClearToken from resetMessages() and setUiMode(); we
  // observe the bump (or any first render of a fresh mount, via the -1
  // sentinel below) and:
  //   (1) call adjustStaticCursor(MAX_SAFE_INTEGER) — clamped to 0 inside
  //       the bridge, drops the monotonic write cursor so the next paint
  //       can land at index 0 of the freshly-emptied items array.
  //   (2) reset staticItemsRef + the bookkeeping refs so the prior
  //       session's rows can't bleed into the new one.
  //   (3) emit the KIRO banner at the head of the new session when there
  //       are existing messages — a "now in lite" visual cue at the top
  //       of the freshly-cleared scrollback. Fresh sessions
  //       (messages.length === 0 — fresh boot, /chat new, /clear) keep
  //       using the live-region banner above so a resize re-renders it.
  //
  // We deliberately do NOT write \x1b[3J / \x1b[2J. CSI 3J wipes the entire
  // terminal scrollback buffer — including any pre-kiro shell history the
  // user could otherwise scroll back to (previous kiro sessions, prior
  // shell output, etc.). The previous behavior was to wipe everything for
  // a "fresh slate" feel; the cost was destroying the user's terminal
  // context entirely. Treating /chat new like a normal shell command (just
  // print new content below) preserves that context. Memory for twinki's
  // own accumulatedStaticOutput is bounded by its 10k-line cap, so growth
  // across many /chat new invocations is safe.
  //
  // Why this still runs in the render body (not a useEffect): the
  // staticItems memo below reads `staticItemsRef.current`,
  // `welcomeAppendedRef`, `forceWelcomeRef`, and
  // `lastFlushedEligibleCountRef` synchronously during render. If we reset
  // them in a post-commit effect, the FIRST render after a token bump
  // would already have run the memo with stale refs and committed the
  // prior session's items to twinki's <Static> a second time — twinki's
  // bridge would then append them to accumulatedStaticOutput, and the
  // user's scrollback would gain a duplicate copy of the old session
  // before the new one starts.
  //
  // The observed-token store is MODULE-LEVEL (`_liteLastObservedClearToken`,
  // declared above the component) rather than a per-mount ref. That is what
  // keeps this block from firing on a BARE remount — `mode` toggles
  // (Ctrl+G crew monitor, expanded, session-view) unmount and remount
  // LiteLayout without bumping the token, and the bridge's monotonic cursor
  // + accumulatedStaticOutput buffer persist across that cycle. A per-mount
  // ref would re-init on every remount and re-run the reset (cursor wipe +
  // re-emit) against the still-populated bridge buffer, duplicating scrollback
  // (or, with the cursor left ahead of the rebuilt array, swallowing new
  // rows). With the module-level token the block runs only on real token
  // bumps (resetMessages / setUiMode — /chat new, /clear, /rewind, lite↔tui
  // swap) plus once on the first mount (via the -1 init), which is exactly
  // when staticItemsRef is genuinely emptied and the cursor must realign.
  //
  // Cost: a single integer compare per render. The body runs once per
  // process plus a few times per session for actual token bumps.
  const liteScrollbackClearToken = useAppStore(
    (s) => s.liteScrollbackClearToken
  );
  const { adjustStaticCursor } = useTwinkiContext();
  if (liteScrollbackClearToken !== _liteLastObservedClearToken) {
    _liteLastObservedClearToken = liteScrollbackClearToken;
    // Snapshot "did this layout commit any rows to <Static> in the prior
    // session?" BEFORE we wipe staticItemsRef. This decides whether the
    // current clear-token bump is "swap-with-content" (banner shown as a
    // static session anchor) or truly fresh (banner shown live via the
    // gate below the JSX).
    //
    // Without this snapshot, /chat new mid-session falls through both
    // gates: messages was just emptied (so messages.some(User) is false)
    // and the live banner re-renders against a terminal scrollback that
    // still holds the prior session's static rows above it. The user
    // sees the KIRO art twice on screen — once in the preserved
    // scrollback, once newly above the divider — which reads as a
    // "banner re-emerged" bug. Promoting /chat new from non-empty
    // sessions into the swap-with-content path keeps the banner
    // anchored to the new (logically empty) session at the top of
    // <Static> and suppresses the live-region duplicate via the
    // length === 0 gate below the JSX.
    const hadPriorStaticContent = staticItemsRef.current.length > 0;
    adjustStaticCursor?.(Number.MAX_SAFE_INTEGER);
    staticItemsRef.current = [];
    lastFlushedEligibleCountRef.current = 0;
    pushedStaticIdsRef.current = new Set();
    committedTurnSummariesRef.current = new Set();
    openTurnUserIdRef.current = null;
    lastAppendedEligibleMsgRef.current = null;
    // Session-mode marker on swap-with-content. Truly-fresh sessions
    // (cold boot — staticItemsRef has never been populated AND messages
    // has no User row) keep using the live-region banner above so a
    // resize re-renders it; any session that already painted scrollback
    // in this mount (mid-session /chat new) OR carries a tui→lite /
    // /chat <id> load with chat history shows the banner as a static
    // row at index 0 of the freshly-emptied items array. Without this
    // static row the user gets no visual confirmation they've crossed
    // into lite mode — the live-region banner is permanently
    // suppressed on remount because `liteWelcomeEmitted` is true after
    // any prior unmount.
    //
    // Two-armed gate. The User-messages arm covers tui→lite mid-
    // session and /chat <id> load with history — `messages` carries
    // the prior chat at the moment of the bump. The
    // hadPriorStaticContent arm covers /chat new mid-session, where
    // `resetMessages` empties messages BEFORE bumping the token (so
    // messages.some(User) is false by the time this body runs) but the
    // layout had already committed rows to staticItemsRef from the
    // prior session. We deliberately don't broaden the User-messages
    // arm to e.g. all non-greeting roles: System rows from slash-
    // command announcements ("Switched to TUI mode", "/verbose density
    // set...") on an otherwise-empty session are NOT chat, and
    // counting them would re-emit the banner on every mode swap of an
    // empty session. The integ test `lite-welcome-banner-roundtrip`
    // pins that rule by performing tui↔lite swaps without sending
    // prompts and asserting the banner appears exactly once across
    // the run.
    //
    // The id includes the clear-token so each successive swap produces
    // a new monotonic id; reusing a fixed id would make twinki's
    // <Static> by-index cursor see the same item across two distinct
    // sessions and silently drop the second emission.
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

  // Cursor realignment on a real mode/session boundary is handled by the
  // clear-token reset block above: lite↔tui swaps go through setUiMode, which
  // bumps `liteScrollbackClearToken`, so the block fires and calls
  // adjustStaticCursor(MAX) during the render body (before twinki's next-tick
  // bridge render). We deliberately do NOT realign on every mount — a bare
  // remount (Ctrl+G crew monitor / expanded / session-view round-trip) leaves
  // the bridge cursor and accumulatedStaticOutput intact, so resetting the
  // cursor there would re-emit the whole scrollback on top of the preserved
  // buffer (duplicate rows) or strand newly-appended rows behind a stale
  // cursor (rows flash then vanish). The per-mount staticItemsRef rebuild
  // reconstructs the identical item array, so the persisted cursor still
  // aligns and new rows append correctly.

  // Queue-drain input restore. When processQueue dispatches a queued
  // slash command that opens a picker (e.g. /model, /agent), it stashes
  // the user's pre-drain input snapshot in `queuedInputRestore` rather
  // than restoring inline — see the field's doc comment in app-store.ts
  // for why. This effect catches the moment the picker closes
  // (`activeCommand` transitions from non-null to null) and applies the
  // restore via the store action.
  //
  // Catches BOTH close paths:
  //   1. Esc-dismissal (CommandMenu's `handleActiveCommandClose` calls
  //      `setActiveCommand(null)` then `clearCommandInput()`).
  //   2. Selection (`executeCommandWithArg` calls
  //      `set({ activeCommand: null })` and dispatches the chosen value).
  // In path 1, clearCommandInput already wiped commandInputValue to '';
  // applying the restore here puts the user's pending text back. In
  // path 2, the dispatcher's executeCommand RPC may take seconds — we
  // restore on the local `activeCommand` change rather than waiting
  // for the dispatch to complete, so the input shows the user's
  // pending text immediately after their selection.
  //
  // useLayoutEffect (not useEffect) so the restore commits in the same
  // browser frame as the activeCommand → null transition. Using
  // useEffect would let the empty input paint for one frame between
  // picker dismissal and restore, producing a visible flicker the
  // user would read as "my text disappeared and then came back".
  //
  // The ref tracks the previous activeCommand so we only fire on the
  // non-null → null edge — not on the initial mount with both null,
  // and not when activeCommand changes between two non-null values
  // (sub-command picker → top-level picker, etc.).
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

  // Cheap predicate — has any subagent tool ever appeared in this session?
  // Most turns never spawn a pipeline, and the per-message walks below
  // (subagentSummariesById in staticItems, activeSubagents row builder)
  // are wasted work otherwise. Gate them on this single .some() so the
  // no-subagent path stays O(0) past the check.
  const hasAnySubagentTool = useMemo(() => {
    for (const m of messages) {
      if (m.role === MessageRole.ToolUse && m.name === 'subagent') return true;
    }
    return false;
  }, [messages]);

  // Static items: welcome + finalized messages, append-only. Each item's
  // text is baked once at flush time. The previous item's text is never
  // touched after first paint, so <Static>'s by-index cursor stays valid.
  //
  // Delta-append: rather than rebuilding the items array each render (the
  // prior shape allocated O(messages) objects per chunk just to hand twinki
  // a stable-length array), we maintain `staticItemsRef.current` as a
  // persistent array and only append rows for messages that joined `eligible`
  // since the last walk. The output of `useMemo` is the same array reference
  // most of the time, mutated in place by the append; React + twinki re-read
  // it via length each render. Memo deps only include the inputs we actually
  // need to detect: messages reference (a chunk landed), isProcessing flip
  // (turn boundary), turnSummaries (a trailer became available), and
  // theming/agent inputs that affect the renderer for NEW rows.
  //
  // Resize caveat: this memo bakes `process.stdout.columns` (via
  // renderCtx.termCols) into each row's text. If the terminal resizes after
  // a row lands in <Static>, its text is now stale. Twinki's monotonic
  // cursor silently ignores re-emissions for already-printed indices — which
  // is exactly why scrollback stays append-only. DO NOT add a resize-driven
  // redraw here: it would either be silently dropped (status quo) or,
  // worse, scramble historical rows if someone later "fixes" Static to honor
  // in-place edits. Soft-wrap by the terminal itself absorbs width changes
  // for live writes; historical rows are correct as-of-flush-time.
  const staticItems = useMemo(() => {
    const items = staticItemsRef.current;

    // Welcome banner placement is hybrid:
    //   1. Live region (rendered below as a `<Text>` while
    //      `showWelcomeBanner` is true): handles the fresh-session
    //      resize case — `doResize` (in lite-scope `preserveScrollback`
    //      mode) drops `accumulatedStaticOutput` and the bridge cursor
    //      stays put, so a static-only banner would silently disappear
    //      from the viewport on resize while the user is still on the
    //      welcome screen. The live region re-composes every frame, so
    //      a resize reflows it at the new width for free.
    //   2. <Static> at index 0 (pushed below, gated on `items.length
    //      === 0`): handles persistence as a session delimiter the
    //      moment chat content lands. `showWelcomeBanner` flips false
    //      on the first User row in messages, the live-region banner
    //      unmounts in the same render, and the static push happens
    //      atomically — visually the banner stays in place, just
    //      transitions from "rendered each frame" to "anchored at the
    //      top of scrollback forever". From there it scrolls into the
    //      terminal's saved-lines buffer as the chat grows, and the
    //      user can scroll up to it as a top-of-session marker.
    //
    // Why both, not just one: a static-only banner regressed the
    // fresh-session resize UX (b72df6cd8 reverted that). A live-only
    // banner regressed scrollback persistence — the user's complaint
    // here, and the behavior the released toolbox version had. Doing
    // both keeps each case correct without either path having to fight
    // the other.
    //
    // Coordination with the swap-with-content push above: that handler
    // pushes its own `lite-mode-banner-${liteScrollbackClearToken}`
    // when `messages.some(User)` was already true at clear-token time
    // (tui→lite swap mid-session, /chat <id> load with history,
    // resume). After that push, `items.length` is 1, so the gate below
    // declines to push a second banner. The two paths are mutually
    // exclusive: one fires on session-boundary-with-history, the
    // other on first-message-in-fresh-session.

    // Apply the tui→lite bookmark by slicing prior messages out before
    // selecting static-eligible rows. selectStaticEligible's "skip last
    // streaming Model" rule still needs to see the slice's tail as the
    // current tail, which `slice(skip)` preserves. activeToolBatchIds was
    // computed from the FULL `messages` to pick the right trailing batch
    // including in-flight tools that started before the swap; passing the
    // same Set in lets selectStaticEligible exclude those whether or not
    // the batch crosses the bookmark.
    //
    // Welcome-screen suppression: while the welcome screen is up
    // (`showWelcomeBanner === true`), drop any agent standalone-greeting
    // Model rows from the eligible set. Those rows are rendered alongside
    // the welcome banner in the live region (see JSX below) so the banner
    // sits ABOVE the greeting visually — the original layout was
    // [banner][greeting][chrome] and we want to preserve that.
    // The moment any non-greeting message lands (User, System, Model
    // response, ToolUse), `showWelcomeBanner` flips false and the
    // greeting falls back into the eligible set on the next memo run, so
    // it commits to <Static> exactly once and then behaves like any other
    // chat message (scrolls into scrollback as the conversation grows).
    const sliced =
      liteStaticSkipBefore > 0 && liteStaticSkipBefore <= messages.length
        ? messages.slice(liteStaticSkipBefore)
        : messages;
    const visibleMessages = showWelcomeBanner
      ? sliced.filter((m) => !(m.role === MessageRole.Model && m.standalone))
      : sliced;
    // /verbosity → showThinkingContent: when off, the renderer returns '' for
    // empty-content+thinking-only Model rows. Without this gate at eligibility
    // time, those rows would still get a '\n' leading-blank prefix and become
    // `prevMsg` for the next iteration — pinning 2-3 phantom blank rows into
    // <Static> for every Thought-only round (agent thinks, then directly
    // emits a tool call without spoken text). Minimal preset hits this path
    // every time the agent reasons silently before acting.
    const hideThinkingContent =
      getVerboseDisplay().showThinkingContent === false;
    const eligible = selectStaticEligible(
      visibleMessages,
      isProcessing,
      activeToolBatchIds,
      agentName,
      hideThinkingContent
    );

    // Nothing new to flush AND no trailers to emit — skip all the build-up
    // work for the renderCtx / subagent walk / theme. Most spinner-driven
    // re-renders fall through here, so this is the hot-path bypass.
    //
    // Guard against `eligible` shrinking below the high-water mark. The delta
    // walk below is index-based and assumes the eligible prefix only grows.
    // But a message can flip OUT of the eligible set after it was counted —
    // e.g. an agent/agentName change reclassifying a main-agent tool as an
    // inner-subagent tool, a Model row whose content is cleared, or a
    // /verbose thinking toggle dropping empty-thinking rows. When that
    // happens `lastFlushedEligibleCountRef` is left ABOVE eligible.length, the
    // walk `for (i = highWater; i < eligible.length)` never runs, and any
    // message that later lands BELOW the stale mark is silently stranded —
    // never flushed to <Static>, so it "disappears" from scrollback until a
    // re-prompt grows eligible back past the mark (matching the reported
    // disappear-fixed-by-reprompt symptom). Clamp the mark down to the
    // current length so the next delta walk starts from a valid index;
    // pushedStaticIdsRef stays the source of truth for dedup, so re-walking
    // already-emitted rows is a no-op (skipped) and cannot duplicate. The
    // warn fires only on the anomaly, so a real-usage hit leaves a breadcrumb.
    if (lastFlushedEligibleCountRef.current > eligible.length) {
      logger.warn('[lite] static high-water above eligible length — clamping', {
        highWater: lastFlushedEligibleCountRef.current,
        eligible: eligible.length,
        messages: messages.length,
        agentName,
      });
      lastFlushedEligibleCountRef.current = eligible.length;
    }
    const haveNewEligible =
      eligible.length > lastFlushedEligibleCountRef.current;
    // Detect a pending trailer that was just ready (turn ended OR summary
    // arrived after the turn settled). Cheap scan over committed turns —
    // the in-flight turn's id is already in openTurnUserIdRef.
    const openTurnId = openTurnUserIdRef.current;
    const havePendingTrailerForOpenTurn =
      !isProcessing &&
      openTurnId != null &&
      turnSummaries.has(openTurnId) &&
      !committedTurnSummariesRef.current.has(openTurnId);
    if (!haveNewEligible && !havePendingTrailerForOpenTurn) {
      return items;
    }

    // First-content banner: prepend the welcome banner the FIRST time
    // chat content lands in static for this session. From here on out
    // the banner lives at index 0 of <Static> as a top-of-session
    // delimiter — never recomputed, never updated, scrolling into the
    // terminal's saved-lines buffer as the chat grows. The live-region
    // banner above unmounts on the same render via its own gate
    // (`showWelcomeBanner` flips false the moment any non-greeting
    // message lands in `messages` — a User row, a System announcement
    // from a slash command, etc.), so the banner visually stays in
    // place — just transitions from "rendered each frame in the live
    // region" to "anchored in static scrollback".
    //
    // The `items.length === 0` gate is what keeps this exclusive with
    // the swap-with-content push in the clear-token handler above.
    // That handler fires when a clear-token bump lands while
    // `messages.some(User)` is already true (tui→lite swap mid-
    // session, /chat <id> load with prior history, resume past the
    // history cap). After that push, `items.length` is already 1, so
    // we skip here — exactly one banner row per session start.
    //
    // Past this point in the memo `eligible.length > 0` is guaranteed
    // (the early-return above only falls through when there's content
    // to flush or a trailer to emit; trailers require an existing turn,
    // which means an eligible User row), so we don't need to re-check.
    if (items.length === 0) {
      items.push({
        id: '__lite_welcome__',
        text: welcomeBannerText,
      });
    }

    // Build the per-render theme + renderCtx once. /theme bundled:dark/light
    // flips the ThemeContext values via setBaseTheme + setUserColors, which
    // re-runs this memo with new accessors — so the next message rendered
    // into <Static> picks up the new colors. Already-flushed scrollback rows
    // keep their original colors (their text is frozen in `staticItemsRef`
    // at flush time); only future content reflects the swap.
    //
    // Subagent walk is gated on hasAnySubagentTool — the common no-subagent
    // session pays nothing.
    const subagentSummariesById = new Map<
      string,
      Array<{ stageName: string; contextSummary: string; taskResult: string }>
    >();
    if (hasAnySubagentTool) {
      let activeParentId: string | null = null;
      for (const m of messages) {
        if (m.role !== MessageRole.ToolUse) continue;
        const isParentSubagent =
          m.name === 'subagent' && (!m.agentName || m.agentName === agentName);
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
      // Belt-and-suspenders dedup. The index-based delta walk assumes the
      // eligible PREFIX is stable from one render to the next — but a
      // Model row can flip ineligible→eligible AFTER a later (System,
      // User) row was already flushed (e.g. the shell-escape cancel
      // race: empty-buffer Model gets its content set after cancelArmedRef
      // appended a `user interrupted` System). The eligible array grows,
      // but the message at `eligible[lastFlushed]` is now a row we
      // already pushed. Without this skip, we'd push it again — duplicate
      // id in `items`, duplicate React key, and the warning fires forever
      // because static is append-only. The Set is bounded by lifetime
      // message count and reset on clear-token bumps below.
      if (pushedStaticIdsRef.current.has(msg.id)) continue;
      // Trailer flush rule (1): a User or System message after the
      // currently-open turn closes that turn — push the trailer (if
      // available) BEFORE this boundary row. System messages get the same
      // treatment so /verbose status announcements don't push the trailer
      // off into a later turn.
      if (
        openTurnUserIdRef.current &&
        (msg.role === MessageRole.User || msg.role === MessageRole.System)
      ) {
        commitTrailer(openTurnUserIdRef.current);
      }
      const prefix =
        prevMsg !== null && needsLeadingBlank(prevMsg, msg) ? '\n' : '';
      // Each eligible id appears in this walk exactly once — the cursor
      // (`lastFlushedEligibleCountRef`) advances past it on the same pass,
      // so a render-text cache would be write-only after first use. Render
      // straight into the items array and let `staticItemsRef.current` be
      // the single retained store of finalized rows.
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

    // Return a NEW array reference (shallow shell) each render. Twinki's
    // <Static> compares `items` by reference inside its internal
    // `useMemo([items, index], () => items.slice(index))`; passing the same
    // mutated reference would make it blind to newly appended entries.
    // The shallow copy is O(N) on pointer entries — cheap relative to the
    // O(delta) work above and orders of magnitude cheaper than allocating
    // each `{id, text}` row from scratch every chunk.
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
    // Used by the first-content banner push above and the welcome-screen
    // standalone-greeting filter. Identity is stable across renders — the
    // memo for welcomeBannerText only re-computes when allowAsciiArt
    // toggles, and showWelcomeBanner is derived from messages +
    // liteWelcomeEmitted. Adding both ensures a /settings allowAsciiArt
    // toggle on the welcome screen reflows the banner row that's about
    // to be committed (already-committed rows keep their original text
    // by design, same as every other static row).
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

  // Context % — estimate during streaming using accumulated content length.
  // Read from the streamingContent slot directly (no walk over `messages`)
  // so a per-chunk content update doesn't invalidate this memo *and* every
  // other memo with `[messages]` deps. The selector returns '' when not
  // processing so the ctxPct memo's gate still suppresses the estimate.
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

  // Active subagents footer strip. One row per running stage, in spawn
  // order, with a per-stage state machine:
  //
  //   - "Running"      — most recent tool is non-summary, in-flight or just
  //                      settled. Row shows "<tool> <detail>...".
  //   - "Synthesizing" — stage's `summary` tool is in flight. Row shows
  //                      "Synthesizing...".
  //   - "Complete"     — stage's `summary` tool has finished. Row shows
  //                      "✓ Complete" (kept around so the user sees each
  //                      stage tick off rather than rows just disappearing).
  //
  // Below the per-stage rows, when EVERY known stage is Complete and the
  // parent `subagent` tool is still in flight, we surface one footnote line
  // ("Summarizing N agents...") for the parent's finalize phase.
  //
  // Every active subagent gets a row — no overflow collapse. Footer height
  // is bounded only by how many stages are actually running concurrently,
  // which the user already controls when designing their pipeline. The
  // previous 3-row cap silently dropped stages past the cap into a
  // "(+N more)" chip, which made the strip lie about pipeline state when
  // the user spawned 4+ stages.
  const { activeSubagents, summarizingPhase } = useMemo<{
    activeSubagents: SubagentRow[];
    summarizingPhase: boolean;
  }>(() => {
    if (!isProcessing) return { activeSubagents: [], summarizingPhase: false };
    // No pipeline has ever run in this session — skip the .some() walk and
    // both downstream loops. Common case for short turns; cuts O(messages)
    // out of every render while the agent is processing.
    if (!hasAnySubagentTool)
      return { activeSubagents: [], summarizingPhase: false };

    const anyParentSubagentRunning = messages.some(
      (m) =>
        m.role === MessageRole.ToolUse && m.name === 'subagent' && !m.isFinished
    );
    // Final outputs are only built when a parent subagent is in flight (see
    // the guard at the rows assembly below). Bail before walking sessions +
    // messages when no parent is running — the strip would render empty.
    if (!anyParentSubagentRunning)
      return { activeSubagents: [], summarizingPhase: false };

    const byStage = new Map<string, SubagentRow>();
    const order: string[] = [];

    // Seed rows from sessions FIRST — a stage starts emitting ToolUse
    // messages only after its model produces its first tool call, but the
    // ACP session for the stage is created the moment the stage spawns.
    // Without this seed pass, stages that spend their early turn thinking
    // / streaming text show up in the footer only when their `summary`
    // tool fires (Synthesizing...), which made the user think their
    // subagents weren't running.
    //
    // Include terminated/failed sessions too: `sessions.values()` walks
    // in insertion order, which is spawn order. If we skip terminated
    // stages here, the message-walk loop below re-introduces them via
    // `order.push(m.agentName)` at the TAIL — so the row reshuffles to
    // the end the instant a stage finishes. The user sees the footer
    // reorder mid-run. Seeding terminated stages here lets the message
    // walk update them in place; phase still flips to 'complete' when
    // their `summary` tool finishes. The early-return above (no parent
    // subagent in flight) already prevents stale rows from a prior
    // pipeline leaking into the next turn.
    for (const session of sessions.values()) {
      const stageName = session.name;
      if (!stageName || stageName === agentName) continue;
      if (byStage.has(stageName)) continue;
      order.push(stageName);
      byStage.set(stageName, {
        name: stageName,
        // The footer's `killed` phase must reflect explicit user kills only —
        // not "session ended for any reason." Rust's session-list update
        // drops a session as soon as its orchestrated task finishes
        // (success path) and index.tsx flips the missing-but-busy entry to
        // status: 'terminated', so a status check would paint normally-
        // completed stages as red `✗ killed`. The Ctrl+X handler below is
        // the only path that should surface the kill chip; it adds the
        // sessionId to userKilledSessionsRef before issuing terminate.
        phase: userKilledSessionsRef.current.has(session.id)
          ? 'killed'
          : 'running',
        activeToolName: null,
        activeToolDetail: null,
        activeToolFinished: false,
      });
    }

    // Toolcall id for the *current* approval prompt (if any). When a subagent
    // requests a tool that the user must approve, the prompt mounts at the
    // parent layout — but the subagent's footer row otherwise still says
    // "<tool>..." like it's running, which is misleading. We want the row to
    // explicitly say it's blocked on the user, with the same yellow accent
    // the approval prompt uses.
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

  // Auto-expand the requesting subagent's panel while a subagent permission
  // is pending. The chat log otherwise hides inner subagent activity, so a
  // tool approval can be hard to contextualize. Snapshot the prior panel
  // state at the moment the request lands; restore it once the request
  // clears so the user is dropped back exactly where they were.
  //
  // Snapshot is keyed by the approval's toolCallId so consecutive requests
  // from different stages each get their own snapshot, and a request that
  // clears via answer / cancel walks back through the same snapshot used to
  // open it. We track the "owning" approval id with a ref so subsequent
  // approvals don't re-snapshot from an already-overridden state.
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

  // Panel keypress handler. Bindings:
  //   ctrl+o            toggle open/close
  //   esc               close
  //   shift+←/→         cycle prev/next subagent (←/→ alone are left for the
  //                     prompt input so the user can still edit text)
  //   ↑/↓               scroll 3 lines (configurable below)
  //   pgup/pgdn         page scroll
  //   ctrl+a            jump to top   (mnemonic: A = first letter / above)
  //   ctrl+z            jump to bottom — re-engages follow (Z = last / below)
  // We deliberately avoid ctrl+1 / ctrl+0 because most terminal emulators
  // bind those to tab navigation. Ctrl+Z normally suspends the process, so
  // the global app-keypress dispatcher checks subagentPanelOpen and skips
  // suspend when the panel claims it.
  // PromptInput suppresses unmodified ↑/↓ while the panel is open so arrow
  // scrolling doesn't also move its cursor; ←/→ flow through unchanged.
  const SUBAGENT_SCROLL_STEP = 3;
  const SUBAGENT_PAGE_STEP = 8;
  const PANEL_LINES = 16;
  // Floor offset for follow disengagement / re-engagement.
  const subagentMaxOffset = Math.max(0, subagentTotalLines - PANEL_LINES);
  useKeypress((input, key) => {
    // Ctrl+X — kill ladder for the focused subagent. First press while the
    // panel is open ARMS the kill (yellow chip + 2s window); a second press
    // within the window invokes terminateSession and cleans up. Bails for
    // stages already in the 'complete' phase (nothing to kill) and for
    // closed panels (the panel-keypress useKeypress runs in this layout
    // unconditionally; we have to gate every branch on `subagentOpenIndex`
    // ourselves). Mirrors CrewMonitorLayout's pattern so users carry the
    // same muscle memory across modes.
    if (key.ctrl && (input === 'x' || input === 'X')) {
      if (subagentOpenIndex == null) return;
      const focused = activeSubagents[subagentOpenIndex];
      // Bail on terminal phases — `complete` (stage finished naturally) and
      // `killed` (user already killed it). Re-firing terminate on a dead
      // session is a no-op at the backend but the local cleanup would race
      // the prior kill's state.
      if (
        !focused ||
        focused.phase === 'complete' ||
        focused.phase === 'killed'
      )
        return;
      const sessionId = subagentSessionIdByName.get(focused.name);
      if (!sessionId) return;
      if (armedKillSessionId === sessionId) {
        // Second press within window — actually kill. Logged at info level
        // so the kill action shows up in trace logs alongside the eventual
        // backend cancel notifications, useful for debugging "why did the
        // parent agent re-spawn this stage?" reports. The backend's
        // session_manager.rs (err arm of the orchestrated session task)
        // writes "[Cancelled by user]" to the stage's result before
        // terminating, so the parent's `subagent` tool output shows that
        // string in place of the ambiguous "No result" — without that
        // backend tweak, the parent reads a None result as silent failure
        // and may re-spawn the work the user just killed.
        logger.info('[lite] killing subagent stage', {
          sessionId,
          stageName: focused.name,
          phase: focused.phase,
        });
        disarmKill();
        // Mark this session as user-killed so the footer paints the kill
        // chip. Must happen before updateSession/terminateSession so the
        // next byStage memo run sees the kill flag at the same time it sees
        // status: 'terminated'.
        userKilledSessionsRef.current.add(sessionId);
        updateSession(sessionId, { status: 'terminated' });
        kiro?.terminateSession(sessionId).catch(() => {});
        cleanupTerminatedSession(sessionId);
        // Mark in-flight tool calls as finished in the session conversation
        // store so the trace doesn't have a forever-spinning row after
        // kill. Mirror of the same write CrewMonitorLayout does — the
        // backend's terminate notification doesn't replay in-flight tool
        // updates as cancelled, so we stamp them locally.
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
        // If the pending approval belonged to the killed stage, drop it.
        // The session's process is gone the moment terminateSession lands;
        // any answer to the prompt would just fail silently at the backend.
        // Pure-logic helper so the matching rules are unit-tested.
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
      // First press — arm. Replace any existing arm timer (e.g. user was
      // armed on a different session and pressed Ctrl+X on a new one — the
      // arm shifts to the new target with a fresh 2s window).
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

  // Resolve the user's prompt preset into chalk wrappers used by the input row.
  // The glyph color follows the prompt text color; the surrounding box bg
  // follows the prompt bg hex (when set), giving /theme purple a visible
  // highlight on the input area itself — not just on user messages in
  // scrollback. PromptInput already picks up getUserPromptColor() for the
  // typed text, so this fills in the chrome around it.
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
      {/* Scrollback: finalized messages — append-only, never deleted.
          wrap="overflow" tells twinki to write each item.text as-is, even
          when wider than the terminal — the terminal soft-wraps long lines
          visually instead. Pairs with the no-paragraph-wrap, no-code-line-
          wrap policy in lite/render.ts: the rendered text contains \n only
          at structural breaks (between paragraphs, around tables, between
          list items, around blockquotes) so copy-paste from scrollback
          preserves logical lines. URLs and long sentences in agent prose
          and code block bodies copy as one line, not N hard-newline
          chunks. wideLines is already enabled at index.tsx for lite mode
          so twinki's physical-row math handles soft-wrapped scrollback
          correctly. */}
      <Static items={staticItems}>
        {(item) => (
          <Text key={item.id} wrap="overflow">
            {item.text}
          </Text>
        )}
      </Static>

      {/* Welcome banner — rendered as a live-region row while the
          welcome screen is still active. The agent's standalone
          welcome greeting may already be in `messages`, but it
          doesn't count as chat content; any non-greeting row (User
          prompt, System slash-command announcement, Model response,
          ToolUse) ends the welcome screen. Lives outside <Static> so
          a terminal resize, which clears `accumulatedStaticOutput`,
          re-renders it fresh against the new width instead of
          leaving an empty viewport. The moment a non-greeting
          message lands, the banner unmounts and the static path
          takes over (pushing a fresh banner row at index 0 of
          <Static> as the session anchor) — no duplicate-banner
          ghost frame because static and live are composed in the
          same render pass.

          The `staticItemsRef.current.length === 0` gate keeps this
          mutually exclusive with the swap-with-content path above:
          when a clear-token bump pushed a `lite-mode-banner-${...}`
          row (mid-session /chat new, tui→lite with chat, /chat <id>
          load with history), staticItemsRef is non-empty by the
          time the JSX runs and the live banner stays suppressed —
          so the user doesn't see two KIRO arts on screen at once
          (one in scrollback / static, one above the divider). On
          true cold boot the ref is empty, so the live banner shows
          and resize reflows it cleanly. */}
      {showWelcomeBanner && staticItemsRef.current.length === 0 && (
        <Text wrap="overflow">{welcomeBannerText}</Text>
      )}

      {/* Agent's standalone greeting — rendered alongside the banner so the
          user sees the agent's "Hi, I'm <agent>..." text from the start of
          the welcome screen. The greeting Model row is filtered out of the
          static eligible set while `showWelcomeBanner` is true (see the
          staticItems memo above). The moment any non-greeting message
          lands (User, System, Model response, ToolUse), the gate flips
          off here and the same greeting commits to <Static> via the
          eligible-set fallback path — visually the row stays in place,
          just transitions from "rendered each frame in the live region"
          to "anchored in static scrollback". */}
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

      {/* Queued messages — the slot the user is currently editing (if any)
          is highlighted with a chevron + non-dim color so it's obvious which
          message in the queue the input box maps to. Each row is a *preview*
          only: the full text lives in `queuedMessages` and is restored
          verbatim when the user pulls the slot back into the prompt input.
          Rendering the full text here used to hang the lite UI on multi-KB
          paste — twinki's default word-wrap walked the entire string and
          laid out hundreds of physical rows on every parent re-render,
          blowing past the 150ms spinner cadence in <LiteLiveRegion>. The
          per-slot `previewLine` cap + `wrap="truncate-end"` belt-and-
          suspenders means each row's render cost is bounded by terminal
          width, not message length.

          Hidden during shell escape — the queue is for the agent, not for
          bash. The user's queued messages are still in the store and will
          be available the moment bash exits; suppressing the strip just
          keeps the visual focus on the bash interaction. */}
      {queuedMessages.length > 0 && !isShellEscape && (
        <Box flexDirection="column">
          {queuedMessages.map((msg, i) => {
            const editing = editingQueueIndex === i;
            const marker = editing ? chalk.cyan(`${glyphs.chevron} `) : '  ';
            // Reserve cols for the marker (`> ` or two spaces, 2 cols) and
            // the index prefix (`NN. `, ~4 cols at three-digit queues) so a
            // wide message preview can't push the row past the terminal
            // edge before truncate-end kicks in.
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
      {/* Task tray — mirrors the modern TUI's <ActivityTray /> for the tasks
          half. Mounted right above the divider so it sits in the input zone
          (matches where the modern TUI places ActivityTray, above PromptBar).
          Self-renders null when no tasks exist. Toggle: Ctrl+X (handled at
          the layout level above so it can be gated on queue-edit / approval /
          panel state). */}
      <LiteTaskTray />
      <Divider />

      {/* Status line — sits between the divider and the input area, matching
          the TUI's `Divider → header → input` order. Hidden while connecting
          since we don't have real data yet. When an agent swap is pending,
          render the requested name with a spinner instead of the (still-old)
          currentAgent. Hidden during shell escape: the agent isn't running,
          and showing model · ctx · effort while the user is interacting
          with bash misleads about what's actually happening on screen. */}
      {isInitialized &&
        !isShellEscape &&
        (() => {
          // Build the status footer as discrete colored segments, then pack
          // them into width-bounded lines so the row collapses onto new lines
          // as the terminal narrows (agent · model · effort · ctx · branch ·
          // goal → wraps), mirroring the modern TUI's ContextBar flex-wrap.
          // The goal segment is '' when no goal is active, so packStatusSegments
          // drops it. Floor at 20 cols so a tiny terminal still wraps sanely.
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

      {/* Boot indicator — single dim row that surfaces in-flight async setup
          (agent connect, session create, MCP loading). Rendered between the
          status line and the input area so it reads as session-level chrome
          without crowding scrollback. The row picks the most relevant phase
          (agent_connect > session_create > MCP aggregate) so the user sees
          one piece of progress at a time, and disappears the moment its
          phase settles.

          Replaces the previous multi-line connecting panel. Rationale lives
          on the `showBootIndicator` memo above. The render is computed
          inline (not memoized) because elapsed needs to update every
          bootFrame tick — the closure cost is bounded by the small
          bootProgress + mcpInitStatus map sizes. */}
      {showBootIndicator && (
        <Text>
          {formatBootIndicator(
            selectBootIndicatorPhase(bootProgress, mcpInitStatus),
            spinners.brailleRotate[bootFrame % spinners.brailleRotate.length]!
          )}
        </Text>
      )}

      {/* Approval prompt — inside the input area, clearly separated.
          No marginTop: matches the input area's flow exactly, so the divider
          and status line don't jump when approval mounts/unmounts. */}
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

      {/* Backend-driven panels (/context, /mcp, /help, /tools, ...). These
          replace the input area while open; the same components InlineLayout
          mounts in TUI mode. They consume their own keypresses (Esc to close
          via Panel.tsx's useInput), and our always-armed handler above
          short-circuits Esc when anyPanelOpen so we don't also cancel the
          turn that produced the panel. The append-only <Static> scrollback
          above is untouched. */}
      {!showApproval && anyPanelOpen && (
        <Box flexDirection="column">
          <BackendPanels handlers={handlers} />
        </Box>
      )}

      {!showApproval && !anyPanelOpen && (
        <Box flexDirection="column">
          {/* Editing-queue header — only visible while the user has pulled a
              queued message back into the input. Tells them exactly which
              slot they're editing so the input doesn't feel like a fresh
              compose. Esc abandons; Enter commits (or deletes when empty). */}
          {editingQueueIndex != null && (
            <Text>
              {chalk.cyan(
                `${glyphs.chevron} editing queued #${editingQueueIndex + 1}`
              )}
              {chalk.dim(' · enter saves · ctrl+x deletes · esc cancels')}
            </Text>
          )}
          <CommandMenu />
          {transientAlert && (
            <Text>
              {colorTransientAlert(
                transientAlert.message,
                transientAlert.status
              )}
            </Text>
          )}
          {exitSequence > 0 && (
            <Text>{chalk.dim('Press Ctrl+C or Ctrl+D again to exit')}</Text>
          )}
          {/* Input row — `> ` glyph and surrounding box pick up the user's
              prompt preset colors so /theme actually re-skins the lite input.
              The bg highlight (when set) paints across the whole row to match
              what standard mode does via `<Box backgroundColor>` in
              chat/message/Message.tsx. PromptInput already uses
              getUserPromptColor() for the typed text itself.

              `width="100%"` on the row + `flexShrink={1}` on the input's
              flexGrow box are the load-bearing pieces for multi-line wrap:
              without an explicit width on the row, twinki's Yoga shell
              computes the row's width from its content (the `> ` glyph),
              and `flexGrow={1}` on the inner box can't grow past a parent
              that's only 2 cols wide. The Text inside PromptInput then has
              no real width to wrap against, so >3-line input runs off the
              right edge of the terminal. PromptBar uses the same pattern. */}
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
        </Box>
      )}

      {/* Subagent activity strip — pinned at the bottom of the input area so
          inner tool activity is always visible without being mixed into the
          chat log. Every active stage gets one row.
          When the user presses Ctrl+O, the focused subagent's row is
          replaced inline by a fixed-height trace panel, with the other
          rows still visible above and below for context. */}
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
          // Panel viewport: ~16 lines gives enough room to read a few turns of
          // tool activity / reasoning without dominating the screen. PANEL_LINES
          // here MUST match the constant used by the keypress handler above so
          // the floor-detection math stays consistent.
          return (
            <Box flexDirection="column">
              {/* Top breathing room so the strip doesn't sit flush against the
                input box / approval prompt. */}
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
                  // Backend hasn't surfaced a sessionId for this stage yet
                  // (rare race when the panel opens immediately after spawn).
                  // Show a placeholder so the user sees feedback. Tag color
                  // matches the rest of the strip so it doesn't pop.
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
              {/* Discoverability hint: only when the panel is closed and there's
                at least one subagent the user can inspect. Stays out of the
                way otherwise. */}
              {openIdx == null && activeSubagents.length > 0 && (
                <Text>{chalk.dim('  press ctrl+o to expand')}</Text>
              )}
            </Box>
          );
        })()}
    </Box>
  );
};

// Color the transient alert message based on status so warnings/errors stand
// out from informational notices. We don't render a chip/icon — the lite UI
// already runs lean on glyphs; color is enough signal at this density.
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

// Goal-loop status segment for the lite status line. Returns '' when no goal
// is active so packStatusSegments skips the slot entirely. Mirrors the modern
// TUI's ContextBar goal chip (InlineLayout) in content — icon + state +
// iteration — but rendered as a flat colored string in the lite footer idiom
// (dim like the branch segment for the common active case). The goal TEXT
// isn't shown here (it can be long and would dominate the row); bare `/goal`
// surfaces the text via a transient alert and a scrollback confirmation lands
// when the goal is first set. Terminal states (done/exhausted) auto-clear from
// the store after 3s, so they flash briefly then the segment disappears.
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

// Color the agent name in the status footer the same way the chip in V2's
// InlineLayout does — each agent gets a stable color from agentColors.ts so
// users get a consistent visual cue across modes. The agent name is shown
// verbatim (including `kiro_default`); only the *color* is mapped.
function colorAgentName(
  agentName: string | null,
  getColor: (path: string) => any
): string {
  const raw = agentName || 'kiro';
  const color = getAgentColor(raw, getColor);
  return color(raw);
}

// Smooth RGB gradient for the ctx-usage indicator. Replaces the prior step
// thresholds (green<50%, yellow<80%, red≥80%) which felt misaligned with
// the larger context windows of newer models — by the time the indicator
// flipped yellow at 50%, the user had already burned hundreds of thousands
// of tokens on a 1M-window model with no visual warning along the way.
//
// Curve: three segments in RGB space, piecewise-linear within each.
//   0–20%   → flat green        rgb(80, 200, 80)   ← stable, no tint yet
//   20–30%  → green → yellow    transition zone
//   30%     → full yellow       rgb(220, 220, 0)
//   30–100% → yellow → red      drift through amber/orange
//   100%    → warm red          rgb(220, 60, 60)
//
// The 0–20% plateau keeps the indicator solidly green at low context (12%
// shouldn't visually differ from 4%) — only once the user crosses into
// the "starting to fill" zone does the color move. The narrow 20–30%
// transition makes the green→yellow shift visibly fast so the user
// notices the change, then the wider 30–100% segment lets the eye see a
// steady drift rather than another sudden flip on the way to red.
// chalk.rgb falls back to nearest 256-color on terminals without
// truecolor support, so the gradient still degrades gracefully.
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
