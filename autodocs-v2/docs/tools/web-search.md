---
doc_meta:
  validated: 2026-06-09
  commit: 5bc633954
  status: validated
  testable_headless: true
  category: tool
  title: web_search
  description: Search the web for current information with automatic source citation
  keywords: [web_search, search, web, internet, research, governance, enterprise]
  related: [web-fetch]
---

# web_search

Search the web for current information with automatic source citation.

## Overview

> **Note**: This tool is used by the AI assistant to fulfill your requests. You don't invoke it directly - simply ask questions naturally, and the assistant will use this tool to search the web as needed.

The web_search tool searches the internet and returns titles, URLs, snippets, and publication dates. Automatically used when AI needs current information. Results include source citations with strict content compliance rules.

## Enterprise Governance

For enterprise users (IDC/ExternalIDP) and API key users, web tools availability is controlled by your organization's governance settings. If your administrator has disabled web tools, this tool will not be available even if explicitly configured in an agent.

Social login and Builder ID users are not subject to governance restrictions—web tools are enabled by default.

## Usage

> **Technical Reference**: The JSON examples below show the internal tool format used by the AI assistant. Users should not copy or type these - they are provided for developers and agent configuration authors only.

### Basic Usage

```json
{
  "query": "latest React version"
}
```

### Common Use Cases

#### Use Case 1: Current Information

```json
{
  "query": "AWS Lambda pricing us-east-1"
}
```

**What this does**: Searches for current Lambda pricing information.

#### Use Case 2: Verify Facts

```json
{
  "query": "Python 3.13 release date"
}
```

**What this does**: Finds release information with dates.

#### Use Case 3: Research Topic

```json
{
  "query": "WebAssembly performance benchmarks"
}
```

**What this does**: Searches for technical information and comparisons.

## Configuration

No agent configuration - web_search requires approval unless in allowedTools.

## Output Format

Returns JSON with results array:

```json
{
  "results": [
    {
      "title": "Page Title",
      "url": "https://example.com",
      "snippet": "Brief excerpt...",
      "publishedDate": "2025-11-20T10:30:00Z",
      "domain": "example.com",
      "id": "unique-id",
      "maxVerbatimWordLimit": 30,
      "publicDomain": false
    }
  ]
}
```

## Content Compliance

AI follows strict rules when using search results:

- **Attribution**: All sources cited with inline links
- **Verbatim Limit**: Max 30 consecutive words from any source
- **Paraphrasing**: Content rephrased for compliance
- **References**: Sequential numbering [1], [2], [3] at end

## Examples

### Example 1: Latest Version

```
> What's the latest version of React?
```

**Output**:
```
Here's what I found:
React 18.3.1 is the current stable version...

References:
[1] React - Official Documentation - https://react.dev
```

### Example 2: Pricing Information

```
> Search for AWS Lambda pricing
```

**Output**:
```
Here's what I found:
AWS Lambda pricing is $0.20 per 1M requests...

References:
[1] AWS Lambda Pricing - https://aws.amazon.com/lambda/pricing/
```

## Web Tools Governance

Administrators can disable web tools (web_search, web_fetch) via the Kiro console. When disabled:

- web_search is unavailable to the agent
- The `/tools` panel shows a warning: "Web tools have been disabled by your administrator"
- A transient alert appears at session start

If the governance API cannot be reached, web tools are disabled as a fail-closed safety measure with the message: "Failed to retrieve web tools settings — web tools disabled". When both MCP and web tools are disabled due to the same API failure, a single coalesced message is shown: "failed to retrieve governance settings — MCP and web tools disabled".

This applies to enterprise users (IAM Identity Center) and API key users. Builder ID and social auth users are not subject to web tools governance.

## Troubleshooting

### Issue: Web Tools Disabled by Governance

**Symptom**: web_search tool not available  
**Cause**: Organization administrator disabled web tools  
**Solution**: Contact your administrator to enable web tools in governance settings

### Issue: Tool Requires Approval

**Symptom**: Prompted for permission  
**Cause**: web_search not in allowedTools  
**Solution**: Approve or add to agent config

### Issue: No Results

**Symptom**: Empty results  
**Cause**: Query too specific or no matches  
**Solution**: Rephrase with different keywords

## Related Features

- [web_fetch](web-fetch.md) - Fetch detailed content from URLs
- [Agent Configuration](../features/agent-configuration.md) - Permanent tool trust

## Limitations

- Requires approval unless in allowedTools
- Results quality depends on search service
- No control over result ranking
- Publication dates may be missing
- Enterprise/API key users: subject to organization governance settings

## Technical Details

**Aliases**: `web_search`

**Permissions**: Requires approval unless in allowedTools.

**Content Rules**: Max 30 consecutive words from any source, must paraphrase and cite.
