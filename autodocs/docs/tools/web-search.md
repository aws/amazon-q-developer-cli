---
doc_meta:
  validated: 2026-01-27
  commit: b3f10998
  status: validated
  testable_headless: true
  category: tool
  title: web_search
  description: Search the web for current information with automatic source citation
  keywords: [web_search, search, web, internet, research, enterprise]
  related: [web-fetch]
---

# web_search

Search the web for current information with automatic source citation.

## Overview

The web_search tool searches the internet and returns titles, URLs, snippets, and publication dates. Automatically used when AI needs current information. Results include source citations with strict content compliance rules.

## Usage

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

## Troubleshooting

### Issue: Tool Requires Approval

**Symptom**: Prompted for permission  
**Cause**: web_search not in allowedTools  
**Solution**: Approve or add to agent config

### Issue: No Results

**Symptom**: Empty results  
**Cause**: Query too specific or no matches  
**Solution**: Rephrase with different keywords

### Issue: Not Available

**Symptom**: "Web search not available in your region"  
**Cause**: Feature not enabled in eu-central-1  
**Solution**: Feature unavailable in that region

### Issue: Tool Not Listed

**Symptom**: web_search doesn't appear in available tools  
**Cause**: Enterprise administrator has disabled web tools  
**Solution**: Contact your organization's administrator

## Related Features

- [web_fetch](web-fetch.md) - Fetch detailed content from URLs
- [Agent Configuration](../agent-config/overview.md) - Permanent tool trust

## Limitations

- Regional availability (not in eu-central-1)
- Enterprise administrators can disable web tools for their organization
- Requires approval unless in allowedTools
- Results quality depends on search service
- No control over result ranking
- Publication dates may be missing

## Technical Details

**Aliases**: `web_search`

**Permissions**: Requires approval unless in allowedTools.

**Regional**: Available in most regions except eu-central-1.

**Content Rules**: Max 30 consecutive words from any source, must paraphrase and cite.
