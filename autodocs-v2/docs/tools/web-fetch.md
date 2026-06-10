---
doc_meta:
  validated: 2026-06-09
  commit: 5bc633954
  status: validated
  testable_headless: true
  category: tool
  title: web_fetch
  description: Fetch and extract content from specific URLs with selective, truncated, or full modes
  keywords: [web_fetch, fetch, url, web, content, extract, trusted, blocked, url-permission, governance, admin, disabled]
  related: [web-search, trust-configuration, agent-configuration, tools]
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

Add to agent config for permanent trust:

```json
{
  "allowedTools": ["web_fetch"]
}
```

### URL Permissions

Control which URLs are auto-allowed or denied using regex patterns in `toolsSettings.web_fetch`:

```json
{
  "toolsSettings": {
    "web_fetch": {
      "trusted": [".*docs\\.aws\\.amazon\\.com.*"],
      "blocked": [".*github\\.com.*", ".*pastebin\\.com.*"]
    }
  }
}
```

**Fields**:
- `trusted` — URL regex patterns to auto-allow without prompting
- `blocked` — URL regex patterns to always deny (takes precedence over trusted)

**Evaluation order**:
1. `blocked` patterns checked first — if any match, the request is denied
2. `trusted` patterns checked next — if any match, the request is auto-allowed
3. If no pattern matches, falls back to whether `web_fetch` is in `allowedTools`

**Regex behavior**:
- Patterns are automatically anchored with `^` and `$` if not already present
- Invalid regex in `blocked` list denies all URLs (fail-safe)
- Invalid regex in `trusted` list is silently skipped

**Example: Allow documentation sites, block code hosting**:

```json
{
  "toolsSettings": {
    "web_fetch": {
      "trusted": [
        ".*docs\\.aws\\.amazon\\.com.*",
        ".*docs\\.python\\.org.*",
        ".*developer\\.mozilla\\.org.*"
      ],
      "blocked": [
        ".*github\\.com.*",
        ".*gitlab\\.com.*",
        ".*pastebin\\.com.*"
      ]
    }
  }
}
```

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

## Web Tools Governance

Administrators can disable web tools (web_search, web_fetch) via the Kiro console. When disabled:

- web_fetch is unavailable to the agent
- The `/tools` panel shows a warning: "Web tools have been disabled by your administrator"
- A transient alert appears at session start

If the governance API cannot be reached, web tools are disabled as a fail-closed safety measure with the message: "Failed to retrieve web tools settings — web tools disabled". When both MCP and web tools are disabled due to the same API failure, a single coalesced message is shown: "failed to retrieve governance settings — MCP and web tools disabled".

This applies to enterprise users (IAM Identity Center) and API key users. Builder ID and social auth users are not subject to web tools governance.

## Troubleshooting

### Issue: Web Tools Disabled by Administrator

**Symptom**: "Web tools have been disabled by your administrator" warning  
**Cause**: Your organization's administrator has disabled web tools via the Kiro console  
**Solution**: Contact your administrator to enable web tools

### Issue: Failed to Retrieve Web Tools Settings

**Symptom**: "Failed to retrieve web tools settings — web tools disabled" warning  
**Cause**: Could not reach the governance API to check web tools settings. For security, web tools are disabled when settings cannot be verified (fail-closed).  
**Solution**: Check network connectivity. If the issue persists, contact your administrator.

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

## Related Features

- [web_search](web-search.md) - Search web for URLs
- [Agent Configuration](../features/agent-configuration.md) - Permanent tool trust

## Limitations

- Max 10MB per page
- 30 second timeout
- Max 10 redirects
- HTML/text content only (binary rejected)
- 3 automatic retries with exponential backoff (1s, 2s)
- No JavaScript execution
- No authentication support

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

**Permissions**: Requires approval unless in allowedTools or URL matches a `trusted` pattern in `toolsSettings.web_fetch`. Denied if URL matches a `blocked` pattern.
