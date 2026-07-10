/**
 * 28-file-editor — File tree + text editor
 *
 * Run from the twinki package root:
 *   npx tsx examples/28-file-editor/index.tsx
 *
 * A two-pane editor inspired by Neovim / NvChad: an nvim-tree-style explorer
 * on the left, a breadcrumb winbar + syntax-highlighted editor on the right,
 * and an NvChad-style statusline at the bottom. Click a file or navigate with
 * the keyboard to view it; press `e` to edit and Ctrl+S to save. Syntax themes
 * rotate with Tab / Shift+Tab (default: monokai).
 *
 * IO is scoped to the bundled ./sample-workspace folder, so editing and saving
 * only ever touch those throwaway demo files.
 */
import React from 'react';
import { dirname, join } from 'node:path';
import { fileURLToPath } from 'node:url';
import { render } from 'twinki';
import { App } from './App.js';

const here = dirname(fileURLToPath(import.meta.url));
const workspaceRoot = join(here, 'sample-workspace');

// `mouse: true` is REQUIRED: click-to-open relies on mouse hit-testing.
render(<App workspaceRoot={workspaceRoot} />, { mouse: true, exitOnCtrlC: true });
