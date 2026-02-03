---
doc_meta:
  validated: 2026-02-03
  commit: 78ad87ee
  status: validated
  testable_headless: true
  category: setting
  title: introspect.progressiveMode
  description: Use progressive loading instead of semantic search for introspect
  keywords: [setting, introspect, progressive, semantic, search, enterprise, model]
  related: [introspect, introspect-tangent-mode]
---

# introspect.progressiveMode

Use progressive loading instead of semantic search for introspect.

## Overview

Controls whether the introspect tool uses progressive loading (doc index) instead of semantic search. When enabled, bypasses the embedding model download and semantic search, returning the documentation index directly. The LLM then progressively fetches specific docs as needed.

This is useful for enterprise environments where model downloads may be blocked or restricted.

## Usage

```bash
kiro-cli settings set introspect.progressiveMode true
```

**Type**: Boolean  
**Default**: `false`

## How It Works

When `false` (default):
1. Introspect downloads embedding models from AWS CDN
2. Uses semantic search to find relevant documentation
3. Returns matched docs directly

When `true`:
1. Skips model download entirely
2. Returns the full documentation index
3. LLM selects and fetches specific docs via `doc_path` parameter

## Related

- [introspect](../tools/introspect.md) - Introspect tool
- [introspect.tangentMode](introspect-tangent-mode.md) - Auto-tangent for introspect

## Examples

### Example 1: Enable Progressive Mode

```bash
kiro-cli settings set introspect.progressiveMode true
```

Introspect will skip semantic search and use progressive loading.

### Example 2: Check Current Value

```bash
kiro-cli settings get introspect.progressiveMode
```

### Example 3: Disable (Return to Semantic Search)

```bash
kiro-cli settings unset introspect.progressiveMode
```

## Troubleshooting

### Model Download Blocked

If you see errors about model downloads failing, enable progressive mode:

```bash
kiro-cli settings set introspect.progressiveMode true
```

### Slower Doc Retrieval

Progressive mode may require multiple introspect calls (one for index, then specific docs). This is expected behavior - the tradeoff is avoiding model downloads.
