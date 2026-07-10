# 28-file-editor — File Tree + Text Editor

A two-pane terminal editor inspired by **Neovim / NvChad**: an `nvim-tree`-style
file explorer on the left, a breadcrumb winbar + syntax-highlighted editor on
the right, and an NvChad-style statusline at the bottom. View files by clicking
or with the keyboard, edit them, and save back to disk. Syntax themes rotate
live with `Tab` / `Shift+Tab`.

## Run

From the twinki package root (`packages/twinki`):

```bash
# build the library once (examples import from dist/)
npx tsc -p packages/twinki/tsconfig.build.json

npx tsx examples/28-file-editor/index.tsx
```

Mouse support is enabled, so clicking files works in most terminals.

## Controls

NORMAL mode (explorer focused):

| Key | Action |
| --- | --- |
| `↑`/`↓` or `k`/`j` | Move the cursor |
| `Enter` | Open a file / expand-collapse a folder |
| `←` / `→` | Collapse / expand a folder |
| Click a row | Select + open/toggle |
| `e` | Edit the open file (enter INSERT) |
| `Tab` / `Shift+Tab` | Next / previous syntax theme |
| `Ctrl+S` | Save |
| `q` | Quit |

INSERT mode (editor focused):

| Key | Action |
| --- | --- |
| `Esc` | Back to view (NORMAL) |
| `Ctrl+S` | Save |
| anything else | Edits the buffer |

The `●` marker in the statusline means the buffer has unsaved changes.

## Theme: Monokai Pro

The UI chrome (borders, statusline, tree, accents) uses the **Monokai Pro
(Spectrum)** palette by default. Monokai Pro is a proprietary theme and is not
bundled with shiki, so **code** syntax highlighting defaults to shiki's
`monokai` — its closest bundled equivalent. To use a real Monokai Pro TextMate
theme you own, register it once before rendering and set it as the default:

```ts
import { getHighlighter } from 'twinki';
const hl = await getHighlighter();
await hl.loadTheme(myMonokaiProThemeJson); // name it e.g. "monokai-pro"
// then put "monokai-pro" first in hooks/useThemeRotation.ts
```

## Structure

Organized as a small feature module following SRP / KISS / DRY and the modern
Container/Presenter (hooks-as-container) pattern:

```
28-file-editor/
  index.tsx              entry point (render with mouse enabled)
  App.tsx                composition root: wires hooks -> panes, routes input
  types.ts               shared TreeNode / VisibleRow types
  lib/                   data layer (pure, no React)
    workspace.ts         scoped filesystem IO + path-traversal guard + tree
    language.ts          file extension -> shiki language
    palette.ts           Monokai Pro (Spectrum) UI colors
  hooks/                 logic layer (stateful "containers")
    useFileTree.ts       expand/collapse, cursor, flatten visible rows
    useEditorSession.ts  open file, buffer, dirty tracking, save
    useThemeRotation.ts  cycle syntax themes
  components/            presentation layer (pure, props in / callbacks out)
    FileTree.tsx         left pane
    TreeRow.tsx          one explorer row
    EditorPane.tsx       right pane (breadcrumb + editor)
    StatusBar.tsx        bottom statusline
  sample-workspace/      bundled throwaway files to browse and edit
```

All filesystem access is scoped to `sample-workspace/`, so editing and saving
only ever touch those demo files. Reset them with `git checkout`.
