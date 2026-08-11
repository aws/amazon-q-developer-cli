/** Full-screen session browser with a list and optional wide preview. */
import React, {
  useCallback,
  useEffect,
  useMemo,
  useRef,
  useState,
} from 'react';
import { useFullscreen, Box } from '../../renderer.js';
import { Text } from '../ui/text/Text.js';
import { useTerminalSize } from '../../hooks/useTerminalSize.js';
import { useTheme } from '../../hooks/useThemeContext.js';
import { themeHex } from '../../utils/colorUtils.js';
import { useGlyphs } from '../../hooks/useGlyphs.js';
import { useAppStore } from '../../stores/app-store.js';
import { SessionDashboard } from '../ui/SessionDashboard.js';
import {
  getSessionPreviewProvider,
  type SessionPreview,
} from '../../utils/session-preview.js';
import {
  buildKasTurnTree,
  buildV2TurnList,
  type SessionTurn,
} from '../../utils/kas-session-turns.js';
import { findKasSessionDir, sessionsRoot } from '../../utils/session-store.js';
import { join } from 'node:path';
import { wasLaunchedIntoSessionDashboard } from '../../utils/session-dashboard-boot.js';
import { gracefulExit } from '../../utils/graceful-exit.js';
import { renderMessageToText, buildRenderTheme } from '../../lite/render.js';
import { sanitizeSessionTitleForDisplay } from '../../utils/sanitize-title.js';
import { truncateToWidth } from '../../utils/text-width.js';
import {
  sessionIdentityKey,
  sessionDashboardPaneWidths,
} from '../../utils/session-dashboard.js';
import {
  recordTuiSessionDashboard,
  forceFlushMetrics,
} from '../../utils/tui-telemetry-observer.js';
import { getSessionBookmarkStore } from '../../utils/session-bookmarks.js';
import {
  getCachedAllWorkspaceSessions,
  scanAllWorkspaceSessionsDetailed,
  invalidateAllWorkspaceSessionsCache,
  mergeSessionListings,
  fetchClassicSessionsDetailed,
  getCachedClassicSessions,
  listLiveDashboardSessionsDetailed,
} from '../../utils/all-workspace-sessions.js';
import { chalk } from '../../utils/color.js';
import { logger } from '../../utils/logger.js';

export const SessionDashboardScreen: React.FC = () => {
  useFullscreen();
  const { width, height } = useTerminalSize();
  const { getColor } = useTheme();
  const glyphs = useGlyphs();
  // Preview pane on/off (Ctrl+P). When off, the list takes the full width.
  // The table is the primary surface — preview is opt-in (Tab).
  const [showPreview, setShowPreview] = useState(false);
  // Wide-pane view: conversation (chat-rendered) or turn tree. Shift+Tab.
  const [previewView, setPreviewView] = useState<'conversation' | 'turns'>(
    'conversation'
  );
  const hexOf = (path: string, fallback: string): string =>
    themeHex(getColor, path, fallback);
  const secondaryHex = hexOf('secondary', '#808080');
  const brandHex = hexOf('brand', '#C19AFF');

  const setMode = useAppStore((s) => s.setMode);
  const sessionId = useAppStore((s) => s.sessionId);
  const agentEngine = useAppStore((s) => s.agentEngine);
  const cloudSessionActive = useAppStore((s) => s.cloudSessionActive);
  const kiro = useAppStore((s) => s.kiro);
  const resumeSession = useAppStore((s) => s.resumeSession);
  const sessions = useAppStore((s) => s.sessionDashboardSessions);
  const setShowSessionDashboard = useAppStore((s) => s.setShowSessionDashboard);
  const highlightedSessionIdentity = useAppStore(
    (s) => s.dashboardHighlightedSession
  );
  const dashboardEntry = useAppStore((s) => s.sessionDashboardEntry);

  const [refreshing, setRefreshing] = useState(true);
  const [catalogReady, setCatalogReady] = useState(false);
  const [catalogIncomplete, setCatalogIncomplete] = useState(false);
  const [previewRevision, setPreviewRevision] = useState(0);
  const refreshGeneration = useRef(0);
  const mounted = useRef(true);
  const refreshSessions = useCallback(
    async (resetCache = false) => {
      const generation = ++refreshGeneration.current;
      setRefreshing(true);
      setCatalogReady(false);
      setCatalogIncomplete(false);
      getSessionPreviewProvider().clearCache();
      setPreviewRevision((revision) => revision + 1);
      if (resetCache) invalidateAllWorkspaceSessionsCache();

      const [disk, live] = await Promise.all([
        scanAllWorkspaceSessionsDetailed(),
        listLiveDashboardSessionsDetailed(kiro, process.cwd()),
      ]);
      if (!mounted.current || generation !== refreshGeneration.current) return;

      setShowSessionDashboard(
        true,
        mergeSessionListings(live.sessions, [
          ...disk.sessions,
          ...getCachedClassicSessions(),
        ])
      );

      const classic = await fetchClassicSessionsDetailed();
      if (!mounted.current || generation !== refreshGeneration.current) return;

      const merged = mergeSessionListings(live.sessions, [
        ...disk.sessions,
        ...classic.sessions,
      ]);
      setShowSessionDashboard(true, merged);
      setRefreshing(false);
      setCatalogReady(true);
      setCatalogIncomplete(
        !disk.complete || !live.complete || !classic.complete
      );

      if (disk.complete && live.complete && classic.complete) {
        const pruned = getSessionBookmarkStore().prune(
          new Set(merged.map((s) => s.sessionId))
        );
        if (!pruned.ok) {
          logger.warn(
            '[dashboard] failed to prune session metadata:',
            pruned.reason
          );
        }
      }
    },
    [kiro, setShowSessionDashboard]
  );

  useEffect(() => {
    mounted.current = true;
    void refreshSessions();
    return () => {
      mounted.current = false;
    };
  }, [refreshSessions]);

  const listData =
    sessions.length > 0 ? sessions : getCachedAllWorkspaceSessions();

  const paneWidths = sessionDashboardPaneWidths(width, showPreview);
  const listW = paneWidths.list;
  const previewW = paneWidths.preview;
  const previewVisible = paneWidths.showPreview;

  const highlightedId = highlightedSessionIdentity?.sessionId ?? null;
  const highlightedIdentity = highlightedSessionIdentity
    ? sessionIdentityKey(highlightedSessionIdentity)
    : null;
  const highlightedSession = useMemo(() => {
    if (!highlightedIdentity) return null;
    return (
      listData.find(
        (session) => sessionIdentityKey(session) === highlightedIdentity
      ) ?? null
    );
  }, [highlightedIdentity, listData]);

  // Wide preview data for the highlighted session.
  const previewProvider = useMemo(() => getSessionPreviewProvider(), []);
  const [previewState, setPreviewState] = useState<{
    identity: string;
    preview: SessionPreview | null;
  } | null>(null);
  const [turnsState, setTurnsState] = useState<{
    identity: string;
    turns: SessionTurn[];
  } | null>(null);
  useEffect(() => {
    if (!highlightedId || !highlightedIdentity) return;
    const timer = setTimeout(() => {
      setPreviewState({
        identity: highlightedIdentity,
        preview: previewProvider.getPreview(
          highlightedId,
          highlightedSession?.engine,
          highlightedSession?.source
        ),
      });
      const dir =
        highlightedSession?.source !== 'remote' &&
        (highlightedSession?.engine === 'v3' ||
          highlightedSession?.engine === undefined)
          ? findKasSessionDir(sessionsRoot(), highlightedId)
          : null;
      setTurnsState({
        identity: highlightedIdentity,
        turns: dir
          ? buildKasTurnTree(dir)
          : highlightedSession?.engine === 'v2'
            ? buildV2TurnList(join(sessionsRoot(), 'cli'), highlightedId)
            : [],
      });
    }, 150);
    return () => clearTimeout(timer);
  }, [
    highlightedId,
    highlightedIdentity,
    highlightedSession?.engine,
    highlightedSession?.source,
    previewProvider,
    previewRevision,
  ]);
  const preview =
    previewState?.identity === highlightedIdentity
      ? previewState.preview
      : null;
  const turns =
    turnsState?.identity === highlightedIdentity ? turnsState.turns : null;

  const dim = (s: string) => chalk.hex(secondaryHex)(s);

  // Chat-view render theme (reuses the lite renderer so previews look like
  // the real conversation: Kiro:/You: tags, markdown, wrapping).
  const renderTheme = useMemo(() => buildRenderTheme(getColor), [getColor]);

  // Preview body by view: the turn tree (structured), or the recent messages
  // rendered THROUGH THE CHAT RENDERER so they read like a real chat.
  // When the highlighted session has children (subagent/rewind), both the
  // last turns and the child list are shown together.
  const childSessions = useMemo(() => {
    if (!highlightedSession) return [];
    return (
      listData as Array<{
        parentSessionId?: string;
        createdReason?: string;
        title?: string;
        sessionId: string;
        engine?: 'classic' | 'v2' | 'v3';
        source?: 'local' | 'remote';
      }>
    ).filter(
      (session) =>
        session.parentSessionId === highlightedSession.sessionId &&
        sessionIdentityKey({
          sessionId: session.parentSessionId,
          engine: session.engine,
          source: session.source,
        }) === sessionIdentityKey(highlightedSession)
    );
  }, [listData, highlightedSession]);

  const previewLines: string[] = useMemo(() => {
    const budget = height - 6; // header + rules + footer + border
    const w = Math.max(previewW - 4, 8);
    // Child sessions (subagent/rewind) trail both preview views.
    const pushChildSessions = (lines: string[]): void => {
      if (childSessions.length === 0 || lines.length >= budget) return;
      lines.push('');
      lines.push(
        chalk.hex(secondaryHex)(
          `${glyphs.lineHorizontal} ${childSessions.length} child session${childSessions.length === 1 ? '' : 's'} ${glyphs.lineHorizontal}`
        )
      );
      for (const child of childSessions) {
        if (lines.length >= budget) break;
        const reason =
          child.createdReason === 'rewind'
            ? `${glyphs.rewind} `
            : `${glyphs.gear} `;
        const title =
          sanitizeSessionTitleForDisplay(child.title ?? '') || '(no title)';
        lines.push(dim(truncateToWidth(`  ${reason}${title}`, w)));
      }
    };
    const lines: string[] = [];
    if (previewView === 'turns') {
      if (!turns || turns.length === 0) {
        return [dim('No turn data for this session.')];
      }
      for (const [ti, t] of turns.entries()) {
        if (lines.length >= budget) break;
        lines.push(
          chalk
            .hex(brandHex)
            .bold(
              truncateToWidth(
                `${glyphs.arrowRight} ${ti + 1}. ${t.userText || '(no prompt)'}`,
                w
              )
            )
        );
        if (t.assistantText && lines.length < budget) {
          lines.push(dim(truncateToWidth(`  ${t.assistantText}`, w)));
        }
        if (t.toolNames.length > 0 && lines.length < budget) {
          lines.push(
            getColor('success')(
              truncateToWidth(
                `  ${glyphs.dotFilled} main · ${t.toolNames.join(', ')}`,
                w
              )
            )
          );
        }
        for (const [si, sub] of t.subagents.entries()) {
          if (lines.length >= budget) break;
          const branch =
            si === t.subagents.length - 1
              ? glyphs.cornerBottomLeft
              : glyphs.teeRight;
          lines.push(
            dim(
              truncateToWidth(
                `    ${branch} sub · ${sub.summary || 'subagent'} (${sub.toolCallCount} tools)`,
                w
              )
            )
          );
        }
      }
      pushChildSessions(lines);
      return lines;
    }
    if (preview) {
      // Map preview messages to the renderer's MessageLike and render each
      // as it would appear in the conversation, then clip to the budget.
      for (const m of preview.recentMessages) {
        if (lines.length >= budget) break;
        const rendered = renderMessageToText(
          {
            id: m.timestamp ?? String(lines.length),
            role: m.role === 'user' ? 'user' : 'model',
            content: m.content,
          },
          undefined,
          { theme: renderTheme, termCols: w }
        );
        for (const l of rendered.split('\n')) {
          if (lines.length >= budget) break;
          lines.push(l);
        }
        lines.push(''); // blank line between messages
      }
      if (lines.length < budget) {
        lines.push(
          dim(
            preview.summary.isComplete
              ? `${preview.summary.turnCount} turns`
              : `${preview.summary.turnCount} recent turns`
          )
        );
      }
      pushChildSessions(lines);
      return lines;
    }
    return [dim('Highlight a session to preview it.')];
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [
    turns,
    preview,
    previewW,
    height,
    renderTheme,
    previewView,
    childSessions,
  ]);

  const previewTitle = highlightedSession
    ? sanitizeSessionTitleForDisplay(highlightedSession.title ?? '') ||
      highlightedSession.sessionId.slice(0, 12)
    : 'Preview';

  return (
    <Box flexDirection="column" width={width} height={height}>
      <Box flexDirection="row" flexGrow={1}>
        {/* Master list — the existing dashboard component, full height */}
        <Box flexDirection="column" width={listW} flexShrink={0}>
          <SessionDashboard
            sessions={listData}
            currentCwd={process.cwd()}
            activeSessionId={sessionId}
            activeSessionEngine={agentEngine === 'kas' ? 'v3' : 'v2'}
            activeSessionSource={cloudSessionActive ? 'remote' : 'local'}
            width={listW}
            hidePreview
            onTogglePreview={() => setShowPreview((v) => !v)}
            onCyclePreviewView={() => {
              // Cycling views implies wanting the pane visible.
              setShowPreview(true);
              setPreviewView((v) =>
                v === 'conversation' ? 'turns' : 'conversation'
              );
            }}
            isRefreshing={refreshing}
            backgroundReady={catalogReady}
            catalogIncomplete={catalogIncomplete}
            onSelect={(id, environment, targetCwd, meta) => {
              const selectedEngine = meta?.engine ?? 'v3';
              const telemetry = {
                outcome: targetCwd
                  ? ('resumed_cross_workspace' as const)
                  : ('resumed' as const),
                via: meta?.via,
                entry: dashboardEntry,
                engine:
                  selectedEngine === 'classic'
                    ? ('v1' as const)
                    : selectedEngine,
              };
              void resumeSession(id, environment, targetCwd, selectedEngine)
                .then((resumed) => {
                  if (!resumed) return;
                  recordTuiSessionDashboard(telemetry);
                  setShowSessionDashboard(false);
                  setMode('inline');
                })
                .catch((error) => {
                  logger.warn('[dashboard] failed to resume session:', error);
                });
            }}
            onRefresh={() => {
              void refreshSessions(true);
            }}
            onClose={() => {
              recordTuiSessionDashboard({
                outcome: 'closed',
                entry: dashboardEntry,
                engine: 'v3',
              });
              // `--sessions` boot: no session was ever created, so there is
              // no chat to return to — closing the dashboard ends the run.
              // The metrics exporter runs on a 60s cadence, so the event
              // recorded above must be flushed before the process exits.
              if (wasLaunchedIntoSessionDashboard() && !sessionId) {
                void forceFlushMetrics()
                  .catch(() => {})
                  .finally(() => {
                    process.stdout.write('\x1b[?1049l');
                    gracefulExit(0);
                  });
                return;
              }
              setShowSessionDashboard(false);
              setMode('inline');
            }}
          />
        </Box>

        {/* Divider (only when the preview pane is shown) */}
        {previewVisible && (
          <Box flexDirection="column" width={1} flexShrink={0}>
            {Array.from({ length: Math.max(height - 1, 1) }, (_, i) => (
              <Text key={i}>{dim(glyphs.lineVertical)}</Text>
            ))}
          </Box>
        )}

        {/* Wide preview pane — bordered box marks it as a read-only preview */}
        {previewVisible && (
          <Box
            flexDirection="column"
            width={previewW}
            borderStyle="round"
            borderColor={brandHex}
            borderTitle={truncateToWidth(
              `Preview (${previewView} · ${glyphs.shift} tab: ${
                previewView === 'turns' ? 'conversation' : 'turns'
              }): ${previewTitle} `,
              Math.max(previewW - 4, 8)
            )}
            paddingX={1}
          >
            {previewLines.map((line, i) => (
              <Text key={i}>{line}</Text>
            ))}
          </Box>
        )}
      </Box>
    </Box>
  );
};
