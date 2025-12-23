---
doc_meta:
  validated: 2025-12-19
  commit: 57090ffe
  status: validated
  testable_headless: true
  category: tool
  title: web_fetch
  description: Fetch and extract content from specific URLs with selective, truncated, or full modes
  keywords: [web_fetch, fetch, url, web, content, extract]
  related: [web-search]
---

# web_fetch

Fetch and extract content from specific URLs with selective, truncated, or full modes.

## Overview

The web_fetch tool retrieves content from web pages. Supports three extraction modes: selective (smart extraction around search terms), truncated (first 8KB), and full (complete content up to 10MB). Use selective mode to read specific parts without filling context.

## Usage

### Basic Usage

```json
{
  "url": "https://example.com/page"
}
```

### Common Use Cases

#### Use Case 1: Selective Extraction

```json
{
  "url": "https://docs.example.com/api",
  "mode": "selective",
  "search_terms": "authentication authorization"
}
```

**What this does**: Extracts ~10 lines before/after matches for "authentication" and "authorization". Default mode.

#### Use Case 2: Truncated Content

```json
{
  "url": "https://blog.example.com/article",
  "mode": "truncated"
}
```

**What this does**: Gets first 8000 characters. Good for article previews.

#### Use Case 3: Full Content

```json
{
  "url": "https://example.com/documentation",
  "mode": "full"
}
```

**What this does**: Retrieves complete page content (up to 10MB). Use for comprehensive analysis.

## Configuration

No agent configuration - web_fetch requires approval unless in allowedTools.

## Modes

### selective (default)

Smart extraction around search terms.

**Parameters**:
- `url` (string, required): URL to fetch
- `search_terms` (string, optional): Keywords to find
- `mode`: `"selective"`

**Behavior**: Returns ~10 lines before/after each match. Without search_terms, returns beginning of page.

### truncated

First 8000 characters.

**Parameters**:
- `url` (string, required): URL to fetch
- `mode`: `"truncated"`

**Behavior**: Returns first 8KB of content.

### full

Complete content.

**Parameters**:
- `url` (string, required): URL to fetch
- `mode`: `"full"`

**Behavior**: Returns entire page (up to 10MB).

## Examples

### Example 1: Get Installation Instructions

```json
{
  "url": "https://docs.python.org/3/installing/",
  "mode": "selective",
  "search_terms": "pip install"
}
```

### Example 2: Read Article

```json
{
  "url": "https://blog.rust-lang.org/2024/12/19/release.html",
  "mode": "truncated"
}
```

### Example 3: Full Documentation Page

```json
{
  "url": "https://doc.rust-lang.org/book/ch01-00-getting-started.html",
  "mode": "full"
}
```

## Troubleshooting

### Issue: Fetch Failed

**Symptom**: Error fetching URL  
**Causes**:
- Page >10MB
- Timeout >30s
- Too many redirects (>10)
- Not HTML content
- Network issues

**Solution**: Try different mode or URL.

### Issue: Content Not Found

**Symptom**: Empty or irrelevant content  
**Cause**: Search terms don't match page content  
**Solution**: Try different search terms or use truncated/full mode.

### Issue: Tool Requires Approval

**Symptom**: Prompted for permission  
**Cause**: web_fetch not in allowedTools  
**Solution**: Approve or add to agent config.

## Related Features

- [web_search](web-search.md) - Search web for URLs
- [Agent Configuration](../agent-config/overview.md) - Permanent tool trust

## Limitations

- Max 10MB per page
- 30 second timeout
- Max 10 redirects
- HTML content only
- 3 automatic retries
- No JavaScript execution
- No authentication support
- Regional availability (not in eu-central-1)

## Technical Details

**Aliases**: `web_fetch`

**User Agent**: `Kiro-CLI`

**Limits**:
- Selective: ~10 lines context per match
- Truncated: 8000 chars
- Full: 10MB max
- Timeout: 30s
- Redirects: 10 max
- Retries: 3

**Permissions**: Requires approval unless in allowedTools.

**Regional**: Available in most regions except eu-central-1.
