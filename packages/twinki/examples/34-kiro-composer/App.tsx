import React, { useCallback, useEffect, useMemo, useRef, useState } from 'react';
import { readFileSync, unwatchFile, watchFile } from 'node:fs';
import { basename, resolve } from 'node:path';
import { Box, Tabs, Text, useInput, useMouse, useSelectionCopy, type ComponentMouseEvent, type Tab } from 'twinki';
import type { ShowcaseClient } from '../32-acp-showcase/acp-client.js';
import { ContextMenu, PermissionPanel } from '../32-acp-showcase/components/OverlayPanels.js';
import { FileRail } from '../32-acp-showcase/components/FileRail.js';
import { initialSessionState } from '../32-acp-showcase/session-state.js';
import { nextTheme, type ShowcaseTheme } from '../32-acp-showcase/themes.js';
import type { FileEntry } from '../32-acp-showcase/types.js';
import { openWorkspaceFile, scanWorkspace, workspaceLabel } from '../32-acp-showcase/workspace.js';
import { CanvasPane } from './CanvasPane.js';
import { buildComposerPrompt } from './composer-prompt.js';
import { GitPanel } from './GitPanel.js';
import { HelpPanel } from './HelpPanel.js';
import { LayoutRenderer } from './LayoutRenderer.js';
import { LayoutPicker } from './LayoutPicker.js';
import { SessionPanel } from './SessionPanel.js';
import { SettingsPanel } from './SettingsPanel.js';
import { readGitComparison, readGitSnapshot, type GitFile, type GitSnapshot } from './git.js';
import {
  allWidgets,
  firstWidget,
  parseWorkbenchLayout,
  tabsForWidget,
  widgetBounds,
  type LayoutChoice,
  type WidgetNode,
} from './layout.js';
import { useManagedSessions } from './sessions.js';
import { clamp, copyToClipboard, inBounds, useTerminalSize } from './terminal.js';
import { ComposerHeader, ComposerStatusBar, RenameSessionDialog } from './ComposerChrome.js';
import {
  BUILTIN_WIDGET_IDS,
  createWidgetRegistry,
  defineWidget,
  type BuiltinWidget,
  type WidgetFrame,
} from './widgets.js';
import {
  addMissingCanvasGroups,
  canvasNodes as findCanvasNodes,
  fileTabId,
  initialCanvasGroups,
  sessionIdFromTab,
  sessionTabId,
  titleFromId,
  type CanvasGroupState,
  type WorkbenchDocument,
} from './workbench-state.js';
import { actionsForMenu, type MenuState } from './workbench-menu.js';

function canvasViewKey(groupId: string, tabId: string): string {
  return JSON.stringify([groupId, tabId]);
}

function editorScrollMetrics(
  document: WorkbenchDocument | null | undefined,
  width: number,
  height: number
): { viewport: number; maxScroll: number } {
  const isDiff = document?.before !== undefined && document.after !== undefined;
  const viewport = Math.max(1, height - 1);
  const beforeLines = document?.before?.split('\n').length ?? 0;
  const afterLines = document?.after?.split('\n').length ?? 0;
  const totalLines = isDiff
    ? width >= 96
      ? Math.max(beforeLines, afterLines) + 1
      : beforeLines + afterLines
    : (document?.content.split('\n').length ?? 0);
  return { viewport, maxScroll: Math.max(0, totalLines - viewport) };
}

export function App({
  client,
  createClient,
  workspaceRoot,
  initialFiles,
  initialTheme,
  engine,
  layouts,
  initialLayoutPath,
  loadLayouts,
  composerRoot,
  composerSkill,
}: {
  client: ShowcaseClient;
  createClient: () => ShowcaseClient;
  workspaceRoot: string;
  initialFiles: FileEntry[];
  initialTheme: ShowcaseTheme;
  engine: string;
  layouts: LayoutChoice[];
  initialLayoutPath: string;
  loadLayouts: () => LayoutChoice[];
  composerRoot: string;
  composerSkill: string;
}): React.ReactElement {
  const { columns, rows } = useTerminalSize();
  const initialLayout = layouts.find((layout) => layout.path === initialLayoutPath) ?? layouts[0]!;
  const initialEntry = useMemo(
    () => initialFiles.find((file) => /^readme\.md$/i.test(file.relativePath)) ?? initialFiles[0],
    [initialFiles]
  );
  const initialDocument = useMemo(
    () =>
      initialEntry
        ? {
            ...openWorkspaceFile(workspaceRoot, initialEntry),
            sourcePath: initialEntry.relativePath,
          }
        : null,
    [initialEntry, workspaceRoot]
  );
  const [documents, setDocuments] = useState<Record<string, WorkbenchDocument>>(() =>
    initialDocument ? { [fileTabId(initialDocument.path)]: initialDocument } : {}
  );
  const [layoutChoices, setLayoutChoices] = useState(layouts);
  const [activeLayoutPath, setActiveLayoutPath] = useState(initialLayout.path);
  const [layoutRatios, setLayoutRatios] = useState<Record<string, number>>({});
  const [layoutTabs, setLayoutTabs] = useState<Record<string, string>>({});
  const [layoutStatus, setLayoutStatus] = useState('LIVE');
  const [activeWidget, setActiveWidget] = useState(() => firstWidget(initialLayout.spec.layout, {}));
  const [pickerOpen, setPickerOpen] = useState(false);
  const [pickerSelected, setPickerSelected] = useState(() => Math.max(0, layouts.indexOf(initialLayout)));
  const [files, setFiles] = useState(initialFiles);
  const [theme, setTheme] = useState(initialTheme);
  const [yolo, setYolo] = useState(false);
  const [selectedPath, setSelectedPath] = useState(initialEntry?.path);
  const [selectedGitPath, setSelectedGitPath] = useState<string>();
  const [viewScrolls, setViewScrolls] = useState<Record<string, number>>({});
  const initialSessionSeeds = useMemo(
    () =>
      allWidgets(initialLayout.spec.layout)
        .filter((node) => node.widget === 'chat')
        .map((node) => ({ id: node.id, title: node.title ?? titleFromId(node.id) })),
    [initialLayout.spec.layout]
  );
  const initialSessionId = initialSessionSeeds[0]?.id ?? 'chat';
  const [selectedSessionId, setSelectedSessionId] = useState(initialSessionId);
  const [canvasGroups, setCanvasGroups] = useState<Record<string, CanvasGroupState>>(() =>
    initialCanvasGroups(initialLayout.spec.layout, initialDocument, initialSessionId)
  );
  const [renameSessionId, setRenameSessionId] = useState<string>();
  const [renameDraft, setRenameDraft] = useState('');
  const [git, setGit] = useState<GitSnapshot>({
    branch: '',
    files: [],
    loading: true,
  });
  const [menu, setMenu] = useState<MenuState | null>(null);
  const [menuSelected, setMenuSelected] = useState(0);
  const [help, setHelp] = useState(false);
  const [permissionSelected, setPermissionSelected] = useState(0);
  const [copied, setCopied] = useState(false);
  const copyTimer = useRef<ReturnType<typeof setTimeout> | undefined>(undefined);
  const gitSequence = useRef(0);
  const diffSequence = useRef(new Map<string, number>());
  const previousConnections = useRef<Record<string, string>>({});
  const choicesRef = useRef(layouts);
  const {
    sessions,
    ensureSession,
    createSession,
    renameSession,
    closeSession,
    updateSession,
    dispatchSession,
    clientFor,
  } = useManagedSessions(client, createClient, initialSessionSeeds);

  const activeLayout = layoutChoices.find((layout) => layout.path === activeLayoutPath) ?? layoutChoices[0]!;
  const compact = columns < 92;
  const bodyHeight = Math.max(5, rows - 2);
  const layoutWidgets = useMemo(() => allWidgets(activeLayout.spec.layout), [activeLayout.spec.layout]);
  const layoutWidgetIds = useMemo(() => layoutWidgets.map((node) => node.id), [layoutWidgets]);
  const canvasNodes = useMemo(() => findCanvasNodes(activeLayout.spec.layout), [activeLayout.spec.layout]);
  const activeWidgetNode = layoutWidgets.find((node) => node.id === activeWidget);
  const renderedWidget = layoutWidgetIds.includes(activeWidget)
    ? activeWidget
    : layoutWidgets.find((node) => node.widget === 'chat')?.id
      ? layoutWidgets.find((node) => node.widget === 'chat')!.id
      : layoutWidgetIds[0]!;
  const compactTabHeight = compact && layoutWidgetIds.length > 1 ? 1 : 0;
  const frames = useMemo(() => {
    if (compact) {
      return {
        [renderedWidget]: {
          x: 0,
          y: 1 + compactTabHeight,
          width: columns,
          height: Math.max(1, bodyHeight - compactTabHeight),
        },
      };
    }
    return widgetBounds(activeLayout.spec.layout, columns, bodyHeight, layoutRatios, layoutTabs, 0, 1);
  }, [
    activeLayout.spec.layout,
    bodyHeight,
    columns,
    compact,
    compactTabHeight,
    layoutRatios,
    layoutTabs,
    renderedWidget,
  ]);
  const renderedWidgetNode = layoutWidgets.find((node) => node.id === renderedWidget)!;
  const sessionList = Object.values(sessions);
  const selectedSession = sessions[selectedSessionId] ?? sessionList[0];
  const activeCanvasTab = activeWidgetNode ? canvasGroups[activeWidgetNode.id]?.activeId : undefined;
  const activeChatId = activeCanvasTab ? sessionIdFromTab(activeCanvasTab) : undefined;
  const activeChatSession = activeChatId ? sessions[activeChatId] : undefined;
  const controlSession = activeChatSession ?? selectedSession;
  const statusSession = controlSession?.state ?? initialSessionState;
  const permissionSession = sessionList.find((item) => item.state.permission);
  const permissionSessionId = permissionSession?.id;
  const permission = permissionSession?.state.permission ?? null;
  const activeDocument = activeCanvasTab ? (documents[activeCanvasTab] ?? null) : null;
  const activeEditorMetrics = editorScrollMetrics(
    activeDocument,
    frames[activeWidget]?.width ?? columns,
    frames[activeWidget]?.height ?? bodyHeight
  );
  const activeEditorScrollKey =
    activeDocument && activeCanvasTab && activeWidgetNode
      ? canvasViewKey(activeWidgetNode.id, activeCanvasTab)
      : undefined;
  const menuActions = actionsForMenu(menu);
  const overlayOpen = help || pickerOpen || Boolean(permission || menu || renameSessionId);

  const updateViewScroll = useCallback((groupId: string, tabId: string, update: (current: number) => number) => {
    if (!tabId) return;
    const key = canvasViewKey(groupId, tabId);
    setViewScrolls((current) => {
      const value = current[key] ?? 0;
      const next = update(value);
      return next === value ? current : { ...current, [key]: next };
    });
  }, []);

  useEffect(() => {
    choicesRef.current = layoutChoices;
  }, [layoutChoices]);
  useEffect(() => {
    for (const node of layoutWidgets) {
      if (node.widget === 'chat') ensureSession(node.id, node.title ?? titleFromId(node.id));
    }
  }, [ensureSession, layoutWidgets]);
  useEffect(() => {
    setCanvasGroups((current) => {
      return addMissingCanvasGroups(current, canvasNodes, initialDocument, selectedSessionId);
    });
  }, [canvasNodes, initialDocument, selectedSessionId]);
  useEffect(() => {
    if (!layoutWidgetIds.includes(activeWidget)) {
      setActiveWidget(renderedWidget);
    }
  }, [activeWidget, layoutWidgetIds, renderedWidget]);
  useEffect(() => {
    if (!activeEditorScrollKey) return;
    setViewScrolls((current) => {
      const value = current[activeEditorScrollKey] ?? 0;
      const next = Math.min(value, activeEditorMetrics.maxScroll);
      return next === value ? current : { ...current, [activeEditorScrollKey]: next };
    });
  }, [activeEditorMetrics.maxScroll, activeEditorScrollKey]);
  useEffect(() => {
    setPermissionSelected(0);
    if (permission) {
      setMenu(null);
      setHelp(false);
      setPickerOpen(false);
      setRenameSessionId(undefined);
      setRenameDraft('');
    }
  }, [permission]);
  useEffect(() => {
    if (!yolo || !permission || !permissionSessionId) return;
    const allow =
      permission.choices.find((choice) => /allow.once/i.test(choice.kind)) ??
      permission.choices.find((choice) => /allow|approve/i.test(`${choice.kind} ${choice.label}`));
    if (allow) clientFor(permissionSessionId)?.choosePermission(allow.id);
  }, [clientFor, permission, permissionSessionId, yolo]);
  useSelectionCopy((text) => {
    copyToClipboard(text);
    setCopied(true);
    clearTimeout(copyTimer.current);
    copyTimer.current = setTimeout(() => setCopied(false), 1_600);
  });
  useEffect(() => () => clearTimeout(copyTimer.current), []);

  useEffect(() => {
    let timer: ReturnType<typeof setTimeout> | undefined;
    const reload = (): void => {
      try {
        const spec = parseWorkbenchLayout(
          JSON.parse(readFileSync(activeLayoutPath, 'utf8')) as unknown,
          BUILTIN_WIDGET_IDS
        );
        setLayoutChoices((current) =>
          current.map((layout) => (layout.path === activeLayoutPath ? { ...layout, spec } : layout))
        );
        setLayoutStatus('RELOADED');
      } catch (error) {
        setLayoutStatus(`INVALID: ${error instanceof Error ? error.message : String(error)}`);
      }
    };
    const listener = (): void => {
      clearTimeout(timer);
      timer = setTimeout(reload, 80);
    };
    watchFile(activeLayoutPath, { interval: 180 }, listener);
    return () => {
      clearTimeout(timer);
      unwatchFile(activeLayoutPath, listener);
    };
  }, [activeLayoutPath]);

  const refreshLayouts = useCallback(() => {
    try {
      const next = loadLayouts();
      const known = new Set(choicesRef.current.map((layout) => layout.path));
      const added = next.find((layout) => !known.has(layout.path));
      setLayoutChoices(next);
      if (added) {
        setActiveLayoutPath(added.path);
        setLayoutTabs({});
        setLayoutRatios({});
        setActiveWidget(firstWidget(added.spec.layout, {}));
        setLayoutStatus('NEW LAYOUT');
      }
    } catch (error) {
      setLayoutStatus(`INVALID: ${error instanceof Error ? error.message : String(error)}`);
    }
  }, [loadLayouts]);

  const refreshGit = useCallback(() => {
    const sequence = ++gitSequence.current;
    setGit((current) => ({ ...current, loading: true }));
    void readGitSnapshot(workspaceRoot).then((snapshot) => {
      if (sequence === gitSequence.current) setGit(snapshot);
    });
  }, [workspaceRoot]);

  const refreshAll = useCallback(() => {
    setFiles(scanWorkspace(workspaceRoot));
    refreshGit();
    refreshLayouts();
  }, [refreshGit, refreshLayouts, workspaceRoot]);

  useEffect(refreshGit, [refreshGit]);
  useEffect(() => {
    let refresh = false;
    const next: Record<string, string> = {};
    for (const item of Object.values(sessions)) {
      const connection = item.state.connection;
      if (previousConnections.current[item.id] === 'running' && (connection === 'ready' || connection === 'error')) {
        refresh = true;
      }
      next[item.id] = connection;
    }
    previousConnections.current = next;
    if (refresh) refreshAll();
  }, [refreshAll, sessions]);

  const openLayout = useCallback(
    (index: number) => {
      const layout = layoutChoices[index];
      if (!layout) return;
      setPickerSelected(index);
      setActiveLayoutPath(layout.path);
      setLayoutTabs({});
      setLayoutRatios({});
      setActiveWidget(firstWidget(layout.spec.layout, {}));
      setLayoutStatus('LIVE');
      setPickerOpen(false);
      setMenu(null);
    },
    [layoutChoices]
  );

  const showLayoutPicker = useCallback(() => {
    setPickerSelected(
      Math.max(
        0,
        layoutChoices.findIndex((layout) => layout.path === activeLayout.path)
      )
    );
    setPickerOpen(true);
    setMenu(null);
  }, [activeLayout.path, layoutChoices]);

  const revealWidget = useCallback(
    (target: BuiltinWidget | string, preferredLayout?: string) => {
      const findTarget = (layout: LayoutChoice): WidgetNode | undefined => {
        const nodes = allWidgets(layout.spec.layout);
        return nodes.find((node) => node.id === target) ?? nodes.find((node) => node.widget === target);
      };
      const preferredChoice = preferredLayout
        ? layoutChoices.find(
            (layout) =>
              layout.spec.title.toLowerCase() === preferredLayout.toLowerCase() ||
              layout.source.toLowerCase() === preferredLayout.toLowerCase()
          )
        : undefined;
      const preferred = preferredChoice && findTarget(preferredChoice) ? preferredChoice : undefined;
      const layout =
        preferred ?? (findTarget(activeLayout) ? activeLayout : layoutChoices.find((choice) => findTarget(choice)));
      if (!layout) return;
      const node = findTarget(layout);
      if (!node) return;
      if (layout.path !== activeLayout.path) {
        setActiveLayoutPath(layout.path);
        setLayoutStatus('LIVE');
      }
      setLayoutTabs((current) =>
        tabsForWidget(layout.spec.layout, node.id, layout.path === activeLayout.path ? current : {})
      );
      setActiveWidget(node.id);
      if (node.widget === 'chat') setSelectedSessionId(node.id);
      setPickerOpen(false);
      return node.id;
    },
    [activeLayout, layoutChoices]
  );

  const openCanvasTab = useCallback(
    (tabId: string, placement: 'current' | 'left' | 'right' = 'current') => {
      const split = placement !== 'current';
      let layout = activeLayout;
      let slots = allWidgets(layout.spec.layout).filter((node) => node.widget === 'chat' || node.widget === 'editor');
      if ((split && slots.length < 2) || slots.length === 0) {
        const fallback = layoutChoices.find((choice) => {
          const count = allWidgets(choice.spec.layout).filter(
            (node) => node.widget === 'chat' || node.widget === 'editor'
          ).length;
          return split ? count >= 2 : count >= 1;
        });
        if (fallback) {
          layout = fallback;
          slots = allWidgets(layout.spec.layout).filter((node) => node.widget === 'chat' || node.widget === 'editor');
          setActiveLayoutPath(layout.path);
          setLayoutTabs({});
          setLayoutRatios({});
          setLayoutStatus('LIVE');
        }
      }
      if (slots.length === 0) return;
      const activeSlot = slots.find((node) => node.id === activeWidget);
      const slot =
        placement === 'left' ? slots[0]! : placement === 'right' ? (slots[1] ?? slots[0]!) : (activeSlot ?? slots[0]!);
      const switchedLayout = layout.path !== activeLayout.path;
      setCanvasGroups((current) => {
        const next = { ...current };
        if (switchedLayout && activeCanvasTab && slots.length > 1) {
          const other = placement === 'left' ? slots[1]! : slots[0]!;
          const otherGroup = next[other.id] ?? { tabs: [], activeId: '' };
          next[other.id] = {
            tabs: otherGroup.tabs.includes(activeCanvasTab) ? otherGroup.tabs : [...otherGroup.tabs, activeCanvasTab],
            activeId: activeCanvasTab,
          };
        }
        const existing = current[slot.id] ?? { tabs: [], activeId: '' };
        next[slot.id] = {
          tabs: existing.tabs.includes(tabId) ? existing.tabs : [...existing.tabs, tabId],
          activeId: tabId,
        };
        return next;
      });
      setLayoutTabs((current) =>
        tabsForWidget(layout.spec.layout, slot.id, layout.path === activeLayout.path ? current : {})
      );
      const sessionId = sessionIdFromTab(tabId);
      if (sessionId) setSelectedSessionId(sessionId);
      setActiveWidget(slot.id);
      setPickerOpen(false);
    },
    [activeCanvasTab, activeLayout, activeWidget, layoutChoices]
  );

  const openManagedSession = useCallback(
    (sessionId: string, split = false) => {
      openCanvasTab(sessionTabId(sessionId), split ? 'right' : 'current');
    },
    [openCanvasTab]
  );

  const activateCanvasTab = useCallback((groupId: string, tabId: string) => {
    setCanvasGroups((current) => {
      const group = current[groupId];
      if (!group || !group.tabs.includes(tabId)) return current;
      return { ...current, [groupId]: { ...group, activeId: tabId } };
    });
    const sessionId = sessionIdFromTab(tabId);
    if (sessionId) setSelectedSessionId(sessionId);
    setActiveWidget(groupId);
  }, []);

  const closeCanvasView = useCallback((groupId: string, tabId: string) => {
    setCanvasGroups((current) => {
      const group = current[groupId];
      if (!group) return current;
      const index = group.tabs.indexOf(tabId);
      const tabs = group.tabs.filter((candidate) => candidate !== tabId);
      const activeId =
        group.activeId === tabId ? (tabs[Math.min(index, Math.max(0, tabs.length - 1))] ?? '') : group.activeId;
      return { ...current, [groupId]: { tabs, activeId } };
    });
  }, []);

  const openCanvasContext = useCallback(
    (groupId: string, tabId: string, event: ComponentMouseEvent) => {
      if (event.button !== 'right') return;
      const sessionId = sessionIdFromTab(tabId);
      const label = sessionId ? (sessions[sessionId]?.title ?? sessionId) : (documents[tabId]?.sourcePath ?? tabId);
      setMenu({
        kind: 'canvas',
        groupId,
        tabId,
        point: { x: event.x, y: event.y, label },
      });
      setMenuSelected(0);
      setActiveWidget(groupId);
      if (sessionId) setSelectedSessionId(sessionId);
    },
    [documents, sessions]
  );

  const createManagedSession = useCallback(() => {
    const id = createSession();
    openManagedSession(id);
  }, [createSession, openManagedSession]);

  const beginRenameSession = useCallback(
    (id: string) => {
      setRenameSessionId(id);
      setRenameDraft(sessions[id]?.title ?? titleFromId(id));
      setMenu(null);
    },
    [sessions]
  );

  const closeManagedSession = useCallback(
    (id: string) => {
      const fallback = sessionList.find((session) => session.id !== id);
      closeSession(id);
      const tabId = sessionTabId(id);
      setCanvasGroups((current) => {
        const next: Record<string, CanvasGroupState> = {};
        for (const [slot, group] of Object.entries(current)) {
          const tabs = group.tabs.filter((candidate) => candidate !== tabId);
          const activeId =
            group.activeId === tabId ? (tabs[0] ?? (fallback ? sessionTabId(fallback.id) : '')) : group.activeId;
          next[slot] = { tabs, activeId };
          if (activeId && !next[slot]!.tabs.includes(activeId)) {
            next[slot]!.tabs.push(activeId);
          }
        }
        return next;
      });
      if (selectedSessionId === id) {
        if (fallback) {
          setSelectedSessionId(fallback.id);
          openManagedSession(fallback.id);
        } else {
          const replacement = createSession('Session 1');
          setSelectedSessionId(replacement);
          openManagedSession(replacement);
        }
      }
    },
    [closeSession, createSession, openManagedSession, selectedSessionId, sessionList]
  );

  const createWithKiro = useCallback(() => {
    const chatIndex = layoutChoices.findIndex((layout) => layout.spec.title.toLowerCase() === 'chat');
    const layout = chatIndex >= 0 ? layoutChoices[chatIndex]! : activeLayout;
    const chat = allWidgets(layout.spec.layout).find((node) => node.widget === 'chat');
    if (chatIndex >= 0) openLayout(chatIndex);
    else revealWidget('chat');
    if (chat) {
      ensureSession(chat.id, chat.title ?? titleFromId(chat.id));
      updateSession(chat.id, (item) => ({ ...item, draft: '/layout ' }));
      setSelectedSessionId(chat.id);
    }
  }, [activeLayout, ensureSession, layoutChoices, openLayout, revealWidget, updateSession]);

  const openFile = useCallback(
    (entry: FileEntry, split = false) => {
      const document: WorkbenchDocument = {
        ...openWorkspaceFile(workspaceRoot, entry),
        sourcePath: entry.relativePath,
      };
      const id = fileTabId(document.path);
      setSelectedPath(entry.path);
      setDocuments((current) => ({ ...current, [id]: document }));
      openCanvasTab(id, split ? 'right' : 'current');
    },
    [openCanvasTab, workspaceRoot]
  );

  const closeEditor = useCallback((id: string) => {
    diffSequence.current.delete(id);
    setCanvasGroups((current) => {
      const next: Record<string, CanvasGroupState> = {};
      for (const [slot, group] of Object.entries(current)) {
        const tabs = group.tabs.filter((tab) => tab !== id);
        next[slot] = {
          tabs,
          activeId: group.activeId === id ? (tabs[0] ?? '') : group.activeId,
        };
      }
      return next;
    });
    setDocuments((current) => {
      const next = { ...current };
      delete next[id];
      return next;
    });
  }, []);

  const openGitFile = useCallback(
    (file: GitFile, split = false) => {
      setSelectedGitPath(file.path);
      const workspaceEntry = files.find((entry) => entry.relativePath === file.path) ?? {
        path: resolve(workspaceRoot, file.path),
        relativePath: file.path,
        name: basename(file.path),
      };
      if (file.untracked) {
        openFile(workspaceEntry, split);
        return;
      }
      const id = `diff:${file.path}`;
      const sequence = (diffSequence.current.get(id) ?? 0) + 1;
      diffSequence.current.set(id, sequence);
      const loading: WorkbenchDocument = {
        path: id,
        sourcePath: file.path,
        relativePath: `Diff · ${file.path}`,
        name: basename(file.path),
        language: workspaceEntry.language,
        content: 'Loading diff...',
      };
      setDocuments((current) => ({ ...current, [id]: loading }));
      openCanvasTab(id, split ? 'right' : 'current');
      void readGitComparison(workspaceRoot, file)
        .then((comparison) => {
          if (sequence !== diffSequence.current.get(id)) return;
          setDocuments((current) => ({
            ...current,
            [id]: {
              ...loading,
              content: comparison.patch,
              before: comparison.before,
              after: comparison.after,
            },
          }));
        })
        .catch((error: unknown) => {
          if (sequence !== diffSequence.current.get(id)) return;
          setDocuments((current) => ({
            ...current,
            [id]: {
              ...loading,
              content: '',
              error: error instanceof Error ? error.message : String(error),
            },
          }));
        });
    },
    [files, openCanvasTab, openFile, workspaceRoot]
  );

  const openFileContext = useCallback((file: FileEntry, event: ComponentMouseEvent) => {
    setSelectedPath(file.path);
    setMenu({
      kind: 'file',
      file,
      point: { x: event.x, y: event.y, label: file.relativePath },
    });
    setMenuSelected(0);
  }, []);

  const openGitContext = useCallback((file: GitFile, event: ComponentMouseEvent) => {
    setSelectedGitPath(file.path);
    setMenu({
      kind: 'git',
      file,
      point: { x: event.x, y: event.y, label: file.path },
    });
    setMenuSelected(0);
  }, []);

  const openEditorContext = useCallback((document: WorkbenchDocument, event: ComponentMouseEvent) => {
    if (event.button !== 'right') return;
    setMenu({
      kind: 'editor',
      document,
      point: { x: event.x, y: event.y, label: document.sourcePath },
    });
    setMenuSelected(0);
  }, []);

  const openSessionContext = useCallback(
    (sessionId: string, event: ComponentMouseEvent) => {
      if (event.button !== 'right') return;
      setSelectedSessionId(sessionId);
      setMenu({
        kind: 'session',
        sessionId,
        point: { x: event.x, y: event.y, label: sessions[sessionId]?.title ?? sessionId },
      });
      setMenuSelected(0);
    },
    [sessions]
  );

  const showCopied = useCallback(() => {
    setCopied(true);
    clearTimeout(copyTimer.current);
    copyTimer.current = setTimeout(() => setCopied(false), 1_600);
  }, []);

  const addPathToPrompt = useCallback(
    (path: string) => {
      const id = activeChatSession?.id ?? selectedSession?.id ?? revealWidget('chat');
      if (!id) return;
      updateSession(id, (item) => ({
        ...item,
        draft: `${item.draft}${item.draft && !item.draft.endsWith(' ') ? ' ' : ''}@${path} `,
      }));
      setSelectedSessionId(id);
      revealWidget(id);
    },
    [activeChatSession?.id, revealWidget, selectedSession?.id, updateSession]
  );

  const runMenuAction = useCallback(
    (id: string) => {
      if (!menu) return;
      if (menu.kind === 'session') {
        if (id === 'focus-session') openManagedSession(menu.sessionId);
        if (id === 'split-left-session') openCanvasTab(sessionTabId(menu.sessionId), 'left');
        if (id === 'split-right-session') openCanvasTab(sessionTabId(menu.sessionId), 'right');
        if (id === 'rename-session') beginRenameSession(menu.sessionId);
        if (id === 'new-session') createManagedSession();
        if (id === 'close-session') closeManagedSession(menu.sessionId);
        setMenu(null);
        return;
      }
      if (menu.kind === 'canvas') {
        if (id === 'split-left-canvas') openCanvasTab(menu.tabId, 'left');
        if (id === 'split-right-canvas') openCanvasTab(menu.tabId, 'right');
        if (id === 'close-canvas') closeCanvasView(menu.groupId, menu.tabId);
        const sessionId = sessionIdFromTab(menu.tabId);
        if (id === 'rename-canvas-session' && sessionId) beginRenameSession(sessionId);
        if (id === 'close-canvas-session' && sessionId) closeManagedSession(sessionId);
        setMenu(null);
        return;
      }
      const path =
        menu.kind === 'file' ? menu.file.relativePath : menu.kind === 'git' ? menu.file.path : menu.document.sourcePath;
      const entry = files.find((file) => file.relativePath === path) ?? {
        path: resolve(workspaceRoot, path),
        relativePath: path,
        name: basename(path),
      };
      if (id === 'open' && menu.kind === 'file') openFile(menu.file);
      if (id === 'split' && menu.kind === 'file') {
        openFile(menu.file, true);
      }
      if (id === 'diff' && menu.kind === 'git') openGitFile(menu.file);
      if (id === 'split-diff' && menu.kind === 'git') {
        openGitFile(menu.file, true);
      }
      if (id === 'close' && menu.kind === 'editor') {
        const tab = Object.entries(documents).find(([, document]) => document === menu.document)?.[0];
        if (tab) closeEditor(tab);
      }
      if (id === 'split-editor' && menu.kind === 'editor') {
        const tab = Object.entries(documents).find(([, document]) => document === menu.document)?.[0];
        if (tab) openCanvasTab(tab, 'right');
      }
      if (id === 'files') {
        setSelectedPath(entry.path);
        revealWidget('files');
      }
      if (id === 'git') {
        setSelectedGitPath(path);
        revealWidget('git');
      }
      if (id === 'prompt') addPathToPrompt(path);
      if (id === 'copy') {
        copyToClipboard(path);
        showCopied();
      }
      if (id === 'refresh') {
        if (menu.kind === 'git') refreshGit();
        else refreshAll();
      }
      setMenu(null);
    },
    [
      addPathToPrompt,
      beginRenameSession,
      closeCanvasView,
      closeEditor,
      closeManagedSession,
      createManagedSession,
      files,
      menu,
      documents,
      openFile,
      openGitFile,
      openCanvasTab,
      openManagedSession,
      refreshAll,
      refreshGit,
      revealWidget,
      showCopied,
      workspaceRoot,
    ]
  );

  const promptForAgent = useCallback(
    (text: string): string =>
      buildComposerPrompt({
        text,
        activeLayout,
        layouts: layoutChoices,
        canvasGroups,
        composerRoot,
        composerSkill,
      }),
    [activeLayout.path, activeLayout.spec, canvasGroups, composerRoot, composerSkill, layoutChoices]
  );

  const startPrompt = useCallback(
    (id: string, text: string) => {
      dispatchSession(id, { type: 'user_message', text });
      void clientFor(id)?.prompt(promptForAgent(text));
    },
    [clientFor, dispatchSession, promptForAgent]
  );

  useEffect(() => {
    const ready = Object.values(sessions).find(
      (item) => item.queued.length > 0 && (item.state.connection === 'ready' || item.state.connection === 'error')
    );
    if (!ready) return;
    const [next, ...rest] = ready.queued;
    updateSession(ready.id, (item) => ({ ...item, queued: rest }));
    startPrompt(ready.id, next!);
  }, [sessions, startPrompt, updateSession]);

  const submit = useCallback(
    (id: string, value: string) => {
      const text = value.trim();
      if (!text) return;
      const item = sessions[id];
      if (!item) return;
      updateSession(id, (current) => ({ ...current, draft: '' }));
      if (
        item.state.connection === 'running' ||
        item.state.connection === 'connecting' ||
        item.state.connection === 'cancelling'
      ) {
        updateSession(id, (current) => ({
          ...current,
          queued: [...current.queued, text],
        }));
        return;
      }
      startPrompt(id, text);
    },
    [sessions, startPrompt, updateSession]
  );

  useInput((input, key) => {
    const helpKey = key.f1 || (key.ctrl && (input === '_' || input === '/'));
    if (permission && permissionSession) {
      const choices = permission.choices;
      const permissionClient = clientFor(permissionSession.id);
      if (key.upArrow) {
        setPermissionSelected((value) => Math.max(0, value - 1));
      } else if (key.downArrow) {
        setPermissionSelected((value) => Math.min(choices.length - 1, value + 1));
      } else if (key.return) {
        const choice = choices[permissionSelected];
        if (choice) permissionClient?.choosePermission(choice.id);
      } else if (key.escape) {
        const reject = choices.find((choice) => /reject|deny|cancel/i.test(`${choice.kind} ${choice.label}`));
        if (reject) permissionClient?.choosePermission(reject.id);
        else permissionClient?.dismissPermission();
      } else {
        const choice = choices[Number.parseInt(input, 10) - 1];
        if (choice) permissionClient?.choosePermission(choice.id);
      }
      return;
    }
    if (renameSessionId) {
      if (key.escape) {
        setRenameSessionId(undefined);
        setRenameDraft('');
      }
      return;
    }
    if (menu) {
      if (key.escape) {
        setMenu(null);
      } else if (key.upArrow) {
        setMenuSelected((value) => (value + menuActions.length - 1) % menuActions.length);
      } else if (key.downArrow) {
        setMenuSelected((value) => (value + 1) % menuActions.length);
      } else if (key.return) {
        runMenuAction(menuActions[menuSelected]!.id);
      }
      return;
    }
    if (help) {
      if (helpKey || key.escape || key.return) setHelp(false);
      return;
    }
    if (pickerOpen) {
      const options = layoutChoices.length + 1;
      if (key.upArrow) {
        setPickerSelected((value) => (value + options - 1) % options);
      } else if (key.downArrow) {
        setPickerSelected((value) => (value + 1) % options);
      } else if (key.return) {
        if (pickerSelected === layoutChoices.length) createWithKiro();
        else openLayout(pickerSelected);
      } else if (key.escape || (key.ctrl && input === 'l')) {
        setPickerOpen(false);
      }
      return;
    }
    if (helpKey) {
      setHelp(true);
      return;
    }
    if (key.ctrl && input === 'l') {
      showLayoutPicker();
      return;
    }
    if (key.ctrl && input >= '1' && input <= '4') {
      revealWidget((['files', 'git', 'session', 'settings'] as BuiltinWidget[])[Number.parseInt(input, 10) - 1]!);
      return;
    }
    if (key.ctrl && key.tab) {
      const group = canvasGroups[activeWidget];
      if (group?.tabs.length) {
        const index = group.tabs.indexOf(group.activeId);
        activateCanvasTab(activeWidget, group.tabs[(index + 1) % group.tabs.length]!);
      }
      return;
    }
    if (key.ctrl && input === 'w' && activeCanvasTab) {
      closeCanvasView(activeWidget, activeCanvasTab);
      return;
    }
    if (key.ctrl && input === 'r') {
      refreshAll();
      return;
    }
    if (key.ctrl && input === 'g') {
      setTheme((current) => nextTheme(current));
      return;
    }
    if ((key.ctrl && input === 'x') || (key.escape && controlSession?.state.connection === 'running')) {
      if (controlSession) void clientFor(controlSession.id)?.cancel();
      return;
    }
    if (activeCanvasTab && documents[activeCanvasTab]) {
      if (key.pageUp) {
        updateViewScroll(activeWidget, activeCanvasTab, (value) => Math.max(0, value - activeEditorMetrics.viewport));
      } else if (key.pageDown) {
        updateViewScroll(activeWidget, activeCanvasTab, (value) =>
          Math.min(activeEditorMetrics.maxScroll, value + activeEditorMetrics.viewport)
        );
      }
    }
  });

  useMouse((event) => {
    if (overlayOpen) return;
    if (event.type !== 'scrollup' && event.type !== 'scrolldown') return;
    const delta = event.type === 'scrollup' ? -3 : 3;
    const canvas = canvasNodes.find((node) => inBounds(event, frames[node.id]));
    if (canvas) {
      const tabId = canvasGroups[canvas.id]?.activeId ?? '';
      const sessionId = sessionIdFromTab(tabId);
      if (sessionId) {
        updateViewScroll(canvas.id, tabId, (value) => Math.max(0, value - delta));
      } else if (documents[tabId]) {
        const frame = frames[canvas.id];
        const metrics = editorScrollMetrics(documents[tabId], frame?.width ?? columns, frame?.height ?? bodyHeight);
        updateViewScroll(canvas.id, tabId, (value) => clamp(value + delta, 0, metrics.maxScroll));
      }
    }
  });

  const connectionColor =
    statusSession.connection === 'error'
      ? theme.danger
      : statusSession.connection === 'running' || statusSession.connection === 'cancelling'
        ? theme.warning
        : theme.success;

  const filesPane = ({ id, width, height, left, top }: WidgetFrame): React.ReactElement => (
    <Box width={width} height={height} backgroundColor={theme.panel} onClick={() => setActiveWidget(id)}>
      <FileRail
        files={files}
        selectedPath={selectedPath}
        width={width}
        height={height}
        left={left}
        top={top}
        theme={theme}
        onOpen={openFile}
        onContext={openFileContext}
      />
    </Box>
  );

  const gitPane = ({ id, width, height, left, top }: WidgetFrame): React.ReactElement => (
    <Box width={width} height={height} backgroundColor={theme.panel} onClick={() => setActiveWidget(id)}>
      <GitPanel
        snapshot={git}
        selectedPath={selectedGitPath}
        width={width}
        height={height}
        left={left}
        top={top}
        theme={theme}
        onOpen={openGitFile}
        onContext={openGitContext}
        onRefresh={refreshGit}
      />
    </Box>
  );

  const sessionPane = ({ id, width, height }: WidgetFrame): React.ReactElement => (
    <Box width={width} height={height} backgroundColor={theme.panel} onClick={() => setActiveWidget(id)}>
      <SessionPanel
        sessions={sessionList}
        selectedId={selectedSession?.id}
        width={width}
        height={height}
        theme={theme}
        onSelect={(sessionId) => {
          openManagedSession(sessionId);
        }}
        onCreate={createManagedSession}
        onRename={(session) => beginRenameSession(session.id)}
        onClose={closeManagedSession}
        onContext={(session, event) => openSessionContext(session.id, event)}
      />
    </Box>
  );

  const settingsPane = ({ id, width, height }: WidgetFrame): React.ReactElement => (
    <SettingsPanel
      width={width}
      height={height}
      theme={theme}
      yolo={yolo}
      engine={engine}
      layoutTitle={activeLayout.spec.title}
      onActivate={() => setActiveWidget(id)}
      onNextTheme={() => setTheme((current) => nextTheme(current))}
      onTogglePermissions={() => setYolo((current) => !current)}
    />
  );

  const canvasPane = ({ id, width, height, active }: WidgetFrame): React.ReactElement => {
    const group = canvasGroups[id] ?? { tabs: [], activeId: '' };
    const scrollKey = canvasViewKey(id, group.activeId);
    const viewScroll = viewScrolls[scrollKey] ?? 0;
    return (
      <CanvasPane
        width={width}
        height={height}
        active={active}
        overlayOpen={overlayOpen}
        group={group}
        sessions={sessions}
        documents={documents}
        sessionScroll={viewScroll}
        editorScroll={viewScroll}
        theme={theme}
        onActivatePane={(sessionId) => {
          setActiveWidget(id);
          if (sessionId) setSelectedSessionId(sessionId);
        }}
        onActivateTab={(tabId) => activateCanvasTab(id, tabId)}
        onCloseTab={(tabId) => closeCanvasView(id, tabId)}
        onContext={(tabId, event) => openCanvasContext(id, tabId, event)}
        onSessionScroll={(scroll) => updateViewScroll(id, group.activeId, () => scroll)}
        onDraftChange={(sessionId, draft) => updateSession(sessionId, (current) => ({ ...current, draft }))}
        onSubmit={(sessionId, value) => {
          updateViewScroll(id, group.activeId, () => 0);
          submit(sessionId, value);
        }}
        onEditorContext={openEditorContext}
        onEditorScroll={(scroll) => updateViewScroll(id, group.activeId, () => scroll)}
      />
    );
  };

  const registry = createWidgetRegistry([
    defineWidget({ id: 'chat', title: 'Canvas', render: canvasPane }),
    defineWidget({ id: 'files', title: 'Files', render: filesPane }),
    defineWidget({ id: 'git', title: 'Git', render: gitPane }),
    defineWidget({ id: 'session', title: 'Session', render: sessionPane }),
    defineWidget({ id: 'settings', title: 'Settings', render: settingsPane }),
    defineWidget({ id: 'editor', title: 'Canvas', render: canvasPane }),
  ]);

  const compactTabs: Tab[] = layoutWidgets.map((node) => ({
    id: node.id,
    title: node.title ?? registry.get(node.widget)?.title ?? node.id,
    icon:
      (node.widget === 'chat' || node.widget === 'editor') &&
      sessions[sessionIdFromTab(canvasGroups[node.id]?.activeId ?? '') ?? '']?.state.connection === 'running'
        ? '*'
        : node.widget.slice(0, 1).toUpperCase(),
    dirty: node.widget === 'git' && git.files.length > 0,
  }));
  const compactWidget = registry.get(renderedWidgetNode.widget);

  return (
    <Box width={columns} height={rows} flexDirection="column" backgroundColor={theme.bg}>
      <ComposerHeader
        width={columns}
        theme={theme}
        layoutTitle={activeLayout.spec.title}
        workspace={workspaceLabel(workspaceRoot)}
        status={layoutStatus}
        branch={git.branch}
        onOpenLayouts={showLayoutPicker}
      />
      {compact ? (
        <Box width={columns} height={bodyHeight} flexDirection="column" backgroundColor={theme.bg}>
          {layoutWidgetIds.length > 1 && (
            <Tabs
              tabs={compactTabs}
              activeId={renderedWidget}
              onActivate={(id) => {
                setLayoutTabs((current) => tabsForWidget(activeLayout.spec.layout, id, current));
                setActiveWidget(id);
                const sessionId = sessionIdFromTab(canvasGroups[id]?.activeId ?? '');
                if (sessionId) setSelectedSessionId(sessionId);
              }}
              width={columns}
              activeColor={theme.raised}
              activeTextColor={theme.accent}
              inactiveColor={theme.muted}
              borderColor={theme.border}
              stripColor={theme.panel}
            />
          )}
          {compactWidget ? (
            compactWidget.render({
              id: renderedWidgetNode.id,
              title: renderedWidgetNode.title ?? compactWidget.title,
              width: columns,
              height: Math.max(1, bodyHeight - compactTabHeight),
              left: 0,
              top: 1 + compactTabHeight,
              active: true,
            })
          ) : (
            <Text color={theme.danger}>Unknown widget: {renderedWidget}</Text>
          )}
        </Box>
      ) : (
        <LayoutRenderer
          node={activeLayout.spec.layout}
          width={columns}
          height={bodyHeight}
          left={0}
          top={1}
          registry={registry}
          activeWidget={activeWidget}
          activeTabs={layoutTabs}
          ratios={layoutRatios}
          gitDirty={git.files.length > 0}
          theme={theme}
          onActivateTab={(node, id) => {
            const selected = node.tabs.find((tab) => tab.id === id);
            if (!selected) return;
            const nextTabs = { ...layoutTabs, [node.id]: id };
            setLayoutTabs(nextTabs);
            setActiveWidget(firstWidget(selected.child, nextTabs));
          }}
          onResize={(id, ratio) => setLayoutRatios((current) => ({ ...current, [id]: ratio }))}
        />
      )}
      <ComposerStatusBar
        width={columns}
        theme={theme}
        copied={copied}
        statusColor={connectionColor}
        status={`${statusSession.connection.toUpperCase()} · ${engine} · ${theme.label} · ${yolo ? 'YOLO' : 'ASK'}`}
        onHelp={() => setHelp(true)}
      />
      {menu && !pickerOpen && (
        <ContextMenu
          point={menu.point}
          actions={menuActions}
          selected={menuSelected}
          columns={columns}
          rows={rows}
          theme={theme}
          onSelect={runMenuAction}
        />
      )}
      {renameSessionId && !permission && (
        <RenameSessionDialog
          columns={columns}
          rows={rows}
          value={renameDraft}
          theme={theme}
          onChange={setRenameDraft}
          onSubmit={(value) => {
            renameSession(renameSessionId, value);
            setRenameSessionId(undefined);
            setRenameDraft('');
          }}
        />
      )}
      {help && !permission && <HelpPanel columns={columns} rows={rows} theme={theme} onClose={() => setHelp(false)} />}
      {pickerOpen && !permission && (
        <LayoutPicker
          layouts={layoutChoices}
          selected={pickerSelected}
          columns={columns}
          rows={rows}
          theme={theme}
          onHighlight={setPickerSelected}
          onSelect={openLayout}
          onCreate={createWithKiro}
        />
      )}
      {permission && permissionSession && (
        <PermissionPanel
          prompt={permission}
          selected={permissionSelected}
          columns={columns}
          rows={rows}
          theme={theme}
          onSelect={(id) => clientFor(permissionSession.id)?.choosePermission(id)}
        />
      )}
    </Box>
  );
}
