import React, { useCallback, useEffect, useMemo, useReducer, useRef, useState } from "react";
import { spawn } from "node:child_process";
import {
  Box,
  EditorInput,
  Split,
  Tabs,
  Text,
  useInput,
  useMouse,
  useSelectionCopy,
  type ComponentMouseEvent,
  type Tab,
} from "twinki";
import { ShowcaseClient } from "./acp-client.js";
import { initialSessionState, sessionReducer } from "./session-state.js";
import { nextTheme, type ShowcaseTheme } from "./themes.js";
import type { ContextPoint, FileEntry, OpenFile, ViewId } from "./types.js";
import { openWorkspaceFile, workspaceLabel } from "./workspace.js";
import { FileRail } from "./components/FileRail.js";
import { FileViewer } from "./components/FileViewer.js";
import { Transcript } from "./components/Transcript.js";
import { ContextMenu, HelpPanel, PermissionPanel, type ContextAction } from "./components/OverlayPanels.js";
const CONTEXT_ACTIONS: ContextAction[] = [
  { id: "preview", label: "Preview file" },
  { id: "prompt", label: "Add path to prompt" },
  { id: "agent", label: "Open agent" },
  { id: "theme", label: "Cycle theme" },
  { id: "help", label: "Keyboard help" },
];
const PANE_ACTIONS: ContextAction[] = [
  { id: "split", label: "Split agent + file" },
  { id: "unify", label: "Unify panes" },
  { id: "equalize", label: "Equalize panes" },
  { id: "agent", label: "Show agent" },
  { id: "file", label: "Show file" },
  { id: "theme", label: "Cycle theme" },
  { id: "help", label: "Keyboard help" },
];
function readTerminalSize(): { columns: number; rows: number } {
  return {
    columns: process.stdout.columns || 100,
    rows: process.stdout.rows || 32,
  };
}
function useTerminalSize(): { columns: number; rows: number } {
  const [size, setSize] = useState(readTerminalSize);
  useEffect(() => {
    const resize = () => setSize(readTerminalSize());
    process.stdout.on("resize", resize);
    return () => {
      process.stdout.off("resize", resize);
    };
  }, []);
  return size;
}
function copyToMacClipboard(text: string): void {
  if (process.platform !== "darwin") return;
  const child = spawn("/usr/bin/pbcopy", {
    stdio: ["pipe", "ignore", "ignore"],
  });
  child.on("error", () => {});
  child.stdin?.end(text);
}
function clamp(value: number, min: number, max: number): number {
  return Math.max(min, Math.min(max, value));
}
export interface AppProps {
  client: ShowcaseClient;
  workspaceRoot: string;
  files: FileEntry[];
  initialTheme: ShowcaseTheme;
  engine: string;
  steeringEnabled: boolean;
}
export function App({
  client,
  workspaceRoot,
  files,
  initialTheme,
  engine,
  steeringEnabled,
}: AppProps): React.ReactElement {
  const { columns, rows } = useTerminalSize();
  const [session, dispatch] = useReducer(sessionReducer, initialSessionState);
  const [theme, setTheme] = useState(initialTheme);
  const [view, setView] = useState<ViewId>("agent");
  const [draft, setDraft] = useState("");
  const [interruptMode, setInterruptMode] = useState<"steer" | "queue">("queue");
  const [queued, setQueued] = useState<string[]>([]);
  const [selectedPath, setSelectedPath] = useState(files[0]?.path);
  const [openFile, setOpenFile] = useState<OpenFile | null>(null);
  const [fileScroll, setFileScroll] = useState(0);
  const [transcriptScroll, setTranscriptScroll] = useState(0);
  const [menu, setMenu] = useState<ContextPoint | null>(null);
  const [menuSelected, setMenuSelected] = useState(0);
  const [help, setHelp] = useState(false);
  const [permissionSelected, setPermissionSelected] = useState(0);
  const [copied, setCopied] = useState(false);
  const [railRatio, setRailRatio] = useState(() => Math.min(0.26, 30 / Math.max(2, columns - 1)));
  const [paneSplit, setPaneSplit] = useState(false);
  const [paneRatio, setPaneRatio] = useState(0.55);
  const copyTimer = useRef<ReturnType<typeof setTimeout> | undefined>(undefined);
  const contentHeight = Math.max(3, rows - 2);
  const splitWidth = Math.max(2, columns - 1);
  const minRailWidth = Math.min(16, Math.floor(splitWidth / 2));
  const railWidth = clamp(Math.round(splitWidth * railRatio), minRailWidth, Math.max(minRailWidth, splitWidth - 20));
  const mainWidth = Math.max(1, splitWidth - railWidth);
  const paneWidth = Math.max(2, mainWidth - 1);
  const minPaneWidth = Math.min(18, Math.floor(paneWidth / 2));
  const agentWidth = clamp(
    Math.round(paneWidth * paneRatio),
    minPaneWidth,
    Math.max(minPaneWidth, paneWidth - minPaneWidth),
  );
  const fileWidth = Math.max(1, paneWidth - agentWidth);
  const composerHeight = Math.min(5, Math.max(3, contentHeight - 2));
  const transcriptHeight = Math.max(1, contentHeight - composerHeight);
  const fileBodyHeight = Math.max(1, contentHeight - 1);
  const fileLineCount = openFile?.content.split("\n").length ?? 0;
  const maxFileScroll = Math.max(0, fileLineCount - fileBodyHeight);
  useEffect(() => client.subscribe(dispatch), [client]);
  useEffect(() => void client.start(), [client]);
  useSelectionCopy((text) => {
    copyToMacClipboard(text);
    setCopied(true);
    clearTimeout(copyTimer.current);
    copyTimer.current = setTimeout(() => setCopied(false), 1600);
  });
  useEffect(() => () => clearTimeout(copyTimer.current), []);
  useEffect(() => {
    setPermissionSelected(0);
    if (session.permission) {
      setMenu(null);
      setHelp(false);
    }
  }, [session.permission]);
  const openPreview = useCallback(
    (file: FileEntry) => {
      setSelectedPath(file.path);
      setOpenFile(openWorkspaceFile(workspaceRoot, file));
      setFileScroll(0);
      setView("file");
      setMenu(null);
    },
    [workspaceRoot],
  );
  const openContext = useCallback((file: FileEntry, event: ComponentMouseEvent) => {
    if (event.button !== "right") return;
    setSelectedPath(file.path);
    setMenu({ x: event.x, y: event.y, label: file.name, file });
    setMenuSelected(0);
  }, []);
  const cycleTheme = useCallback(() => setTheme((current) => nextTheme(current)), []);
  const runContextAction = useCallback(
    (id: string) => {
      if (!menu) return;
      if (id === "preview" && menu.file) openPreview(menu.file);
      if (id === "prompt" && menu.file) {
        setDraft((current) => `${current}${current && !current.endsWith(" ") ? " " : ""}@${menu.file!.relativePath} `);
        setView("agent");
      }
      if (id === "split") setPaneSplit(true);
      if (id === "unify") setPaneSplit(false);
      if (id === "equalize") setPaneRatio(0.5);
      if (id === "agent") setView("agent");
      if (id === "file") setView("file");
      if (id === "theme") cycleTheme();
      if (id === "help") setHelp(true);
      setMenu(null);
    },
    [cycleTheme, menu, openPreview],
  );
  const menuActions = menu?.pane ? PANE_ACTIONS : CONTEXT_ACTIONS;
  const startPrompt = useCallback(
    (text: string) => {
      setTranscriptScroll(0);
      dispatch({ type: "user_message", text });
      void client.prompt(text);
    },
    [client],
  );
  useEffect(() => {
    if (queued.length === 0) return;
    if (session.connection !== "ready" && session.connection !== "error") return;
    const [next, ...rest] = queued;
    setQueued(rest);
    startPrompt(next!);
  }, [queued, session.connection, startPrompt]);
  const submit = useCallback(
    (value: string) => {
      const text = value.trim();
      if (!text) return;
      setDraft("");
      if (session.connection === "running") {
        if (interruptMode === "steer" && steeringEnabled) {
          void client
            .steer(text)
            .then(() => dispatch({ type: "user_message", text }))
            .catch(() => setQueued((items) => [...items, text]));
        } else {
          setQueued((items) => [...items, text]);
        }
        return;
      }
      if (session.connection === "connecting" || session.connection === "cancelling") {
        setQueued((items) => [...items, text]);
        return;
      }
      startPrompt(text);
    },
    [client, interruptMode, session.connection, startPrompt, steeringEnabled],
  );
  useInput((input, key) => {
    const helpKey = key.f1 || (key.ctrl && (input === "_" || input === "/"));
    if (session.permission) {
      if (key.upArrow) {
        setPermissionSelected((value) => Math.max(0, value - 1));
        return;
      }
      if (key.downArrow) {
        setPermissionSelected((value) => Math.min(session.permission!.choices.length - 1, value + 1));
        return;
      }
      if (key.return) {
        const choice = session.permission.choices[permissionSelected];
        if (choice) client.choosePermission(choice.id);
        return;
      }
      if (key.escape) {
        const reject = session.permission.choices.find((choice) =>
          /reject|deny|cancel/i.test(`${choice.kind} ${choice.label}`),
        );
        if (reject) client.choosePermission(reject.id);
        else client.dismissPermission();
        return;
      }
      const choice = Number.parseInt(input, 10) - 1;
      if (choice >= 0 && choice < session.permission.choices.length) {
        client.choosePermission(session.permission.choices[choice]!.id);
      }
      return;
    }
    if (menu) {
      if (key.escape) {
        setMenu(null);
        return;
      }
      if (key.upArrow) {
        setMenuSelected((value) => (value + menuActions.length - 1) % menuActions.length);
        return;
      }
      if (key.downArrow) {
        setMenuSelected((value) => (value + 1) % menuActions.length);
        return;
      }
      if (key.return) {
        runContextAction(menuActions[menuSelected]!.id);
        return;
      }
      return;
    }
    if (help) {
      if (key.escape || key.return || helpKey) {
        setHelp(false);
      }
      return;
    }
    if (helpKey) {
      setHelp(true);
      return;
    }
    if (key.ctrl && key.tab) {
      setView((current) => (current === "agent" && (openFile || paneSplit) ? "file" : "agent"));
      return;
    }
    if ((key.ctrl && input === "x") || (key.escape && session.connection === "running")) {
      void client.cancel();
      return;
    }
    if (key.ctrl && input === "g") {
      cycleTheme();
      return;
    }
    if (key.ctrl && input === "s") {
      if (steeringEnabled) {
        setInterruptMode((current) => (current === "steer" ? "queue" : "steer"));
      }
      return;
    }
    if (view === "file") {
      if (key.escape) setView("agent");
      if (key.pageUp) setFileScroll((value) => Math.max(0, value - fileBodyHeight));
      if (key.pageDown) {
        setFileScroll((value) => Math.min(maxFileScroll, value + fileBodyHeight));
      }
    }
  });

  useMouse((event) => {
    if (
      event.type === "mousedown" &&
      event.button === "right" &&
      event.x > railWidth &&
      event.y > 0 &&
      event.y < rows - 1 &&
      !menu &&
      !help &&
      !session.permission
    ) {
      const pane = paneSplit ? (event.x < railWidth + 1 + agentWidth ? "agent" : "file") : view;
      setView(pane);
      setMenu({
        x: event.x,
        y: event.y,
        label: pane === "agent" ? session.agentName : (openFile?.name ?? "File preview"),
        pane,
      });
      setMenuSelected(0);
      return;
    }
    if (paneSplit && event.type === "mousedown" && event.button === "left" && event.x > railWidth)
      setView(event.x < railWidth + 1 + agentWidth ? "agent" : "file");
    if (event.type !== "scrollup" && event.type !== "scrolldown") return;
    const delta = event.type === "scrollup" ? -3 : 3;
    if (event.x < railWidth) return;
    if (paneSplit ? event.x >= railWidth + 1 + agentWidth : view === "file") {
      setFileScroll((value) => clamp(value + delta, 0, maxFileScroll));
    } else {
      setTranscriptScroll((value) => Math.max(0, value - delta));
    }
  });

  const tabs = useMemo<Tab[]>(
    () => [
      {
        id: "agent",
        title: session.agentName,
        icon: session.connection === "running" ? "*" : "A",
        iconColor: session.connection === "error" ? theme.danger : theme.success,
      },
      ...(openFile
        ? [
            {
              id: "file",
              title: openFile.name,
              icon: "F",
              closable: true,
            },
          ]
        : []),
    ],
    [openFile, session.agentName, session.connection, theme],
  );

  const connectionColor =
    session.connection === "error"
      ? theme.danger
      : session.connection === "running" || session.connection === "cancelling"
        ? theme.warning
        : theme.success;
  const agentPane = (width: number): React.ReactElement => (
    <>
      <Transcript
        blocks={session.blocks}
        active={session.connection === "running"}
        scrollFromBottom={transcriptScroll}
        width={width}
        height={transcriptHeight}
        theme={theme}
        onScrollFromBottom={setTranscriptScroll}
      />
      <EditorInput
        value={draft}
        onChange={setDraft}
        onSubmit={submit}
        isActive={!menu && !help && !session.permission && view === "agent"}
        visibleLines={Math.max(1, composerHeight - 2)}
        width={width}
        color={theme.fg}
        backgroundColor={theme.panel}
        placeholder="Ask the ACP agent..."
        mouseCursor
      />
    </>
  );
  const filePane = (width: number): React.ReactElement => (
    <FileViewer
      file={openFile}
      scrollTop={fileScroll}
      width={width}
      height={contentHeight}
      theme={theme}
      onScroll={setFileScroll}
    />
  );

  return (
    <Box flexDirection="column" width={columns} height={rows} backgroundColor={theme.bg}>
      <Box flexDirection="row" height={1}>
        <Box width={railWidth} paddingX={1} backgroundColor={theme.accent}>
          <Text color={theme.accentText} bold wrap="truncate">
            TWINKI ACP
          </Text>
        </Box>
        <Text color={theme.border}>│</Text>
        <Tabs
          tabs={tabs}
          activeId={view}
          onActivate={(id) => setView(id as ViewId)}
          onClose={() => {
            setOpenFile(null);
            setPaneSplit(false);
            setView("agent");
          }}
          width={mainWidth}
          activeColor={theme.accent}
          activeTextColor={theme.accentText}
          inactiveColor={theme.muted}
          borderColor={theme.border}
          stripColor={theme.panel}
        />
      </Box>
      <Split
        direction="row"
        ratio={railWidth / splitWidth}
        width={columns}
        height={contentHeight}
        activeColor={theme.accent}
        inactiveColor={theme.border}
        showPaneBorders={false}
        onResize={setRailRatio}
      >
        <FileRail
          files={files}
          selectedPath={selectedPath}
          width={railWidth}
          height={contentHeight}
          top={1}
          theme={theme}
          onOpen={openPreview}
          onContext={openContext}
        />
        <Box flexDirection="column" width={mainWidth} height={contentHeight} backgroundColor={theme.bg}>
          {paneSplit ? (
            <Split
              direction="row"
              ratio={agentWidth / paneWidth}
              width={mainWidth}
              height={contentHeight}
              activePane={view === "agent" ? "a" : "b"}
              activeColor={theme.accent}
              inactiveColor={theme.border}
              showPaneBorders={false}
              onResize={setPaneRatio}
            >
              {agentPane(agentWidth)}
              {filePane(fileWidth)}
            </Split>
          ) : view === "agent" ? (
            agentPane(mainWidth)
          ) : (
            filePane(mainWidth)
          )}
        </Box>
      </Split>
      <Box height={1} paddingX={1} justifyContent="space-between" backgroundColor={theme.raised}>
        <Box>
          <Text color={copied ? theme.success : connectionColor} bold wrap="truncate">
            {copied
              ? "[+] Copied to clipboard"
              : `${session.connection.toUpperCase()} | ${engine}${session.mode ? ` | ${session.mode}` : ""}`}
          </Text>
          <Text
            color={theme.accent}
            bold
            onClick={() => steeringEnabled && setInterruptMode((value) => (value === "steer" ? "queue" : "steer"))}
          >
            {` | ${interruptMode.toUpperCase()}${queued.length ? `:${queued.length}` : ""}`}
          </Text>
        </Box>
        <Box>
          <Text color={theme.muted} wrap="truncate-middle">
            {`${workspaceLabel(workspaceRoot)} | `}
          </Text>
          <Text color={theme.accent} bold onClick={cycleTheme}>
            {theme.label}
          </Text>
          <Text color={theme.muted}>{` | ${Math.round(session.contextPercent)}% | `}</Text>
          <Text color={theme.accent} bold onClick={() => setHelp(true)}>
            ? HELP
          </Text>
        </Box>
      </Box>
      {menu && (
        <ContextMenu
          point={menu}
          actions={menuActions}
          selected={menuSelected}
          columns={columns}
          rows={rows}
          theme={theme}
          onSelect={runContextAction}
        />
      )}
      {session.permission && (
        <PermissionPanel
          prompt={session.permission}
          selected={permissionSelected}
          columns={columns}
          rows={rows}
          theme={theme}
          onSelect={(id) => client.choosePermission(id)}
        />
      )}
      {help && !session.permission && (
        <HelpPanel columns={columns} rows={rows} theme={theme} onClose={() => setHelp(false)} />
      )}
    </Box>
  );
}
