---
doc_meta:
  validated: 2026-01-27
  commit: 7943adae
  status: validated
  testable_headless: true
  category: tool
  title: web_fetch
  description: Fetch and extract content from specific URLs with selective, truncated, or full modes
  keywords: [web_fetch, fetch, url, web, content, extract, trusted, blocked, permissions, enterprise]
  related: [web-search]
---

# web_fetch

Fetch and extract content from specific URLs with selective, truncated, or full modes.

## Overview

> **Note**: This tool is used by the AI assistant to fulfill your requests. You don't invoke it directly - simply ask questions naturally, and the assistant will use this tool to fetch web content as needed.

The web_fetch tool retrieves content from web pages. Supports three extraction modes: selective (smart extraction around search terms), truncated (first 8000 characters), and full (complete content up to 10MB). Use selective mode to read specific parts without filling context.

## Usage

> **Technical Reference**: The JSON examples below show the internal tool format used by the AI assistant. Users should not copy or type these - they are provided for developers and agent configuration authors only.

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

**What this does**: Extracts 10 sentences before/after matches for "authentication" and "authorization". Default mode.

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

### Basic Trust

Add to agent config for permanent trust:

```json
{
  "allowedTools": ["web_fetch"]
}
```

### URL-Based Permissions

Configure granular URL permissions using `toolsSettings`:

```json
{
  "toolsSettings": {
    "web_fetch": {
      "trusted": [".*docs\\.aws\\.amazon\\.com.*", ".*github\\.com.*"],
      "blocked": [".*pastebin\\.com.*", ".*malicious\\.org.*"]
    }
  }
}
```

| Option | Type | Description |
|--------|------|-------------|
| `trusted` | array of regex | URL patterns to auto-allow without prompting |
| `blocked` | array of regex | URL patterns to deny (takes precedence over trusted) |

**Pattern behavior**:
- Patterns are regex and automatically anchored with `^` and `$`
- Invalid regex patterns in `blocked` deny all URLs (fail-safe)
- Invalid regex patterns in `trusted` are skipped
- `blocked` patterns take precedence over `trusted`
- If tool is in `allowedTools` and URL matches neither list, it's allowed
- If tool is not in `allowedTools` and URL matches neither list, user is prompted

## Modes

### selective (default)

Smart extraction around search terms.

**Parameters**:
- `url` (string, required): URL to fetch
- `search_terms` (string, optional): Keywords to find
- `mode`: `"selective"`

**Behavior**: Content is split by sentences (periods). Returns 10 sentences before/after each match. Without search_terms or if no matches found, returns first 20 sentences.

### truncated

First 8000 characters.

**Parameters**:
- `url` (string, required): URL to fetch
- `mode`: `"truncated"`

**Behavior**: Returns first 8000 characters of content.

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
- Not HTML/text content (binary rejected)
- Network issues

**Solution**: Try different mode or URL.

### Issue: Content Not Found

**Symptom**: Empty or irrelevant content  
**Cause**: Search terms don't match page content  
**Solution**: Try different search terms or use truncated/full mode.

### Issue: Tool Requires Approval

**Symptom**: Prompted for permission  
**Cause**: web_fetch not in allowedTools and URL not in trusted patterns  
**Solution**: Approve, add to allowedTools, or add URL pattern to trusted list.

### Issue: URL Blocked

**Symptom**: Permission denied for URL  
**Cause**: URL matches a pattern in blocked list  
**Solution**: Remove pattern from blocked list in toolsSettings.

### Issue: Tool Not Listed

**Symptom**: web_fetch doesn't appear in available tools  
**Cause**: Enterprise administrator has disabled web tools  
**Solution**: Contact your organization's administrator

## Related Features

- [web_search](web-search.md) - Search web for URLs
- [Agent Configuration](../agent-config/overview.md) - Permanent tool trust

## Limitations

- Max 10MB per page
- 30 second timeout
- Max 10 redirects
- HTML/text content only (binary rejected)
- 3 automatic retries with exponential backoff (1s, 2s, 4s)
- No JavaScript execution
- No authentication support
- Regional availability (not in eu-central-1)
- Enterprise administrators can disable web tools for their organization

## Technical Details

**Aliases**: `web_fetch`

**User Agent**: `Kiro-CLI`

**Limits**:
- Selective: 10 sentences context per match, 20 sentences default
- Truncated: 8000 characters
- Full: 10MB max
- Timeout: 30s
- Redirects: 10 max
- Retries: 3 with exponential backoff

**Permissions**: Requires approval unless in allowedTools or URL matches trusted pattern.

**Regional**: Available in most regions except eu-central-1.
