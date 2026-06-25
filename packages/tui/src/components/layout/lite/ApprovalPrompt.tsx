import React, { useMemo, useState, useEffect } from 'react';
import { Box, Text } from '../../../renderer.js';
import {
  MessageRole,
  type MessageType,
  useAppStore,
} from '../../../stores/app-store.js';
import {
  ApprovalOptionId,
  isParentSubagentTool,
  type PermissionOption,
  type TrustOption,
} from '../../../types/agent-events.js';
import { useKeypress } from '../../../hooks/useKeypress.js';
import { PromptInput } from '../../chat/prompt-bar/PromptInput.js';
import { useTheme } from '../../../hooks/useThemeContext.js';
import {
  buildRenderTheme,
  formatToolArgLines,
  formatSubagentApprovalLines,
  renderUnifiedDiff,
} from '../../../lite/render.js';
import { useGlyphs, useAllowIcons } from '../../../hooks/useGlyphs.js';
import chalk from 'chalk';

export const WRITE_TOOL_NAMES = new Set([
  'fs_write',
  'str_replace',
  'write',
  'edit',
  'create_file',
  'write_file',
]);

export function ApprovalPrompt({
  messages,
  approval,
  respondToApproval,
  getStageInputColor,
  mainAgentName,
  onNotesSubmit,
}: {
  messages: MessageType[];
  approval: any;
  respondToApproval: (
    optionId: string,
    target?: any,
    _meta?: Record<string, unknown>
  ) => void;
  /**
   * Flush a staged approval note to the model as a mid-turn steer. Pressing
   * [tab] on the default page swaps the y/t/n hotkey row for a feedback input;
   * submitting it STAGES the text (see stagedNote) without resolving the
   * approval. respondWithNote then calls this just BEFORE sending the chosen
   * y/t/n disposition, so the note rides as a follow-up user turn while the
   * tool still gets the disposition the user picked. The parent (LiteLayout)
   * wires this to handleUserInput; it must NOT cancel the approval (the old
   * cancel-and-inject behavior force-denied every noted tool). respondWithNote
   * only calls this with a non-empty trimmed note.
   */
  onNotesSubmit: (value: string) => void;
  /**
   * Resolves a stage name to its per-agent input color. Used so the subagent
   * approval pipeline tree shows each stage in the same shade as the chat-log
   * final block / footer activity strip — eye matches one cue across views.
   */
  getStageInputColor: (stageName: string) => (text: string) => string;
  /**
   * Name of the currently-active main agent, used to gate the "subagent
   * request" chip. The chip should only appear when a stage running inside
   * a subagent pipeline asks for approval — parent-agent tool calls also
   * carry an `agentName` (the parent's own name), so a presence check alone
   * was firing the chip on every approval prompt.
   */
  mainAgentName?: string | null;
}) {
  const glyphs = useGlyphs();
  const { allowIcons } = useAllowIcons();
  // Per-render theme so the unified diff below picks up the active /settings
  // theme (kiroDark ↔ kiroLight). Built here rather than threaded via prop —
  // ApprovalPrompt is a self-contained inline surface.
  const { getColor, getUserPromptColor, getUserPromptBgHex } = useTheme();
  const renderTheme = useMemo(
    () => buildRenderTheme(getColor, getUserPromptColor, getUserPromptBgHex),
    [getColor, getUserPromptColor, getUserPromptBgHex]
  );
  // Raw tool name (not the title, which may include args).
  const toolMsg = messages.find(
    (m) =>
      m.role === MessageRole.ToolUse && m.id === approval.toolCall.toolCallId
  );
  const rawToolName =
    toolMsg && toolMsg.role === MessageRole.ToolUse ? toolMsg.name : null;
  const toolName = rawToolName || approval.toolCall.title || 'tool';

  // Subagent attribution: only mark the prompt as a subagent request when
  // the requesting tool's agentName actually differs from the main agent's
  // name. The store also tags parent-agent tool calls with the parent's
  // own name, so a bare presence check fired the "subagent request" chip
  // on every approval — making the parent's own calls look like they came
  // from a subagent stage.
  const requestingStageName =
    toolMsg &&
    toolMsg.role === MessageRole.ToolUse &&
    toolMsg.agentName &&
    toolMsg.agentName !== mainAgentName
      ? toolMsg.agentName
      : null;
  const stageHeader = requestingStageName ? (
    <Text>
      {getStageInputColor(requestingStageName)(`[${requestingStageName}]`)}{' '}
      {chalk.dim('subagent request')}
    </Text>
  ) : null;

  const detail = (() => {
    const raw = approval.toolCall.rawInput;
    if (raw)
      return extractApprovalDetail(
        toolName,
        typeof raw === 'string' ? raw : JSON.stringify(raw)
      );
    if (toolMsg && toolMsg.role === MessageRole.ToolUse) {
      return extractApprovalDetail(toolMsg.name, toolMsg.content);
    }
    return null;
  })();

  // Trust submenu state. Default page = primary [y]/[t]/[n] hotkeys. When the
  // backend ships granular trust tiers (shell argv-prefix or fs path scopes),
  // [t] flips to a tier picker. When the backend declined to scope (parser
  // failure, danger-level detection, MCP tools), [t] just trusts the whole
  // tool directly — the hotkey row's label spells that out so a single
  // keystroke can't surprise anyone. No intermediate confirm page.
  const trustOptions: TrustOption[] = approval.trustOptions ?? [];
  const hasTrustTiers = trustOptions.length > 0;
  const [page, setPage] = useState<'default' | 'trust' | 'notes'>('default');
  // +1 row at the end of the submenu for "Trust entire tool". Reset to top
  // every time the page is opened.
  const [trustIdx, setTrustIdx] = useState(0);
  // The note typed on the [tab] feedback page, held until the user picks the
  // real y/t/n disposition. Pressing Enter in the notes box *stages* the text
  // here and returns to the picker — it does NOT submit/deny the tool (the old
  // behavior cancelled the approval, marking every noted request DENIED). The
  // staged note is flushed to the model as a follow-up turn once a disposition
  // is chosen (see flushStagedNote).
  const [stagedNote, setStagedNote] = useState('');

  // Reset to the default page whenever the approval changes (e.g. a sibling
  // tool in a concurrent batch resolves and the next one slides in). Without
  // this, a user who opened the trust submenu for tool A would see it still
  // open when tool B's approval lands — pressing Enter then trusts B before
  // they've had a chance to see B at all. Keyed on toolCallId so the reset
  // fires once per request.
  const approvalToolCallId: string | null =
    approval.toolCall.toolCallId ?? null;
  useEffect(() => {
    setPage('default');
    setTrustIdx(0);
    setStagedNote('');
  }, [approvalToolCallId]);

  const opts = approval.permissionOptions || [];
  const findOpt = (kind: ApprovalOptionId) =>
    opts.find((o: PermissionOption) => o.kind === kind)?.optionId ?? kind;
  const allowAlwaysId = findOpt(ApprovalOptionId.AllowAlways);
  const cancelMessage = useAppStore((s) => s.cancelMessage);
  const setCommandInput = useAppStore((s) => s.setCommandInput);

  // Open the notes page, seeding the shared input slot with any prior staged
  // note so [tab] genuinely edits the existing text (the mounted PromptInput
  // syncs from commandInputValue on mount). The main lite input is unmounted
  // while the approval shows (LiteLayout gates it on !showApproval), so this
  // seed can't leak into it.
  const openNotesPage = () => {
    // Always seed (empty when no staged note) so a stale global compose value
    // — e.g. a stray key spammed at the y/t/n row — can't prefill the box.
    setCommandInput(stagedNote ?? '');
    setPage('notes');
  };

  // Stash the typed note and return to the y/t/n row WITHOUT resolving the
  // approval. PromptInput has already cleared the shared input slot via its
  // clearAll() on submit, so nothing leaks into the main input.
  const handleStageNote = (value: string) => {
    setStagedNote(value);
    setPage('default');
  };

  // Flush the staged note as a steer FIRST, then send the chosen disposition.
  //
  // The backend approval response can't carry free text (the v2
  // ApprovalResult.reason field is ignored by handle_approval_result and the
  // deny path hardcodes its reason), so the note rides as a mid-turn steer via
  // onNotesSubmit. Ordering matters: SteerMessage and SendApprovalResult are
  // two independent ACP round-trips that the backend actor processes in
  // arrival order off a single mailbox. The note MUST be buffered before the
  // disposition is processed — on REJECT the deny path resumes the model
  // immediately, draining the steering buffer right then; if the steer hasn't
  // landed yet the note slips to the next turn (the "queued for afterwards"
  // bug). On approve/trust the tools run in the background, so the window is
  // wide and either order happens to work — but sending the note first makes
  // all three dispositions deterministic.
  //
  // Sending the note first is safe now that handleNotesSubmit no longer cancels
  // the approval (the old force-deny path): it only buffers a steer, leaving the
  // tool to take the disposition the user actually picks one line below.
  const respondWithNote = (
    optionId: string,
    meta?: Record<string, unknown>
  ) => {
    const trimmed = stagedNote.trim();
    if (trimmed) onNotesSubmit(trimmed);
    respondToApproval(optionId, undefined, meta);
  };

  useKeypress((input, key) => {
    if (page === 'default') {
      // Esc / Ctrl+C interrupt the agent's current turn in addition to
      // dismissing this approval. cancelMessage() calls cancelApproval()
      // internally, so any sibling approvals in the batch are also
      // dropped — which is the right thing when the user is bailing out
      // to give the agent new instructions. Use `n` to deny just this
      // one and keep the queue going.
      if (key.escape || (key.ctrl && input === 'c')) {
        cancelMessage();
        return;
      }
      // Tab → notes page. Parity with the full TUI, where Tab (or Right
      // arrow) flips the dropdown into drill-in mode. We use Tab only:
      // shift+letter isn't reliably detectable (see keybindings.ts:141-143),
      // and Tab is modifier-independent, currently free in this handler, and
      // matches the muscle memory TUI users already have. The notes-page
      // PromptInput owns Tab once mounted (it uses Tab for completion/expand),
      // and because this handler branches on `page` the default-page Tab
      // handler can't fire again while page === 'notes'.
      if (key.tab) {
        openNotesPage();
        return;
      }
      if (input === 'y' || input === 'Y')
        respondWithNote(findOpt(ApprovalOptionId.AllowOnce));
      else if (input === 'n' || input === 'N')
        respondWithNote(findOpt(ApprovalOptionId.RejectOnce));
      else if (input === 't' || input === 'T') {
        if (hasTrustTiers) {
          setTrustIdx(0);
          setPage('trust');
        } else {
          // No backend tiers — `[t]` trusts the whole tool directly. The
          // hotkey row spells this out (`[t] TRUST whole tool`) so the
          // single keystroke can't surprise the user.
          respondWithNote(allowAlwaysId);
        }
      }
      return;
    }
    if (page === 'notes') {
      // Esc steps back to the default page only — never interrupts the turn.
      // The layout-level always-armed Esc/Ctrl+C handler no-ops while
      // pendingApproval is set (LiteLayout.tsx ~446: it calls cancelMessage()
      // only when `isProcessing && !pendingApproval`), so this setPage wins
      // and Esc can't abort the running turn. Mirrors the trust submenu's
      // Esc-back below. The notes-page PromptInput is mounted and consumes
      // printable keys / Enter; it does not claim a bare Esc (its Esc handling
      // is gated on reverse-search / queue-restore state, neither active here),
      // so this handler sees Esc cleanly.
      if (key.escape) {
        setPage('default');
        return;
      }
      // Enter is handled by the mounted PromptInput via handleStageNote, which
      // stashes the text and returns to the y/t/n row; y/t/n are literal
      // characters typed into the feedback box, not hotkeys.
      return;
    }
    // page === 'trust'
    const rowCount = trustOptions.length + 1; // tiers + entire-tool
    if (key.upArrow) {
      setTrustIdx((i) => (i - 1 + rowCount) % rowCount);
    } else if (key.downArrow) {
      setTrustIdx((i) => (i + 1) % rowCount);
    } else if (key.return) {
      if (trustIdx < trustOptions.length) {
        respondWithNote(allowAlwaysId, {
          trustOption: trustOptions[trustIdx],
        });
      } else {
        respondWithNote(allowAlwaysId);
      }
    } else if (key.escape) {
      // Esc inside the submenu only steps back; the always-armed Ctrl+C/Esc
      // handler at the layout level no-ops while pendingApproval is set, so
      // there's no double-handle risk.
      setPage('default');
    }
  });

  const detailStr = detail
    ? Array.isArray(detail)
      ? detail[0]
      : detail
    : null;

  // Extract reasoning. Prefer the typed `purpose` sibling stamped onto
  // ToolUse messages by app-store.ts's ToolCall handler — that's the
  // only surface where the model's `__tool_use_purpose` survives for
  // edit-kind tools, whose `content` JSON is rebuilt by the synthesis
  // path and stripped of the field. Fall back to parsing `content` for
  // tool kinds whose handler keeps __tool_use_purpose in the JSON blob
  // (the generic `else` branch in the ToolCall handler).
  const reasoning = (() => {
    if (!toolMsg || toolMsg.role !== MessageRole.ToolUse) return null;
    if (typeof toolMsg.purpose === 'string' && toolMsg.purpose.trim()) {
      return toolMsg.purpose;
    }
    if (!toolMsg.content) return null;
    try {
      const args = JSON.parse(toolMsg.content);
      return args.__tool_use_purpose || null;
    } catch {
      return null;
    }
  })();

  // Write tools render a proper unified diff instead of the key:value args
  // dump; falls through to the generic printer when args don't parse.
  const writeDiffLines = (() => {
    if (!toolMsg || toolMsg.role !== MessageRole.ToolUse) return null;
    if (!WRITE_TOOL_NAMES.has(toolMsg.name)) return null;
    try {
      const args = JSON.parse(toolMsg.content);
      // Wire format is snake_case (Rust serde — see crates/chat-cli/src/cli/
      // chat/tools/fs_write.rs `enum FsWrite`). Accept camelCase as fallback
      // so non-Rust callers (KAS native tool, future MCP-routed write tools)
      // keep working — same convention as renderWriteToolCall.
      const oldStr = args.old_str ?? args.oldStr;
      const newStr = args.new_str ?? args.newStr;
      const fileText = args.file_text ?? args.content;
      const insertLine = args.insert_line ?? args.insertLine;
      let oldText = '';
      let newText = '';
      let startLine = 1;
      if (
        args.command === 'str_replace' ||
        args.command === 'strReplace' ||
        (oldStr && newStr != null)
      ) {
        oldText = String(oldStr ?? '');
        newText = String(newStr ?? '');
      } else if (args.command === 'insert' || insertLine != null) {
        newText = String(newStr ?? fileText ?? '');
        if (typeof insertLine === 'number') startLine = insertLine + 1;
      } else if (args.command === 'append') {
        newText = String(newStr ?? fileText ?? '');
      } else if (
        args.command === 'create' ||
        (!oldStr && (fileText != null || newStr != null))
      ) {
        newText = String(fileText ?? newStr ?? '');
      } else {
        return null;
      }
      return renderUnifiedDiff(oldText, newText, {
        path: args.path,
        startLine,
        termCols: process.stdout.columns ?? 80,
        // Pass through the per-render theme so the bg + bar colors of
        // the diff above the y/t/n hotkey row follow /settings theme
        // (kiroDark ↔ kiroLight) — matching what the diff looks like
        // when it later commits to scrollback via renderWriteToolCall.
        theme: renderTheme,
      });
    } catch {
      return null;
    }
  })();

  // Pretty-print tool args with the same indented printer the chat log uses,
  // so the approval prompt and finalized scrollback match. The `subagent`
  // tool gets its own per-stage renderer so the user sees the pipeline
  // structure (task, stages with role/depends_on/prompt) instead of a raw
  // JSON dump that wraps awkwardly in the terminal.
  const toolArgsLines = (() => {
    if (writeDiffLines) return null;
    if (!toolMsg || toolMsg.role !== MessageRole.ToolUse) return null;
    if (isParentSubagentTool(toolMsg.name)) {
      return formatSubagentApprovalLines(
        toolMsg.content,
        process.stdout.columns,
        {
          getStageInputColor,
          glyphs,
        }
      );
    }
    return formatToolArgLines(toolMsg.name, toolMsg.content);
  })();

  if (page === 'trust') {
    const rows = [
      ...trustOptions.map((t) => ({ label: t.label, display: t.display })),
      { label: 'Trust entire tool', display: toolName },
    ];
    return (
      <Box flexDirection="column">
        {stageHeader}
        <Text>
          {chalk.yellow.bold(toolName)} {chalk.dim('· trust scope')}
        </Text>
        {rows.map((row, i) => {
          const focused = i === trustIdx;
          // Disabling allowIcons hides the focus indicator entirely (replaced
          // by an empty 2-col pad so the focused/unfocused rows still align).
          const arrow = focused
            ? allowIcons
              ? chalk.cyan(`${glyphs.chevron} `)
              : '  '
            : '  ';
          const label = focused ? chalk.cyan(row.label) : row.label;
          const display = row.display ? chalk.dim(`  ${row.display}`) : '';
          return (
            <Text key={i}>
              {arrow}
              {label}
              {display}
            </Text>
          );
        })}
        <Text>{chalk.dim('[↑↓] select  [enter] confirm  [esc] back')}</Text>
      </Box>
    );
  }

  return (
    <Box flexDirection="column">
      {stageHeader}
      <Text>
        {chalk.yellow.bold(toolName)} {chalk.dim('needs approval')}
      </Text>
      {writeDiffLines &&
        writeDiffLines.map((line, i) => (
          // wrap="overflow" — each diff line is a single logical source
          // line. The terminal soft-wraps long lines visually; the bg
          // tint, syntax highlighting, and gutter all rely on SGR state
          // carrying across visual rows. Without wrap="overflow" twinki
          // would re-wrap at termCols and bake \n into the output,
          // breaking syntax highlighting + URL preservation + triple-
          // click selection. Static scrollback rendering of diffs already
          // uses wrap="overflow" via LiteLayout's <Static>.
          <Text key={`diff-${i}`} wrap="overflow">
            {line}
          </Text>
        ))}
      {!writeDiffLines &&
        toolArgsLines &&
        toolArgsLines.map((line, i) => <Text key={`arg-${i}`}>{line}</Text>)}
      {!writeDiffLines && !toolArgsLines && detailStr && (
        <Text> {detailStr}</Text>
      )}
      {!writeDiffLines &&
        !toolArgsLines &&
        Array.isArray(detail) &&
        detail.length > 1 &&
        detail.slice(1).map((line, i) => <Text key={i}> {line}</Text>)}
      {reasoning && <Text> </Text>}
      {reasoning && (
        <Text>
          {' '}
          {chalk.dim('reasoning:')} {chalk.hex('#C19AFF')(reasoning)}
        </Text>
      )}
      <Text> </Text>
      {page === 'notes' ? (
        // Notes page: the y/t/n hotkey row is replaced by a feedback input.
        // Submitting STAGES the text and returns to the picker — it does not
        // resolve or cancel the approval. The staged note is later flushed to
        // the model as a follow-up turn once the user picks y/t/n (see
        // respondWithNote). The normal lite input row stays suppressed
        // (LiteLayout gates it on !showApproval), so this is the only mounted
        // input while approval shows. PromptInput is keyed on the staged note
        // so re-entering the page remounts it seeded with the prior text via
        // the store's commandInputValue sync.
        <>
          <PromptInput
            onSubmit={handleStageNote}
            isProcessing={false}
            placeholder="add your feedback, then pick y/t/n..."
          />
          <Text>{chalk.dim('[enter] save note · [esc] back')}</Text>
        </>
      ) : (
        <>
          {stagedNote.trim() && (
            <Text>
              {chalk.dim('note attached:')}{' '}
              {chalk.hex('#C19AFF')(truncateLine(stagedNote.trim(), 60))}
            </Text>
          )}
          <Text>
            {chalk.green('[y]')} allow once {chalk.dim('·')}{' '}
            {chalk.yellow('[t]')}{' '}
            {hasTrustTiers ? 'trust scope' : chalk.bold('TRUST whole tool')}{' '}
            {chalk.dim('·')} {chalk.red('[n]')} deny {chalk.dim('·')}{' '}
            {chalk.cyan('[tab]')} {stagedNote.trim() ? 'edit note' : 'add note'}{' '}
            {chalk.dim('·')} {chalk.red('[esc]')} interrupt
          </Text>
        </>
      )}
    </Box>
  );
}

function extractApprovalDetail(
  toolName: string,
  content: string
): string | string[] | null {
  if (!content) return null;
  try {
    const args = JSON.parse(content);

    // bash/execute_bash/shell: show full command
    if (
      toolName === 'bash' ||
      toolName === 'execute_bash' ||
      toolName === 'shell' ||
      toolName === 'run_command'
    ) {
      if (args.command && typeof args.command === 'string') {
        return chalk.white(`command: ${args.command}`);
      }
    }

    // delete_file: show path
    if (
      toolName === 'delete_file' ||
      toolName === 'delete' ||
      toolName === 'fs_delete' ||
      toolName === 'remove_file'
    ) {
      const path = args.path || args.file_path || args.filePath;
      if (path) return chalk.white(`delete: ${path}`);
    }

    // fs_write / str_replace / write: show path + diff. Wire format is
    // snake_case (see renderWriteToolCall); accept camelCase as fallback.
    if (args.path && typeof args.path === 'string') {
      const oldStr = args.old_str ?? args.oldStr;
      const newStr = args.new_str ?? args.newStr;
      const fileText = args.file_text ?? args.content;
      const insertLine = args.insert_line ?? args.insertLine;
      // str_replace: always show diff if old_str present, regardless of
      // command field.
      if (oldStr && newStr != null) {
        const oldLines = (oldStr as string).split('\n');
        const newLines = (newStr as string).split('\n');
        const result: string[] = [chalk.white(`edit: ${args.path}`)];
        for (const line of oldLines.slice(0, 3)) {
          result.push(chalk.red(`  - ${truncateLine(line, 70)}`));
        }
        if (oldLines.length > 3)
          result.push(chalk.red(`  - ... (${oldLines.length} lines)`));
        for (const line of newLines.slice(0, 3)) {
          result.push(chalk.green(`  + ${truncateLine(line, 70)}`));
        }
        if (newLines.length > 3)
          result.push(chalk.green(`  + ... (${newLines.length} lines)`));
        return result;
      }
      if (args.command === 'delete') {
        return chalk.white(`delete: ${args.path}`);
      }
      if (args.command === 'insert' || insertLine != null) {
        const text = newStr ?? fileText ?? '';
        const lineCount =
          typeof text === 'string' ? text.split('\n').length : 0;
        return chalk.white(`insert: ${args.path} +${lineCount} lines`);
      }
      if (args.command === 'append') {
        const text = newStr ?? fileText ?? '';
        const lineCount =
          typeof text === 'string' ? text.split('\n').length : 0;
        return chalk.white(`append: ${args.path} +${lineCount} lines`);
      }
      // create (default for fs_write with file_text/content but no old_str)
      if (fileText != null || newStr != null) {
        const text = fileText ?? newStr ?? '';
        const lineCount =
          typeof text === 'string' ? text.split('\n').length : 0;
        return chalk.white(`create: ${args.path} (${lineCount} lines)`);
      }
      return chalk.white(args.path);
    }

    if (args.url) return chalk.white(args.url);
    if (args.__tool_use_purpose) return chalk.white(args.__tool_use_purpose);
    if (args.query) return chalk.white(`query: ${args.query}`);
    for (const [key, val] of Object.entries(args)) {
      if (key.startsWith('_')) continue;
      if (typeof val === 'string' && val.length > 0) {
        return chalk.white(`${key}: ${val}`);
      }
    }
  } catch {
    // Non-JSON content or unparsable args — fall through to null.
  }
  return null;
}

function truncateLine(s: string, max: number): string {
  return s.length <= max ? s : s.slice(0, max - 1) + '…';
}
