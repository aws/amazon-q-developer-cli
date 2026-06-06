/**
 * LiteLiveRegion: Shows active tool calls + streaming content + thinking.
 *
 * UX:
 * - ALL running tool calls shown simultaneously
 * - Thinking timer resets per thinking batch (not per turn)
 * - Thinking content shown dim/italic
 * - Streaming content NOT height-bounded — terminal scrolls naturally
 */
import React, { useState, useEffect, useRef, useMemo } from 'react';
import { Box, Text } from '../../../renderer.js';
import {
  useAppStore,
  MessageRole,
  type MessageType,
} from '../../../stores/app-store.js';
import { usePendingSwap } from './usePendingSwap.js';
import { getVerboseConfig, getVerboseDisplay } from '../../../lite/verbose.js';
import { useTheme } from '../../../hooks/useThemeContext.js';
import {
  buildRenderTheme,
  renderAgentMessage,
  renderShellOutputBlock,
  renderThinkingBlock,
  renderLiveStreamingOutputBar,
  renderMessageToText,
  type RenderContext,
} from '../../../lite/render.js';
import { needsLeadingBlankByRole } from '../../../lite/blank-rules.js';
import { getAgentColor } from '../../../utils/agentColors.js';
import {
  useGlyphs,
  useSpinners,
  useAllowAsciiArt,
} from '../../../hooks/useGlyphs.js';
import { useAnimationPaused } from '../../../contexts/AnimationPausedContext.js';
import chalk from 'chalk';

// Lite-specific pacman frames (Unicode mode only). When ASCII mode is on,
// the live region falls back to the canonical `quarterSpinner` set
// (`-\|/`) so /settings allowAsciiArt or KIRO_ASCII_MODE=1 produces a
// spinner that copies into any terminal. The pacman is a lite UX flourish;
// the fallback keeps the spinner visible without baking Unicode shapes
// into ASCII users' scrollback.
const PACMAN_SPINNER_FRAMES: readonly string[] = [
  'ᗢ',
  'ᗣ',
  'ᗤ',
  'ᗥ',
  'ᗦ',
  'ᗧ',
  'ᗨ',
  'ᗩ',
];
const SPINNER_INTERVAL = 150;

// Sentinel substituted per spinner tick into the status slot of a running,
// non-trivial tool's rendered chat-log row. Two US-separator control chars
// — picked so it can't collide with anything else in tool args, ANSI
// escapes (chalk uses \x1B[...), markdown bodies, or terminal output. The
// row itself is built ONCE per (id, content) tuple via
// `renderMessageToText` with `runningSpinner: SPINNER_PLACEHOLDER`, then
// each render does a single `replaceAll` to swap the current spinner glyph
// in. That keeps the args/diff/reasoning identical to the row's eventual
// settled appearance — the bug being fixed is that the OLD live-region
// formatter showed only the tool name + a single `__tool_use_purpose`
// chip, hiding the actual command/path/diff while a trusted tool ran.
const SPINNER_PLACEHOLDER = '\x1F\x1F';

export const LiteLiveRegion: React.FC = () => {
  const isProcessing = useAppStore((s) => s.isProcessing);
  const messages = useAppStore((s) => s.messages);
  const retryStatus = useAppStore((s) => s.retryStatus);
  const thinkingContent = useAppStore((s) => s.thinkingContent);
  // Live model output for the in-flight turn. Subscribing to this primitive
  // (instead of deriving it from `messages` and the last Model row) means
  // per-chunk updates only re-render this component — they don't invalidate
  // every memo with `[messages]` deps in LiteLayout.
  const streamingContent = useAppStore((s) => s.streamingContent);
  // Per-tool streaming output buffers. The store flushes ToolCallUpdate
  // chunks into this map on a timer (a few times per second), keyed by
  // tool-call id; entries are deleted on ToolCallFinished so the live
  // region's preview disappears the moment the tool's static rendering
  // takes over. Subscribing here is the single binding that drives the
  // tool-output streaming preview — without it, lite mode showed nothing
  // below an in-flight tool until completion.
  const liveOutputs = useAppStore((s) => s.liveOutputs);
  const pendingApproval = useAppStore((s) => s.pendingApproval);
  const currentAgent = useAppStore((s) => s.currentAgent);
  // Shell-escape (`!command`) flag. When true, the in-flight turn is a PTY-
  // backed bash command, not agent inference: the live region collapses to
  // a single brand-purple `! `-gutter row that streams the bash output
  // directly. Spinners, thinking, tool batches, and the agent streaming
  // path are all suppressed for the duration — none apply to a bash PTY,
  // and showing an agent "thinking" indicator over interactive bash is
  // exactly the bug this branch fixes (mwinit/sudo/brew-OTP appearing
  // hung). The user's keystrokes still forward to the PTY via
  // AppContainer's always-armed shellEscapeWriter handler — this branch
  // only changes what the user SEES, not where their input goes.
  const isShellEscape = useAppStore((s) => s.isShellEscape);
  const pendingSwap = usePendingSwap();
  // Accessibility wiring (1:1 with modern TUI):
  //   - useGlyphs / useSpinners — switch box-drawing chars + spinner frame
  //     sets between Unicode and ASCII based on /settings allowAsciiArt
  //     (and the KIRO_ASCII_MODE=1 env override).
  //   - useAllowAsciiArt — read alongside useSpinners so the lite-specific
  //     pacman frame set can fall back to quarterSpinner in ASCII mode
  //     (the canonical Spinners type doesn't carry a 'pacman' field —
  //     it's a lite-only flourish).
  //   - useAnimationPaused — true when /settings allowAnimations is off.
  //     Used to skip the 150ms setInterval and hold a single static frame
  //     so users with motion-sensitive setups don't see anything cycling.
  const glyphs = useGlyphs();
  const spinners = useSpinners();
  const { allowAsciiArt } = useAllowAsciiArt();
  const animationPaused = useAnimationPaused();
  // Pacman in Unicode mode (lite UX flourish), quarterSpinner ASCII frames
  // when allowAsciiArt=false. Memoized on the toggle so the closure inside
  // the spinner interval reads a stable reference.
  const mainSpinnerFrames = useMemo<readonly string[]>(
    () => (allowAsciiArt ? PACMAN_SPINNER_FRAMES : spinners.quarterSpinner),
    [allowAsciiArt, spinners.quarterSpinner]
  );
  // Tool spinner uses the canonical brailleRotate set so the in-flight
  // tool row matches what Spinner.tsx renders elsewhere — and ASCII mode
  // degrades to the same `-\|/` rotation Spinner.tsx falls back to.
  const toolSpinnerFrames = spinners.brailleRotate;
  const { getColor, getUserPromptColor, getUserPromptBgHex } = useTheme();
  // Streaming `Kiro:` / `<agent>:` tag color follows the agent's assigned
  // palette color. Default agent uses brand; others hash to the 20-color
  // palette — matching the bar color shown on finalized messages.
  const agentTagFn = useMemo<(s: string) => string>(() => {
    try {
      const name = currentAgent?.name;
      if (!name) {
        const fn = getColor('brand');
        const probe = fn('');
        if (typeof probe !== 'string') return chalk.hex('#C19AFF');
        return (s: string) => fn(s);
      }
      const colorFn = getAgentColor(name, getColor);
      const probe = colorFn('');
      if (typeof probe !== 'string') return chalk.hex('#C19AFF');
      return (s: string) => colorFn(s);
    } catch {
      return chalk.hex('#C19AFF');
    }
  }, [getColor, currentAgent?.name]);
  // Full RenderTheme for the thinking block. Lets renderThinkingBlock paint
  // its purple rules with the same brand slot the rest of lite uses, so
  // /theme swaps reflow the live preview the same way they reflow scrollback.
  const renderTheme = useMemo(
    () => buildRenderTheme(getColor, getUserPromptColor, getUserPromptBgHex),
    [getColor, getUserPromptColor, getUserPromptBgHex]
  );

  const [frame, setFrame] = useState(0);
  const [elapsed, setElapsed] = useState(0);
  const thinkingStartRef = useRef(Date.now());
  const prevHadContentRef = useRef(false);
  // The elapsed counter is only displayed in the "thinking only" branch
  // (no live content, no active tools). When tools are running or text is
  // streaming, elapsed is invisible — gate setElapsed on this ref so we
  // don't force a re-render every 150ms for a value that nothing reads.
  // A ref is safe here: it's read inside the interval callback, never
  // affects what the render produces, and is updated on each render.
  const elapsedVisibleRef = useRef(true);
  // Same idea for the spinner glyph: when the live region is showing only
  // streaming model text (no tools, no thinking-only label, no retry status),
  // no spinner is on screen and bumping `frame` would re-render the whole
  // component for a value that nothing displays. Default true so the very
  // first interval tick is allowed; subsequent ticks read whatever the prior
  // render computed.
  const spinnerVisibleRef = useRef(true);
  // Tracks whether the previous tick was inside the no-activity branch
  // (no streaming text, no tools). When we transition INTO that branch
  // (after a tool finishes, between two model rounds), the elapsed counter
  // should restart from 0 — the indicator otherwise reads as a permanent
  // turn-elapsed clock instead of a per-round one.
  const prevIdleVisibleRef = useRef(false);

  // Reset thinking timer when a new thinking batch starts
  // (thinkingContent goes from empty to non-empty)
  useEffect(() => {
    const hasContent = !!thinkingContent;
    if (!prevHadContentRef.current && hasContent) {
      thinkingStartRef.current = Date.now();
      setElapsed(0);
    }
    prevHadContentRef.current = hasContent;
  }, [thinkingContent]);

  useEffect(() => {
    if (!isProcessing) {
      setElapsed(0);
      return;
    }
    thinkingStartRef.current = Date.now();
    // Animation-paused: hold the current frame and skip the elapsed counter
    // tick. The spinner's currently-rendered glyph is whatever index `frame`
    // points to — which stays put when we skip the interval. This matches
    // the modern TUI Spinner pattern (last-frame freeze) so users with
    // motion-sensitive setups see a steady glyph instead of a frozen-blank
    // slot. Elapsed counter resets to 0 to avoid showing a stale timestamp
    // from a prior round.
    if (animationPaused) {
      setElapsed(0);
      return;
    }
    const t = setInterval(() => {
      // Skip the React re-render entirely when the live region currently
      // shows only streaming model text — there's no spinner glyph or elapsed
      // counter on screen, so bumping `frame` would dirty the whole component
      // (and its tool-line substitution / renderThinkingBlock work) for zero
      // visual change. The same predicate also gates setElapsed.
      if (spinnerVisibleRef.current) {
        setFrame((f) => f + 1);
      }
      // Don't count elapsed while waiting for approval, or while a /agent
      // swap is mid-flight (the message is queued, the model isn't running
      // yet, so a climbing "thinking 4s" timer would lie to the user).
      // Also skip when the elapsed counter isn't visible (tools running or
      // text streaming) — the setState would just dirty React for no
      // visual change.
      if (!pendingApproval && !pendingSwap && elapsedVisibleRef.current) {
        setElapsed(Date.now() - thinkingStartRef.current);
      }
    }, SPINNER_INTERVAL);
    return () => clearInterval(t);
  }, [isProcessing, pendingApproval, pendingSwap, animationPaused]);

  // Reset the thinking start time the moment the swap clears so the timer
  // counts from when the model *actually* starts working, not from when the
  // queued message was submitted.
  useEffect(() => {
    if (!pendingSwap && isProcessing) {
      thinkingStartRef.current = Date.now();
      setElapsed(0);
    }
  }, [pendingSwap, isProcessing]);

  // Live streaming content comes straight from the streamingContent slot —
  // no walk over `messages`, no per-chunk memo invalidation downstream.
  const liveContent = isProcessing ? streamingContent : '';

  // Memoize the rendered thinking block so spinner ticks (every 150ms)
  // don't re-run wrapAnsiLine over the entire accumulated reasoning text.
  // The content is monotonic-append per round — only changes when the model
  // emits a new Thought chunk or when /theme swaps `renderTheme`. termCols
  // changes only on resize. Re-computing when any of those shift keeps the
  // block visually correct without paying the wrap cost on every tick.
  const termCols = process.stdout.columns ?? 80;
  const thinkingBlockMemo = useMemo(() => {
    if (!thinkingContent) return '';
    return renderThinkingBlock(thinkingContent, renderTheme, termCols, glyphs);
  }, [thinkingContent, renderTheme, termCols, glyphs]);

  // Live shell-escape output. The store seeds an empty Model row with
  // `shellOutput: true` when the user runs a `!` command, then streams
  // PTY chunks into its `content`. We find that row by walking from the
  // tail (it's the most recent Model message during shell escape) and
  // hand the body to {@link renderShellOutputBlock} for the brand-purple
  // `! ` left gutter. The same helper is also reachable via
  // `renderMessageToText` for committed scrollback rows, so the live →
  // static flush is a no-op visual transition: same gutter glyph, same
  // brand color, same line breaks.
  //
  // Empty content (PTY hasn't emitted anything yet) returns '' so the
  // JSX below can render an "executing" placeholder line — without
  // that, the user would see literally nothing for the first few hundred
  // ms while the bash process starts up. The placeholder lives in the
  // shell-escape return branch, not in this memo, so it doesn't fight
  // the empty-state contract that {@link renderShellOutputBlock} relies
  // on for empty-buffer reuse elsewhere.
  //
  // Memoized on (messages, renderTheme, termCols) — the messages
  // reference changes per chunk delivery (when content lands), and
  // /theme swaps reflow the gutter color via renderTheme. termCols is
  // here for API symmetry; the helper deliberately doesn't wrap (lets
  // the terminal soft-wrap so cursor-positioning escapes survive).
  const shellOutputBlockMemo = useMemo(() => {
    if (!isShellEscape) return '';
    let row: MessageType | null = null;
    for (let i = messages.length - 1; i >= 0; i--) {
      const m = messages[i]!;
      if (m.role !== MessageRole.Model) continue;
      if (!('shellOutput' in m && m.shellOutput)) continue;
      row = m;
      break;
    }
    if (!row) return '';
    return renderShellOutputBlock(row.content, renderTheme, termCols);
  }, [isShellEscape, messages, renderTheme, termCols]);

  // Memoize the streaming markdown render so spinner ticks / unrelated
  // re-renders (a tool starting alongside the model's prose, an approval
  // landing) don't pay for a full markdown parse + ANSI wrap pass over the
  // accumulated `liveContent`. The content is monotonic-append per round
  // (each chunk extends the buffer), so a fresh full re-parse on each
  // chunk delivery is correct — and that's the only time we actually
  // need new output. Empty buffer short-circuits to '' so the JSX below
  // can rely on the memo's truthiness without re-checking liveContent.
  //
  // Why route streaming text through the same `renderAgentMessage` that
  // finalized agent rows use: the rendered output is visually identical
  // to the eventual settled scrollback row — same `<agent>:` tag, same
  // markdown styling (bold, italic, code, links, lists, blockquotes,
  // tables, headers, code blocks), same line-wrap policy, same theme
  // colors. The flush from live region to <Static> becomes a no-op
  // visual transition rather than a "plain text → styled text" flicker.
  //
  // Bleed risk: the marked-based inline lexer treats unclosed `**`, `*`,
  // `_`, `` ` ``, `~~`, `[` as literal text (verified via probe: e.g.
  // `parseInlineMarkdown('**hello')` → `[{ text: '**hello' }]`). Block-
  // level markers commit immediately when complete; partial like `## H`
  // shows as a styled header which is fine. The wrapAnsiLine + ANSI
  // closer preservation infrastructure in render.ts (commits b5b834516
  // and 0b49875ec) is already battle-tested for finalized content; the
  // streaming path now reuses it unchanged.
  //
  // No `tryAppendMarkdownDelta` cache yet — typical streaming buffers
  // are small enough that the parse cost is dominated by React /
  // twinki render cost. If profiling shows this becomes a hotspot, the
  // modern TUI's MarkdownRenderer caching pattern transplants cleanly.
  const streamingBlockMemo = useMemo(() => {
    if (!liveContent) return '';
    return renderAgentMessage(
      liveContent,
      currentAgent?.name,
      renderTheme,
      termCols,
      // `renderAgentMessage` looks up the agent tag color by name. We've
      // already resolved that lookup once into `agentTagFn` above, so
      // wrap into the (name) => fn shape it expects and ignore the name
      // parameter — there's only one streaming agent per live region.
      () => agentTagFn,
      glyphs
    );
  }, [
    liveContent,
    currentAgent?.name,
    renderTheme,
    termCols,
    agentTagFn,
    glyphs,
  ]);

  // The "active tool batch" — the trailing run of tool messages from the
  // FIRST still-unfinished tool onward. Finished tools earlier in the run
  // (the contiguous "done prefix") are flushed to <Static> by LiteLayout as
  // soon as their predecessors complete, so the live region only shows
  // tools that are actually in flight (or that are blocked by an earlier
  // unfinished tool to preserve creation order in scrollback).
  //
  // Mirror the rule in static-flush.ts/computeActiveToolBatchIds — the two
  // walks must agree on which tool ids belong to the live region, otherwise
  // a finished tool could appear in both static and the live region.
  //
  // Inner subagent tools (any ToolUse with agentName != main agent) are
  // ignored entirely here — those render in the footer activity strip
  // instead, and the parent `subagent` tool is what surfaces in the chat
  // area. Treating them as invisible means they neither break the trailing
  // tool run nor force the parent into the live region a second time.
  const activeTools = useMemo(() => {
    if (!isProcessing) return [];
    const mainAgent = currentAgent?.name;
    const isInner = (m: (typeof messages)[number]) =>
      m.role === MessageRole.ToolUse &&
      !!m.agentName &&
      !!mainAgent &&
      m.agentName !== mainAgent;
    const isHidden = (m: (typeof messages)[number]) => isInner(m);
    // Walk from the end: find the trailing run of tool messages, skipping
    // inner subagent tool calls.
    let runStart = messages.length;
    for (let i = messages.length - 1; i >= 0; i--) {
      const m = messages[i]!;
      if (m.role !== MessageRole.ToolUse) break;
      if (isHidden(m)) continue;
      runStart = i;
    }
    // Find the first unfinished tool inside the run. Anything before it
    // is in the done prefix and lives in static, not here.
    let firstUnfinished = -1;
    for (let i = runStart; i < messages.length; i++) {
      const m = messages[i]!;
      if (m.role !== MessageRole.ToolUse) continue;
      if (isHidden(m)) continue;
      if (!m.isFinished) {
        firstUnfinished = i;
        break;
      }
    }
    if (firstUnfinished === -1) return [];
    const run = messages.slice(firstUnfinished).filter((m) => !isHidden(m));
    // Project only the fields the live region actually needs downstream:
    //   - id   → key for liveBarsByToolId / renderedToolBodies lookup
    //   - name → fed to renderLiveStreamingOutputBar's per-tool filter
    //   - isFinished → gates the live output bar (skip when settled)
    //   - msg  → reference to the original MessageType so renderMessageToText
    //            produces the canonical chat-log row (args, diff, reasoning,
    //            output) for in-flight tools — same body as the eventual
    //            settled rendering, just with SPINNER_PLACEHOLDER in the
    //            status slot to be substituted per tick.
    return run.map((m) => ({
      id: m.id,
      name: (m as any).name as string,
      isFinished: !!(m as any).isFinished,
      msg: m,
    }));
  }, [messages, isProcessing, currentAgent]);

  // Whether the live region needs a leading blank line. The rule mirrors the
  // static separator logic so the visible gap between sections is the same
  // before and after the live content lands in <Static> — preventing the
  // "chat shifted up by one row" effect on flush.
  //
  // The "next" role is whichever live content is about to render — a tool
  // batch, streaming model text, or nothing at all. Once we know prev/next,
  // delegate to the shared blank-rules helper so this branch stays in lockstep
  // with the canonical rules used by <Static> and the /verbosity preview.
  const needsLeadingSeparator = useMemo(() => {
    if (!isProcessing) return false;
    if (activeTools.length === 0 && !liveContent) return false;
    // Find the most recent message that would be visible in static.
    let prev: (typeof messages)[number] | undefined;
    for (let i = messages.length - 1; i >= 0; i--) {
      const m = messages[i]!;
      // Skip the currently-streaming model message — that lives in the live
      // region itself and isn't a previous static row.
      if (
        m.role === MessageRole.Model &&
        i === messages.length - 1 &&
        !m.standalone
      )
        continue;
      // Skip tools that are part of the active in-flight batch (they're in
      // the live region, not above it).
      if (m.role === MessageRole.ToolUse && !m.isFinished) continue;
      prev = m;
      break;
    }
    if (!prev) return false;
    // Tools take precedence — when a tool batch is in flight, the live region
    // opens with tool rows even if streaming text also arrived.
    const nextRole: 'tool_use' | 'model' =
      activeTools.length > 0 ? 'tool_use' : 'model';
    return needsLeadingBlankByRole(prev.role, nextRole);
  }, [messages, isProcessing, activeTools, liveContent]);

  // Per-tool live streaming output bars. For each in-flight tool the user
  // has output enabled for (via /verbosity filters), pull the accumulated
  // source lines out of `liveOutputs` and tail-window them through the
  // shared bar formatter so the live preview matches the eventual static
  // rendering exactly — same indent, same `│` glyph, same per-line char
  // clip, same line cap, same "+N more lines above" marker phrasing.
  //
  // Hoisted ABOVE the `if (!isProcessing) return null` and the retryStatus
  // early returns because React hooks must be called unconditionally — moving
  // the useMemo below those branches would change the hook call order across
  // renders and trip rules-of-hooks. The cost is unchanged: when not
  // processing, activeTools is `[]` (the prior memo bails on isProcessing)
  // so this loop body never runs.
  //
  // The map is recomputed when liveOutputs identity, the active-tools list,
  // the cap inputs, or the saved filters change. A spinner tick that doesn't
  // touch any of those slots reuses the prior map — important because the
  // 150ms spinner cadence would otherwise rewrap output buffers six times a
  // second.
  const display = getVerboseDisplay();
  const filtersOverride = useMemo(() => getVerboseConfig().filters, []);
  const liveBarsByToolId = useMemo(() => {
    const out = new Map<string, string[]>();
    for (const tool of activeTools) {
      // Skip finished tools — they're rendered to <Static> by LiteLayout
      // and their liveOutputs entry has already been cleared by the store's
      // ToolCallFinished handler. Defensive: even if cleanup races, a
      // finished tool's live bar would duplicate the static bar that's
      // about to land just above it.
      if (tool.isFinished) continue;
      const sourceChunks = liveOutputs.get(tool.id);
      if (!sourceChunks || sourceChunks.length === 0) continue;
      // main bumped liveOutputs to chunks (string[][]) for O(1) append on
      // hot stdout. The bar formatter still works in flat lines, so flatten
      // at the boundary. The tail-window cap inside the formatter bounds
      // the work it does — flatten cost is O(total lines) per render, but
      // these arrays are small relative to terminal output volume.
      const sourceLines = sourceChunks.flat();
      const bar = renderLiveStreamingOutputBar(tool.name, sourceLines, {
        outputMaxLines: display.outputMaxLines,
        outputMaxChars: display.outputMaxChars,
        termCols,
        filtersOverride,
        glyphs,
      });
      if (bar.length > 0) out.set(tool.id, bar);
    }
    return out;
  }, [
    activeTools,
    liveOutputs,
    display.outputMaxLines,
    display.outputMaxChars,
    termCols,
    filtersOverride,
  ]);

  // Canonical render of each in-flight tool's chat-log row. Same formatter
  // that produces the eventual settled scrollback row, so a running shell
  // / fs_write / fs_read shows its full args, diff body, or file body
  // immediately — fixing the bug where a trusted tool ran to completion
  // while the live region only ever showed `<tool> <one-arg-or-purpose> ⠋`,
  // hiding the actual command/path/diff from the user.
  //
  // We render with `runningSpinner: SPINNER_PLACEHOLDER` so the heavy work
  // (JSON.parse, tree formatting, diff rendering, markdown wrap) only re-
  // runs when one of the deps below actually shifts — content delivery,
  // approval state, terminal width, theme, or which tool joined/left the
  // batch. Per-render we just `replaceAll` the placeholder with the active
  // spinner glyph; the substitution is O(line length) and runs once per
  // tool per frame instead of O(args+diff) per tool per frame.
  //
  // Theme, display config, and filters: the helper reads these via its own
  // ctx fallbacks (theme via `buildRenderTheme` here, display/filters via
  // getVerboseDisplay / getVerboseConfig at call time). Theme identity is
  // tracked in deps so /theme swaps reflow the live preview the same way
  // they reflow scrollback.
  const renderedToolBodies = useMemo(() => {
    const out = new Map<string, string>();
    // Per-stage color resolver — same lookup `LiteLayout`'s static-rendering
    // ctx uses (LiteLayout.tsx:1042-1053). Without these resolvers, the
    // subagent tool's pipeline tree fell back to chalk.blue (#0000ee) for
    // every [stage] tag while the tool was in flight, then snapped to per-
    // agent palette shades the moment the tool finalized and migrated to
    // <Static>. The shift was visible as a color flicker on settle. Same
    // resolver covers input chip, output chip, and agent role-tag color.
    const stageColor = (stageName: string) =>
      getAgentColor(stageName, getColor);
    const ctx: RenderContext = {
      pendingApprovalToolCallId: pendingApproval?.toolCall.toolCallId ?? null,
      termCols,
      theme: renderTheme,
      filtersOverride,
      runningSpinner: SPINNER_PLACEHOLDER,
      glyphs,
      getStageInputColor: stageColor,
      getStageOutputColor: stageColor,
      getAgentTagColor: stageColor,
    };
    for (const tool of activeTools) {
      const text = renderMessageToText(
        tool.msg as MessageType,
        currentAgent?.name,
        ctx
      );
      if (text) out.set(tool.id, text);
    }
    return out;
  }, [
    activeTools,
    pendingApproval,
    termCols,
    renderTheme,
    filtersOverride,
    currentAgent?.name,
    getColor,
  ]);

  if (!isProcessing) return null;

  // Shell-escape branch: collapse the live region to a single brand-purple
  // `! ` gutter row that streams the PTY's output. The body comes from the
  // shell-output Model message in app-store; this branch just paints it.
  //
  // Why this fires BEFORE the spinner / retry / tool / thinking branches:
  // none of those concepts apply to a `!` command. The store sets
  // `isProcessing: true` (so the input box renders the right chrome and
  // Ctrl+C still cancels), but agent inference is NOT running. The bug
  // we're fixing here is exactly that — without this branch, the live
  // region falls into the "thinking..." idle case and the user sees an
  // infinite spinner over a bash command that's actually waiting for
  // their input.
  //
  // Empty buffer placeholder: bash hasn't emitted anything yet (the
  // process is spawning, or the program is reading input before printing
  // a prompt). Show a dim "executing..." line so the user gets immediate
  // feedback that their `!` command landed. The line uses the same `! `
  // gutter color so it visually matches the streamed output that's about
  // to replace it. wrap="overflow" mirrors the policy on every other
  // shell row — the renderShellOutputBlock body isn't wrapped, so the
  // terminal soft-wraps long lines visually without us baking \n into
  // the buffer (which would mangle programs that emit cursor escapes).
  if (isShellEscape) {
    spinnerVisibleRef.current = false;
    elapsedVisibleRef.current = false;
    if (shellOutputBlockMemo) {
      return <Text wrap="overflow">{shellOutputBlockMemo}</Text>;
    }
    const gutter = (renderTheme.brand ?? chalk.hex('#C19AFF'))('! ');
    return (
      <Text wrap="overflow">
        {gutter}
        {chalk.dim('executing...')}
      </Text>
    );
  }

  // Spinner glyphs sourced from the active frame sets — pacman in Unicode,
  // quarterSpinner in ASCII (main); brailleRotate in Unicode, the canonical
  // `-\|/` rotation in ASCII (tool). Brand color stays the same across modes
  // — the accessibility toggles only swap the GLYPH, not the color.
  const spinner = chalk.hex('#C19AFF')(
    mainSpinnerFrames[frame % mainSpinnerFrames.length]
  );
  const toolSpinner = chalk.hex('#C19AFF')(
    toolSpinnerFrames[frame % toolSpinnerFrames.length]
  );

  if (retryStatus) {
    // Retry banner has the spinner glyph attached to the status message — keep
    // ticking so the user sees the retry in motion.
    spinnerVisibleRef.current = true;
    return (
      <Text>
        {spinner} {chalk.yellow(retryStatus.message)}
      </Text>
    );
  }

  // Tool call lines — one per active tool. Each row is the canonical
  // chat-log render baked at memo-build time with SPINNER_PLACEHOLDER in
  // the running-status slot; here we substitute the active spinner glyph.
  // Trivial tools' rendered text contains no placeholder (renderToolCall
  // ignores the spinner override for them), so `replaceAll` is a no-op
  // there — they keep their dim ' ...' suffix. Finished tools held in the
  // batch (waiting on an earlier in-flight tool to preserve creation order
  // in scrollback) also have no placeholder — they show their post-run
  // status (elapsed, FAILED, cancelled) verbatim.
  const toolLines = activeTools.map((tool) => {
    const body = renderedToolBodies.get(tool.id);
    if (!body) return '';
    return body.includes(SPINNER_PLACEHOLDER)
      ? body.replaceAll(SPINNER_PLACEHOLDER, toolSpinner)
      : body;
  });
  // Mirror "is the elapsed counter currently visible?" into the ref the
  // interval reads. The thinking-only branch below is the only place
  // elapsed renders, so the ref is true exactly when both inputs are empty.
  const idleVisible = !liveContent && toolLines.length === 0;
  elapsedVisibleRef.current = idleVisible;
  // Mirror "is a spinner glyph currently on screen?" into the ref the
  // interval reads. The spinner shows in three places: idle/thinking branch
  // (always), tool batch branch (when at least one tool is rendered), and the
  // thinking-block header (when /verbose · Thinking content is on with text).
  // Streaming-only with no tools and no thinking has no spinner — skip the
  // setFrame in that case so the whole component doesn't re-render every
  // 150ms during long agent text streams.
  spinnerVisibleRef.current =
    idleVisible || toolLines.length > 0 || !!thinkingContent;
  // Round-boundary reset: when we re-enter the idle state after streaming
  // text or a tool batch (both common between model rounds), the counter
  // should start over from 0. Without this, the same "thinking 47s" value
  // persists across the whole turn — readers can't tell whether the model
  // is stuck on this round or just took a long total time.
  if (idleVisible && !prevIdleVisibleRef.current) {
    thinkingStartRef.current = Date.now();
    // setElapsed(0) is safe in render: React batches the update and re-renders
    // without infinite-looping because prevIdleVisibleRef will be true on the
    // next pass. Without this, the stale `elapsed` value lingers for one
    // 150ms tick before the interval refreshes it.
    if (elapsed !== 0) setElapsed(0);
  }
  prevIdleVisibleRef.current = idleVisible;

  // Thinking state: no content streaming, no tools running
  if (!liveContent && toolLines.length === 0) {
    const secs = Math.floor(elapsed / 1000);
    const timeStr = secs > 0 ? chalk.dim(` ${secs}s`) : '';
    // While an /agent swap is mid-flight, the message the user just submitted
    // is held by the dispatcher until the swap RPC returns — the agent isn't
    // actually thinking yet. Render "queued" instead of "thinking" so the
    // spinner doesn't lie about what's happening; the footer's pending chip
    // already names the target agent.
    if (pendingSwap) {
      return (
        <Text>
          {spinner} {chalk.dim('queued')}
        </Text>
      );
    }
    if (thinkingContent) {
      // Thinking content is gated by /verbose · Thinking content. When off,
      // we still surface the spinner with the "thinking" label (so the user
      // knows the model is reasoning) but skip the live preview itself —
      // matches the lean / minimal density presets where the body would be
      // visual noise.
      const showThinking = getVerboseDisplay().showThinkingContent;
      if (!showThinking) {
        return (
          <Text>
            {spinner} {chalk.dim.italic('thinking')}
            {timeStr}
          </Text>
        );
      }
      // Render the same purple-bordered block we use in scrollback so the
      // live preview matches the finalized output 1:1 — the prior version
      // showed only the trailing 3 lines as plain dim italic, which made
      // earlier reasoning chunks vanish as new ones arrived. renderThinkingBlock
      // returns top rule + full body + bottom rule; the body wraps at the
      // terminal width via the same wrapAnsiLine pipeline static rows use.
      // Reuse the memoized output so spinner ticks don't re-wrap the body.
      const block = thinkingBlockMemo;
      return (
        <Box flexDirection="column">
          <Text>
            {spinner} {chalk.dim.italic('thinking')}
            {timeStr}
          </Text>
          {block && <Text>{block}</Text>}
        </Box>
      );
    }
    // No thinking tokens, no streaming, no active tools — render the plain
    // "thinking" label. We don't have a reliable signal for whether the
    // agent is actually waiting on inference vs. between rounds, so we
    // don't try to draw that distinction.
    return (
      <Text>
        {spinner} {chalk.dim('thinking')}
        {timeStr}
      </Text>
    );
  }

  // Bake the leading blank into the first row's text rather than rendering it
  // as a sibling `<Text> </Text>`. A single-space Text element is normalized to
  // an empty string by twinki's wrap pipeline (every wrapped line is
  // `trimEnd()`-ed before flush), and an empty Text inside a flex column
  // collapses to zero rows on the next render — the gap doesn't appear until
  // the row that follows it commits content. That's what made the user see
  // their prompt glued to a pending tool until the tool finished. Inlining the
  // newline guarantees the visual gap is part of the row that needs it, so
  // it survives every render pass.
  // Thinking content is gated by /verbose · Thinking content. When off, the
  // thinking preview line above tools is suppressed entirely — same rule as
  // the thinking-only branch above. The active tool block still renders
  // normally; we just don't show the live thinking text alongside it.
  //
  // When showThinkingContent is on, the live preview persists across tools
  // and streaming rather than disappearing the moment a tool fires or text
  // starts streaming. The block accumulates every Thought chunk emitted on
  // this round, so prior reasoning paragraphs stay readable while the model
  // moves into spoken text — matching the scrollback section that lands at
  // commit time.
  const showThinkingContent = getVerboseDisplay().showThinkingContent;
  const showThinkingPreviewBlock = showThinkingContent && !!thinkingContent;
  // Reuse the memoized rendered block — interleaved with the leading-separator
  // logic below. Empty string when the helper declines to render
  // (whitespace-only thinking or terminal too narrow) or the user has the
  // /verbose preview off.
  const thinkingBlockText = showThinkingPreviewBlock ? thinkingBlockMemo : '';
  const hasThinkingBlock = !!thinkingBlockText;
  // Order of rows in this region: [optional leading blank] → thinking block →
  // [blank between block and tools/streaming] → tools → [blank between tools
  // and streaming] → streaming text. The blanks below the thinking block live
  // inside the block's own Text element (baked as trailing '\n') so a single
  // empty <Text> sibling can't collapse out — same trick we use above for
  // the leading separator.
  const thinkingBlockWithBreaks = hasThinkingBlock
    ? (needsLeadingSeparator ? '\n' : '') +
      thinkingBlockText +
      // Trailing blank when content follows the block, so the bottom rule
      // doesn't glue to the first tool line or the streaming `Kiro:` row.
      (toolLines.length > 0 || liveContent ? '\n' : '')
    : null;
  // When the thinking block is taking the leading-separator slot, the tool
  // rows + streaming text don't need to bake their own — the block already
  // emitted it.
  const firstToolWithBreak =
    needsLeadingSeparator && toolLines.length > 0 && !hasThinkingBlock
      ? '\n' + toolLines[0]!
      : (toolLines[0] ?? null);
  const standaloneStreamingWithBreak =
    needsLeadingSeparator &&
    liveContent &&
    toolLines.length === 0 &&
    !hasThinkingBlock
      ? '\n' + streamingBlockMemo
      : null;

  return (
    <Box flexDirection="column">
      {/* Persistent thinking block — purple-bordered, full body. Lives above
          both tools and streaming text so reasoning chunks stay visible
          while the model speaks or runs tools, instead of vanishing the
          instant a tool/content event arrives. */}
      {hasThinkingBlock && <Text>{thinkingBlockWithBreaks}</Text>}
      {/* Active tool calls — first row carries the leading blank when no
          thinking block above already supplied it. Each tool's line is
          followed by its streaming output bar (if any) so the user sees
          tool output flow in as it arrives, with the tail-window marker
          ("+N more lines above") rendered ABOVE the visible window. The
          bar is concatenated onto the tool line's text via newlines so
          twinki treats them as a single Static-eligible block — twinki's
          flex column collapses single-empty Text rows during commit, and
          a separate <Text> sibling for the bar would race the tool line's
          mount/update on each spinner tick. */}
      {toolLines.map((line, i) => {
        const tool = activeTools[i]!;
        const head =
          i === 0 && firstToolWithBreak != null && !hasThinkingBlock
            ? firstToolWithBreak
            : line;
        const bar = liveBarsByToolId.get(tool.id);
        const body =
          bar && bar.length > 0 ? `${head}\n${bar.join('\n')}` : head;
        // wrap="overflow" — the tool body is pre-formatted by the canonical
        // chat-log renderer with its own structural \n at the boundaries
        // that need them (between args lines, between diff lines, between
        // bar-prefixed output rows). Twinki's default would re-wrap any
        // logical line wider than the terminal, baking extra \n into
        // long shell command lines / file paths / output rows. Overflow
        // mode preserves single-line semantics so users copying a long
        // command from a still-running shell tool get one logical line.
        return (
          <Text key={tool.id} wrap="overflow">
            {body}
          </Text>
        );
      })}
      {/* Separator between tools and streaming content */}
      {liveContent && toolLines.length > 0 && <Text> </Text>}
      {/* Streaming content rendered through the same markdown pipeline as
          finalized agent rows (`renderAgentMessage` → `renderMarkdownToLines`).
          Bold, italic, code spans, links, lists, headers, blockquotes,
          code blocks, and tables all style as they stream — visually
          identical to the eventual settled scrollback row, so the
          live→static flush is a no-op transition. Marked treats unclosed
          inline emphasis as literal text (e.g. `**partial` stays plain)
          so a half-arrived `**bold**` doesn't bleed into anything that
          follows. When this is the only live row (no tools, no thinking),
          it carries the leading blank itself. wrap="overflow" mirrors
          the policy on the Static <Text> in LiteLayout — `renderAgentMessage`
          emits the stream's own \n and only those, so URLs / long sentences
          flowing in mid-stream copy as one line just like they do once
          the message finalizes into scrollback. */}
      {liveContent && (
        <Text wrap="overflow">
          {standaloneStreamingWithBreak ?? streamingBlockMemo}
        </Text>
      )}
    </Box>
  );
};
