import React from 'react';
import { Box, EditorInput, Text, type ComponentMouseEvent } from 'twinki';
import { FileViewer } from '../32-acp-showcase/components/FileViewer.js';
import { Transcript } from '../32-acp-showcase/components/Transcript.js';
import { initialSessionState } from '../32-acp-showcase/session-state.js';
import type { ShowcaseTheme } from '../32-acp-showcase/themes.js';
import { CanvasTabs, type CanvasTabItem } from './CanvasTabs.js';
import { DiffViewer } from './DiffViewer.js';
import type { ManagedSession } from './sessions.js';
import { sessionIdFromTab, titleFromId, type CanvasGroupState, type WorkbenchDocument } from './workbench-state.js';

export function CanvasPane({
  width,
  height,
  active,
  overlayOpen,
  group,
  sessions,
  documents,
  sessionScroll,
  editorScroll,
  theme,
  onActivatePane,
  onActivateTab,
  onCloseTab,
  onContext,
  onSessionScroll,
  onDraftChange,
  onSubmit,
  onEditorContext,
  onEditorScroll,
}: {
  width: number;
  height: number;
  active: boolean;
  overlayOpen: boolean;
  group: CanvasGroupState;
  sessions: Readonly<Record<string, ManagedSession>>;
  documents: Readonly<Record<string, WorkbenchDocument>>;
  sessionScroll: number;
  editorScroll: number;
  theme: ShowcaseTheme;
  onActivatePane: (sessionId?: string) => void;
  onActivateTab: (tabId: string) => void;
  onCloseTab: (tabId: string) => void;
  onContext: (tabId: string, event: ComponentMouseEvent) => void;
  onSessionScroll: (scroll: number) => void;
  onDraftChange: (sessionId: string, draft: string) => void;
  onSubmit: (sessionId: string, value: string) => void;
  onEditorContext: (document: WorkbenchDocument, event: ComponentMouseEvent) => void;
  onEditorScroll: (scroll: number) => void;
}): React.ReactElement {
  const tabId = group.activeId;
  const sessionId = sessionIdFromTab(tabId);
  const item = sessionId ? sessions[sessionId] : undefined;
  const document = documents[tabId];
  const tabs: CanvasTabItem[] = group.tabs.map((candidate) => {
    const candidateSessionId = sessionIdFromTab(candidate);
    if (candidateSessionId) {
      const session = sessions[candidateSessionId];
      const connection = session?.state.connection ?? 'connecting';
      return {
        id: candidate,
        title: session?.title ?? titleFromId(candidateSessionId),
        kind: 'session',
        statusColor:
          connection === 'error'
            ? theme.danger
            : connection === 'running' || connection === 'cancelling'
              ? theme.warning
              : theme.success,
      };
    }
    const candidateDocument = documents[candidate];
    return {
      id: candidate,
      title: candidateDocument?.name ?? candidate.replace(/^(?:file|diff):/, ''),
      kind: candidate.startsWith('diff:') ? 'diff' : 'file',
    };
  });

  const state = item?.state ?? initialSessionState;
  const connectionColor =
    state.connection === 'error'
      ? theme.danger
      : state.connection === 'running' || state.connection === 'cancelling'
        ? theme.warning
        : theme.success;
  const contentHeight = Math.max(1, height - 1);

  return (
    <Box
      width={width}
      height={height}
      flexDirection="column"
      backgroundColor={theme.bg}
      onClick={() => onActivatePane(sessionId)}
    >
      <CanvasTabs
        tabs={tabs}
        activeId={tabId}
        width={width}
        theme={theme}
        onActivate={onActivateTab}
        onClose={onCloseTab}
        onContext={(tab, event) => onContext(tab.id, event)}
      />
      {sessionId ? (
        <>
          <Box height={1} paddingX={1} justifyContent="space-between" backgroundColor={theme.bg}>
            <Text color={theme.accent} bold>
              KIRO
            </Text>
            <Text color={connectionColor} bold>
              {item?.queued.length
                ? `${state.connection.toUpperCase()} · Q${item.queued.length}`
                : state.connection.toUpperCase()}
            </Text>
          </Box>
          <Transcript
            blocks={state.blocks}
            active={state.connection === 'running'}
            scrollFromBottom={sessionScroll}
            width={width}
            height={Math.max(1, height - 5)}
            theme={theme}
            onScrollFromBottom={onSessionScroll}
          />
          <Box width={width} height={3} paddingX={1} backgroundColor={theme.panel}>
            <EditorInput
              value={item?.draft ?? ''}
              onChange={(draft) => onDraftChange(sessionId, draft)}
              onSubmit={(value) => onSubmit(sessionId, value)}
              isActive={active && Boolean(item) && !overlayOpen}
              visibleLines={2}
              width={Math.max(1, width - 2)}
              color={theme.fg}
              backgroundColor={theme.panel}
              placeholder={`Ask ${item?.title ?? 'Kiro'}...`}
              mouseCursor
            />
          </Box>
        </>
      ) : document ? (
        <Box width={width} height={contentHeight} onMouseDown={(event) => onEditorContext(document, event)}>
          {document.before !== undefined && document.after !== undefined ? (
            <DiffViewer
              path={document.sourcePath}
              before={document.before}
              after={document.after}
              language={document.language}
              scrollTop={editorScroll}
              width={width}
              height={contentHeight}
              theme={theme}
              showHeader={false}
              onScroll={onEditorScroll}
            />
          ) : (
            <FileViewer
              file={document}
              scrollTop={editorScroll}
              width={width}
              height={contentHeight}
              theme={theme}
              showHeader={false}
              onScroll={onEditorScroll}
            />
          )}
        </Box>
      ) : (
        <Box width={width} height={contentHeight} padding={1}>
          <Text color={theme.muted}>Open a file, diff, or session from the navigator.</Text>
        </Box>
      )}
    </Box>
  );
}
