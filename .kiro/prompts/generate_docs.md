---
description: Generate and update autodocs index for introspect tool
---

# Generate Autodocs

You are tasked with updating the autodocs index used by the introspect tool.

## Process

### 1. Build the doc index

```bash
python3 autodocs/meta/scripts/build-doc-index.py
```

This script:
- Scans all `.md` files in `autodocs/docs/`
- Parses YAML frontmatter (title, description, category, keywords)
- Generates `autodocs/meta/doc-index.json`

### 2. Report results

Tell the user how many docs were indexed.

## When to use

- Before a release to ensure docs are up to date
- After adding/modifying documentation in `autodocs/docs/`
- When introspect tool needs updated search index
