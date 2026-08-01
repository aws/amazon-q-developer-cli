import type { ContextAction } from '../32-acp-showcase/components/OverlayPanels.js';
import type { ContextPoint, FileEntry } from '../32-acp-showcase/types.js';
import type { GitFile } from './git.js';
import { sessionIdFromTab, type WorkbenchDocument } from './workbench-state.js';

export type MenuState =
  | { point: ContextPoint; kind: 'file'; file: FileEntry }
  | { point: ContextPoint; kind: 'git'; file: GitFile }
  | { point: ContextPoint; kind: 'session'; sessionId: string }
  | { point: ContextPoint; kind: 'canvas'; groupId: string; tabId: string }
  | {
      point: ContextPoint;
      kind: 'editor';
      document: WorkbenchDocument;
    };

const FILE_ACTIONS: ContextAction[] = [
  { id: 'open', label: 'Open' },
  { id: 'split', label: 'Open in split layout' },
  { id: 'prompt', label: 'Add path to prompt' },
  { id: 'copy', label: 'Copy relative path' },
  { id: 'git', label: 'Reveal in Git' },
  { id: 'refresh', label: 'Refresh workspace' },
];

const GIT_ACTIONS: ContextAction[] = [
  { id: 'diff', label: 'Open diff' },
  { id: 'split-diff', label: 'Open diff to side' },
  { id: 'files', label: 'Reveal in Files' },
  { id: 'prompt', label: 'Add path to prompt' },
  { id: 'copy', label: 'Copy relative path' },
  { id: 'refresh', label: 'Refresh Git' },
];

const EDITOR_ACTIONS: ContextAction[] = [
  { id: 'close', label: 'Close editor' },
  { id: 'split-editor', label: 'Use split layout' },
  { id: 'files', label: 'Reveal in Files' },
  { id: 'git', label: 'Reveal in Git' },
  { id: 'prompt', label: 'Add path to prompt' },
  { id: 'copy', label: 'Copy relative path' },
];

const SESSION_ACTIONS: ContextAction[] = [
  { id: 'focus-session', label: 'Focus session' },
  { id: 'split-left-session', label: 'Open in left group' },
  { id: 'split-right-session', label: 'Open in right group' },
  { id: 'rename-session', label: 'Rename' },
  { id: 'new-session', label: 'New session' },
  { id: 'close-session', label: 'Close session' },
];

const CANVAS_ACTIONS: ContextAction[] = [
  { id: 'split-left-canvas', label: 'Split left' },
  { id: 'split-right-canvas', label: 'Split right' },
  { id: 'close-canvas', label: 'Close tab' },
  { id: 'rename-canvas-session', label: 'Rename session' },
  { id: 'close-canvas-session', label: 'Close session' },
];

export function actionsForMenu(menu: MenuState | null): ContextAction[] {
  if (!menu) return [];
  if (menu.kind === 'file') return FILE_ACTIONS;
  if (menu.kind === 'git') return GIT_ACTIONS;
  if (menu.kind === 'session') return SESSION_ACTIONS;
  if (menu.kind === 'canvas') {
    return sessionIdFromTab(menu.tabId) ? CANVAS_ACTIONS : CANVAS_ACTIONS.slice(0, 3);
  }
  return EDITOR_ACTIONS;
}
