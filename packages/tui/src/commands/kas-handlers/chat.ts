import { extractRpcErrorMessage } from '../../utils/error-handling';
import { truncateToRecentTurns } from '../../utils/replay-history';
import { logger } from '../../utils/logger';
import {
  exportSession as runExportSession,
  importSession as runImportSession,
} from '../../utils/session-archive-cli';
import { listAllSessions } from '../../utils/list-all-sessions-cli';
import { ensureSession } from '../../utils/ensure-session-cli';
import {
  isActiveEngineSource,
  isResumableSource,
} from '../../utils/cross-engine-session-id';
import { formatRelativeTime } from '../../utils/sessions';
import { formatSessionState } from '../../utils/session-picker';
import { isCloudExecutionTargetKind } from '../../types/multi-session';
import { Feature, features } from '../../features';
import { sanitizeSessionTitleForDisplay } from '../../utils/sanitize-title';
import { unquote } from '../../utils/string';
import type { SessionPickerRow } from '../../components/ui/SessionPickerPanel';
import { basename } from 'node:path';
import { statSync } from 'node:fs';
import type { AgentStreamEvent } from '../../types/agent-events';
import { cancelCloudClearRewipes, scheduleCloudClearRewipes } from '../effects';
import type { CommandContext } from '../types';
import type { KasCommand } from '../../kas-commands';
import type { DispatchOptions } from '../dispatcher';

export async function handleChat(
  cmd: KasCommand,
  args: string,
  ctx: CommandContext,
  options?: DispatchOptions
): Promise<void> {
  const trimmed = args.trim();

  if (!trimmed) {
    return showSessionPicker(ctx, cmd);
  }
  if (trimmed === 'save' || trimmed.startsWith('save ')) {
    // Cloud sessions: save/load shell out to the LOCAL session store, which
    // has no record of a relayed session — refuse up front instead of
    // failing oddly. Strictly cloud-gated (dark-ship): local sessions never
    // reach this branch.
    if (ctx.cloudSessionActive) {
      // cmd.name so the alias reports itself (/sessions save vs /chat save).
      ctx.showAlert(
        `${cmd.name} save is not available for a cloud session yet.`,
        'error',
        5000
      );
      return;
    }
    const rest = trimmed === 'save' ? '' : trimmed.slice(5).trim();
    return handleChatSave(ctx, rest);
  }
  if (trimmed === 'load' || trimmed.startsWith('load ')) {
    // Same cloud gate as `save` above: import targets the local store.
    if (ctx.cloudSessionActive) {
      ctx.showAlert(
        `${cmd.name} load is not available for a cloud session yet.`,
        'error',
        5000
      );
      return;
    }
    const rest = trimmed === 'load' ? '' : trimmed.slice(5).trim();
    return handleChatLoad(ctx, rest);
  }
  if (trimmed === 'new' || trimmed.startsWith('new ')) {
    const prompt = trimmed === 'new' ? null : trimmed.slice(4).trim() || null;
    return startNewSession(ctx, prompt);
  }
  if (options?.argIsSynthetic) {
    return loadExistingSession(ctx, trimmed);
  }
  ctx.showAlert(`Unknown ${cmd.name} subcommand: ${trimmed}`, 'error', 3000);
}

async function showSessionPicker(
  ctx: CommandContext,
  cmd: KasCommand
): Promise<void> {
  ctx.setLoadingMessage('Loading chat options...');
  const listing = await listAllSessions();
  ctx.setLoadingMessage(null);
  if (!listing.ok) {
    ctx.showAlert(`Failed to list sessions: ${listing.error}`, 'error', 3000);
    return;
  }
  const currentSessionId = ctx.kiro.sessionId;
  // Active engine is always KAS here: this handler runs only when the
  // dispatcher's KAS intercept fires.
  const activeIsKas = true;

  // Dark-ship: without the remote-sandbox feature, /chat keeps the exact
  // pre-existing selection-menu flow (no live-client overlay call, no
  // columnar panel) — released users see zero change.
  if (!features.isEnabled(Feature.RemoteSandbox)) {
    const options = listing.sessions
      .filter((s) => s.sessionId !== currentSessionId)
      .filter((s) => isResumableSource(s.source, activeIsKas))
      .map((s) => {
        const native = isActiveEngineSource(s.source, activeIsKas);
        const sourceTag = native ? '' : ` (${s.source})`;
        return {
          value: s.sessionId,
          label: `${sanitizeSessionTitleForDisplay(s.title)} (${s.sessionId.slice(0, 8)})${sourceTag}`,
          description: formatRelativeTime(s.updatedAt),
        };
      });
    if (options.length === 0) {
      ctx.showAlert('No previous sessions found', 'error', 3000);
      return;
    }
    ctx.setActiveCommand({ command: cmd, options });
    return;
  }
  // The shell-out (`chat --list-sessions`) merges V1/V2 stores but its one-shot
  // KAS child isn't wired to the remote endpoint, so it never returns cloud
  // rows. The live, already-connected client IS wired (it spans both stores),
  // so ask it too and overlay its rows — this is what surfaces cloud sessions
  // in the interactive picker. Best-effort: a failure just leaves the V1/V2
  // list intact.
  const liveByCwd = await Promise.resolve()
    .then(() => ctx.kiro.listSessions(process.cwd()))
    .catch(() => ({ sessions: [] }));
  const liveMeta = new Map(liveByCwd.sessions.map((s) => [s.sessionId, s]));
  const resumable = listing.sessions
    .filter((s) => s.sessionId !== currentSessionId)
    .filter((s) => isResumableSource(s.source, activeIsKas));
  // Cloud rows the live client knows about but the shell-out omitted entirely.
  const shellIds = new Set(listing.sessions.map((s) => s.sessionId));
  const liveOnly = liveByCwd.sessions
    .filter(
      (s) =>
        s.sessionId !== currentSessionId &&
        !shellIds.has(s.sessionId) &&
        isCloudExecutionTargetKind(s.executionTarget?.kind)
    )
    .map((s) => ({
      sessionId: s.sessionId,
      source: 'v3' as const,
      title: s.title ?? '',
      updatedAt: s.updatedAt ?? '',
      // The shell-out rows carry executionTarget as a STRING kind; normalize the
      // live client's `{ kind }` object to that so the row map below is uniform.
      executionTarget: s.executionTarget?.kind,
      status: s.status,
    }));
  if (resumable.length === 0 && liveOnly.length === 0) {
    ctx.showAlert('No previous sessions found', 'error', 3000);
    return;
  }
  // Columnar resume table: ID | Name | Environment | Status | Last
  // updated. A local (on-disk, not-running) session has no live status, so it
  // defaults to `idle`. Overlay the live client's per-row cloud metadata onto
  // the shell-out rows, then append the cloud-only rows it alone knows, and
  // sort the whole set most-recent-first so cloud rows aren't buried below the
  // window when many local sessions exist. Both sources emit UTC ISO-8601 (Z)
  // timestamps, so lexicographic order equals chronological order here.
  const merged = [...resumable, ...liveOnly].sort((a, b) =>
    (b.updatedAt ?? '').localeCompare(a.updatedAt ?? '')
  );
  const rows: SessionPickerRow[] = merged.map((s) => {
    const live = liveMeta.get(s.sessionId);
    const executionTarget = live?.executionTarget?.kind ?? s.executionTarget;
    const status = live?.status ?? s.status;
    const isCloud = isCloudExecutionTargetKind(executionTarget);
    return {
      sessionId: s.sessionId,
      title: sanitizeSessionTitleForDisplay(s.title),
      environment: isCloud ? 'cloud' : 'local',
      status: status ? formatSessionState(status) : 'idle',
      updatedAt: s.updatedAt,
    };
  });
  // Echo the command the user actually typed (`/chat` or `/sessions`) as the
  // panel title even though both share this view.
  ctx.setShowSessionPicker(true, rows, cmd.name);
}

async function startNewSession(
  ctx: CommandContext,
  prompt: string | null
): Promise<void> {
  // A pending /clear re-wipe belongs to the outgoing session; firing here
  // would repaint the incoming one mid-stream.
  cancelCloudClearRewipes();
  ctx.clearUIState();
  // The bound repo/branch describe the PREVIOUS session's sandbox — stash so
  // switching back to it restores its footer, then clear for the new session.
  // Keep the id: a rejected RPC leaves that session active, so its scope must
  // come back out of the stash.
  const previousSessionId = ctx.kiro.sessionId;
  ctx.stashCloudSessionScope(previousSessionId);
  ctx.resetCloudSessionScope();
  ctx.resetMessages();
  // Armed before the (potentially slow) create so the previous session's rows
  // wipe immediately; re-wipes on each cloud repaint until startup quiets.
  scheduleCloudClearRewipes(ctx);
  // The loader shows alone during the create; the checklist arms on success.
  ctx.setCloudNewSessionChecklist(false);
  ctx.setLoadingMessage('Starting new conversation...');
  const restoreKasSession = ctx.beginKasSession('new');
  try {
    const session = await ctx.kiro.newSession();
    ctx.setLoadingMessage(null);
    ctx.setSessionId(session.sessionId);
    // The create response reports the repos the BFF actually bound
    // (`_meta.kiro.repositories`); the reset above darkened the footer, so
    // re-light it from that authoritative outcome. Null (nothing reported)
    // leaves the footer dark, matching a fresh unbound session.
    const boundRepos = ctx.kiro.getSessionRepositories();
    if (ctx.kiro.isCloudSessionActive() && boundRepos) {
      ctx.applyRepoFooter(
        boundRepos.map((r) => r.name),
        boundRepos[0]?.branch ?? null
      );
    }
    // Cloud-only creation feedback, dismissed by the first message.
    if (ctx.kiro.isCloudSessionActive()) {
      ctx.setCloudNewSessionChecklist(true);
    }
    if (session.currentModel) ctx.setCurrentModel(session.currentModel);
    if (session.currentAgent) ctx.setCurrentAgent(session.currentAgent);
    ctx.showAlert(
      'New conversation started. Use /chat to switch back.',
      'success',
      3000
    );
    if (prompt) ctx.sendMessage(prompt);
  } catch (err) {
    restoreKasSession();
    // A reconcile left armed would keep re-wiping the restored session.
    cancelCloudClearRewipes();
    // The previous session is still the active one — bring its footer
    // repo/branch back from the stash cleared above.
    ctx.restoreCloudSessionScope(previousSessionId);
    logger.error('[chat] newSession failed', {
      err: JSON.stringify(err),
      stack: err instanceof Error ? err.stack : undefined,
    });
    ctx.setLoadingMessage(null);
    ctx.showAlert(
      extractRpcErrorMessage(err, 'Failed to start new conversation'),
      'error',
      5000
    );
  }
}

export async function loadExistingSession(
  ctx: CommandContext,
  inputId: string,
  options?: { systemMessage?: string; source?: 'local' | 'remote' }
): Promise<void> {
  // A remote session has no local on-disk record, so skip ensure-session's
  // local probes and load the id straight through the connected client, which
  // routes `session/load` to the remote store.
  let sessionId: string;
  if (options?.source === 'remote') {
    sessionId = inputId;
  } else {
    ctx.setLoadingMessage(`Resolving session ${inputId}...`);
    const ensured = await ensureSession({
      sourceFormat: 'auto',
      sourceSessionId: inputId,
      targetFormat: 'kas',
      cwd: process.cwd(),
    });
    ctx.setLoadingMessage(null);
    if (!ensured.ok) {
      // Local stores don't have it — if this is a cloud session, treat the id as
      // a remote one and let the connected client load it. Otherwise surface the
      // original not-found.
      if (ctx.cloudSessionActive) {
        sessionId = inputId;
      } else {
        ctx.showAlert(
          `Failed to load session: ${ensured.message}`,
          'error',
          5000
        );
        return;
      }
    } else {
      sessionId = ensured.sessionId;
    }
  }
  // Pending re-wipes and the post-create checklist describe the outgoing
  // session; neither may fire under the incoming one.
  cancelCloudClearRewipes();
  ctx.setCloudNewSessionChecklist(false);
  ctx.clearUIState();
  // Deliberately no resetMessages: switches APPEND each load's replay,
  // matching local-session behavior.
  // The bound repo/branch describe the PREVIOUS session's sandbox — snapshot
  // them under that session's id (so switching back restores its footer
  // without a re-fetch), then clear so nothing leaks into the loaded session.
  // Keep the id: a rejected load leaves that session active, so its scope
  // must come back out of the stash.
  const previousSessionId = ctx.kiro.sessionId;
  ctx.stashCloudSessionScope(previousSessionId);
  ctx.resetCloudSessionScope();
  ctx.setLoadingMessage(`Loading session ${sessionId}...`);
  // Buffer history events during load via direct onUpdate subscriber, then
  // replay them after the load resolves so the conversation renders in order.
  const buffered: AgentStreamEvent[] = [];
  const restoreKasSession = ctx.beginKasSession('resumed');
  try {
    const session = await ctx.kiro.loadSession(
      sessionId,
      (e) => buffered.push(e),
      options?.source ? { source: options.source } : undefined
    );
    logger.debug('[chat] loadSession resolved', {
      sessionId,
      bufferedCount: buffered.length,
    });
    // Cloud resume reads as "Connected to session"; a local
    // resume keeps the "Loaded session" wording. An explicit systemMessage
    // (e.g. an import) overrides both. Read the client's live mode — the
    // store snapshot still describes the pre-load session here.
    ctx.addSystemMessage(
      options?.systemMessage ??
        (ctx.kiro.isCloudSessionActive()
          ? `Connected to session ${sessionId}`
          : `Loaded session ${sessionId}`),
      true
    );
    if (buffered.length > 0) {
      const MAX_DISPLAY_TURNS = 10;
      const { events, omittedTurns } = truncateToRecentTurns(
        buffered,
        MAX_DISPLAY_TURNS
      );
      if (omittedTurns > 0) {
        ctx.addSystemMessage(
          `⋯ ${omittedTurns} earlier turn${omittedTurns === 1 ? '' : 's'} not shown`,
          true
        );
      }
      // cloudReplay from the client's live placement — the store's flag still
      // describes the outgoing session here.
      if (!ctx.kiro.replayHistory?.(events)) {
        const handler = ctx.createStreamEventHandler({
          fromHistory: true,
          cloudReplay: ctx.kiro.isCloudSessionActive(),
        });
        for (const e of events) handler(e);
        handler.flush();
        handler.dispose();
      }
    }
    ctx.setLoadingMessage(null);
    ctx.setSessionId(sessionId);
    // Mode follows the session: a local session loaded from a cloud surface
    // (or vice versa) flips the footer + cloud-only command gating globally.
    ctx.setCloudSessionActive(ctx.kiro.isCloudSessionActive());
    // Hydrate this session's footer repo/branch: prefer the load response's
    // own bound repos (`_meta.kiro.repositories` — authoritative when
    // reported, even as an explicit zero-repo set), falling back to a prior
    // switch-away stash only when KAS reported nothing.
    const boundRepos = ctx.kiro.getSessionRepositories();
    if (ctx.kiro.isCloudSessionActive() && boundRepos) {
      ctx.applyRepoFooter(
        boundRepos.map((r) => r.name),
        boundRepos[0]?.branch ?? null
      );
    } else {
      ctx.restoreCloudSessionScope(sessionId);
    }
    if (session.currentModel) ctx.setCurrentModel(session.currentModel);
    if (session.currentAgent)
      ctx.setCurrentAgent(session.currentAgent, { suppressWelcome: true });
    ctx.showAlert('Session loaded', 'success', 3000);
  } catch (err) {
    restoreKasSession();
    // The previous session is still the active one — bring its footer
    // repo/branch back from the stash cleared above.
    ctx.restoreCloudSessionScope(previousSessionId);
    logger.error('[chat] loadSession failed', {
      sessionId,
      err: JSON.stringify(err),
      stack: err instanceof Error ? err.stack : undefined,
    });
    ctx.setLoadingMessage(null);
    ctx.showAlert(
      extractRpcErrorMessage(err, 'Failed to load session'),
      'error',
      5000
    );
  }
}

/**
 * `/chat save [--force] <path>` - shell out to `kiro-cli chat _ export-session`
 * to write the current KAS session to a portable zip archive.
 */
async function handleChatSave(
  ctx: CommandContext,
  rest: string
): Promise<void> {
  const tokens = rest.split(/\s+/).filter((t) => t.length > 0);
  let force = false;
  const positionals: string[] = [];
  for (const t of tokens) {
    if (t === '--force') force = true;
    else positionals.push(t);
  }
  // Re-join then strip surrounding quotes: whitespace-splitting above breaks
  // a quoted path with spaces into several tokens; joining and unquoting
  // reconstructs the single intended path.
  const out = unquote(positionals.join(' ').trim());
  if (out.length === 0) {
    ctx.showAlert('Usage: /chat save [--force] <path>', 'error', 5000);
    return;
  }
  const sessionId = ctx.kiro.sessionId;
  if (!sessionId) {
    ctx.showAlert('No active session to save', 'error', 5000);
    return;
  }
  const result = runExportSession({
    sessionId,
    cwd: process.cwd(),
    out,
    force,
  });
  if (result.ok) {
    ctx.showAlert(`Saved session to ${result.path}`, 'success', 5000);
    return;
  }
  logger.error('[chat] export-session failed', { error: result.message });
  ctx.showAlert(result.message, 'error', 5000);
}

/**
 * `/chat load <path>` - shell out to `kiro-cli chat _ import-session`
 * to extract a portable zip archive into the local sessions tree, then
 * load the imported session via the existing `loadExistingSession` flow.
 */
async function handleChatLoad(
  ctx: CommandContext,
  rest: string
): Promise<void> {
  const archivePath = unquote(rest.trim());
  if (!archivePath) {
    ctx.showAlert('Usage: /chat load <path>', 'error', 5000);
    return;
  }
  let stat: import('node:fs').Stats;
  try {
    stat = statSync(archivePath);
  } catch {
    ctx.showAlert(`No such file: ${archivePath}`, 'error', 5000);
    return;
  }
  if (!stat.isFile()) {
    ctx.showAlert(`Not a file: ${archivePath}`, 'error', 5000);
    return;
  }
  const result = runImportSession({ archivePath, cwd: process.cwd() });
  if (!result.ok) {
    logger.error('[chat] import-session failed', { error: result.message });
    ctx.showAlert(result.message, 'error', 5000);
    return;
  }
  const newSessionId = basename(result.path);
  if (!newSessionId) {
    ctx.showAlert(
      `Imported but couldn't derive session id from ${result.path}`,
      'error',
      5000
    );
    return;
  }
  await loadExistingSession(ctx, newSessionId, {
    systemMessage: `Loaded session from ${archivePath}`,
  });
}
