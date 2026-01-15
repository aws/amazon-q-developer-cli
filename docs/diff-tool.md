# Custom Diff Tools for the Write Tool

You can configure **external diff tools** to view code changes produced by the write tool, instead of using the built-in inline diff.

This allows you to review changes using familiar terminal or GUI-based diff viewers.

## Setup

Configure the diff tool via CLI settings:

```bash
kiro-cli settings chat.diffTool <tool>
````

## Supported Diff Tools

### Terminal (Inline) Tools

These tools display diffs **directly in your terminal**:

| Tool           | Config                                           | Notes                                   |
| -------------- | ------------------------------------------------ | --------------------------------------- |
| delta          | `kiro-cli settings chat.diffTool delta`          | Git-style diff with syntax highlighting |
| difftastic     | `kiro-cli settings chat.diffTool difft`          | Structural, language-aware diff         |
| icdiff         | `kiro-cli settings chat.diffTool icdiff`         | Side-by-side colored diff               |
| diff-so-fancy  | `kiro-cli settings chat.diffTool diff-so-fancy`  | Human-readable diff output              |
| colordiff      | `kiro-cli settings chat.diffTool colordiff`      | Colorized unified diff                  |
| diff-highlight | `kiro-cli settings chat.diffTool diff-highlight` | Git word-level highlighting             |
| ydiff          | `kiro-cli settings chat.diffTool ydiff`          | Side-by-side with word diff             |
| bat            | `kiro-cli settings chat.diffTool bat`            | Syntax-highlighted output               |

### GUI Tools

These tools open a **separate diff window**:

| Tool     | Config                                     | Notes                      |
| -------- | ------------------------------------------ | -------------------------- |
| VS Code  | `kiro-cli settings chat.diffTool code`     | Opens diff in VS Code      |
| VSCodium | `kiro-cli settings chat.diffTool codium`   | Opens diff in VSCodium     |
| Meld     | `kiro-cli settings chat.diffTool meld`     | Visual diff and merge      |
| KDiff3   | `kiro-cli settings chat.diffTool kdiff3`   | Cross-platform diff viewer |
| opendiff | `kiro-cli settings chat.diffTool opendiff` | macOS FileMerge            |
| vimdiff  | `kiro-cli settings chat.diffTool vimdiff`  | Vim-based diff viewer      |
| nvim     | `kiro-cli settings chat.diffTool nvim`     | Neovim diff mode           |
| vim      | `kiro-cli settings chat.diffTool vim`      | Vim diff mode              |

## Custom Arguments

You can pass additional arguments to the diff tool.

Example: enable side-by-side mode in `delta`:

```bash
kiro-cli settings chat.diffTool "delta --side-by-side"
```

## Other Tools

Tools not listed above may still work.

The CLI will attempt the following, in order:

1. Pipe a unified diff to the tool via **stdin**
2. Invoke the tool with **two file paths** as arguments

If both approaches fail, the CLI falls back to the built-in inline diff.

## Limitations

* For GUI tools, diff files are **view-only**
* Any edits made in the diff viewer are **not applied** back to the write tool changes

## Troubleshooting

### Diff Tool Not Found

If you see the error:

```
Couldn't find the diff tool 'X'. Make sure it's installed and available in your PATH.
```

Install the diff tool and ensure it’s on your PATH. For example:

```bash
which delta
```

## Disable Custom Diff Tool

To disable custom diff tools and revert to the built-in inline diff:

```bash
kiro-cli settings -d chat.diffTool
```
