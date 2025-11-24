# Web Search

Web search gives KIRO CLI access to current information from the internet. Ask about recent updates, verify facts, or research unfamiliar topics, and the assistant will search the web and cite sources.

## Overview

Two tools work together for web information retrieval:

- **web_search**: Searches the web, returns titles, URLs, and snippets
- **web_fetch**: Fetches detailed content from specific URLs

## Basic Usage

### Automatic Search

The assistant automatically uses web search when appropriate:

```
> What's the latest version of React?

Searching the web for: latest React version

Here's what I found:
React 18.3.1 is the current stable version, released in April 2024...

References:
[1] React - Official Documentation - https://react.dev
[2] React Releases - GitHub - https://github.com/facebook/react/releases
```

### Explicit Search Request

```
> Search for AWS Lambda pricing in us-east-1

Searching the web for: AWS Lambda pricing us-east-1

Here's what I found:
AWS Lambda pricing in us-east-1 is $0.20 per 1M requests...

References:
[1] AWS Lambda Pricing - https://aws.amazon.com/lambda/pricing/
```

### Fetching Page Content

```
> Get the installation instructions from https://docs.example.com/install

Fetching content from: https://docs.example.com/install

The installation process requires:
1. Install Node.js 18 or higher
2. Run npm install...
```

## Permission Model

Both tools require approval on first use:

```
> Search for Python 3.12 features

web_search wants to search the web for: Python 3.12 features
Allow? (y/n/t for trust this session):
```

**Options**:
- `y` - Allow once
- `n` - Deny
- `t` - Trust for this session

**Permanent Trust**: Add to agent config (`~/.kiro/agents/default.json`):
```json
{
  "allowedTools": ["web_search", "web_fetch"]
}
```

## Fetch Modes

### Selective (Default)
Extracts sections around search terms:
```
> Fetch https://example.com/docs with search terms "installation setup"
```
Returns ~10 lines before and after matches. Best for targeted information.

### Truncated
Gets first 8KB of content:
```
> Fetch https://example.com/article in truncated mode
```
Good for general purpose reading.

### Full
Retrieves complete content (up to 10MB):
```
> Fetch https://example.com/page in full mode
```
Use sparingly for comprehensive analysis.

## Usage Examples

### Example 1: Current Information
```
> What are the system requirements for macOS Sonoma?

Searching the web for: macOS Sonoma system requirements

Here's what I found:
macOS Sonoma requires:
- Mac models from 2018 or later
- 4GB of memory minimum...

References:
[1] macOS Sonoma - Apple Support - https://support.apple.com/...
```

### Example 2: Verify Facts
```
> Has Python 3.13 been released yet?

Searching the web for: Python 3.13 release status

Here's what I found:
Python 3.13.0 was released on October 7, 2024...

References:
[1] Python Release Schedule - https://peps.python.org/pep-0719/
```

### Example 3: Research Topic
```
> What is WebAssembly?

Searching the web for: WebAssembly explanation

Here's what I found:
WebAssembly (Wasm) is a binary instruction format that enables...

References:
[1] WebAssembly - Official Site - https://webassembly.org
[2] MDN Web Docs - https://developer.mozilla.org/...
```

### Example 4: Detailed Content
```
> Read the pricing section from https://aws.amazon.com/lambda/pricing/

Fetching content from: https://aws.amazon.com/lambda/pricing/
Mode: selective, search terms: pricing

AWS Lambda pricing includes:
- $0.20 per 1 million requests
- $0.0000166667 per GB-second of compute time...
```

## Content Compliance

The assistant follows strict rules when using web content:

### Attribution
- All sources cited with inline links: [description](url)
- "References:" section lists sources at end
- Sequential numbering: [1], [2], [3]

### Verbatim Limits
- Maximum 30 consecutive words from any source
- Content is paraphrased and summarized
- Original meaning preserved

### Source Quality
- Prioritizes recent sources (publication dates)
- Prefers official documentation
- Assesses domain authority

## When to Use

✅ **Use for**:
- Current events, news, recent updates
- Latest versions, pricing, specifications
- Verifying frequently changing information
- Researching unfamiliar technologies

❌ **Don't use for**:
- Basic programming concepts
- Historical facts
- Code in your repository
- Topics not requiring current info

## Configuration

### Trust Tools Permanently
```bash
# Edit agent config
~/.kiro/agents/default.json
```

```json
{
  "allowedTools": ["web_search", "web_fetch"]
}
```

## Limitations

### Technical Limits
- **Size**: 10MB max per page
- **Timeout**: 30 seconds
- **Redirects**: 10 maximum
- **Content**: HTML only
- **Retries**: 3 automatic attempts

### Regional Availability
- **Available**: Most regions (uses us-east-1)
- **Not available**: eu-central-1 (Frankfurt)

Tools are hidden in unsupported regions.

## Troubleshooting

### "Tool requires approval"
**Solution**: Press `y` (once), `t` (session), or add to agent config (permanent).

### "Web search not available in your region"
**Solution**: You're in eu-central-1 where the feature isn't available yet.

### "Failed to fetch URL"
**Causes**:
- Page too large (>10MB)
- Timeout (>30s)
- Too many redirects (>10)
- Not HTML content
- Network issues

**Solution**: Try different URL or use truncated mode.

### "No results found"
**Solution**: Rephrase query with different keywords or break into smaller searches.

## Best Practices

1. **Let it auto-search** - Assistant knows when to search
2. **Be specific** - "AWS Lambda pricing us-east-1" > "Lambda costs"
3. **Check dates** - Prioritize recent sources
4. **Use selective fetch** - Only fetch full pages when needed
5. **Trust wisely** - Permanent trust only if used frequently

## Privacy & Security

- Queries processed through AWS infrastructure
- No browsing history stored between sessions
- URLs fetched on-demand, not cached
- No sharing with third parties
- Standard AWS service logging applies

## Related Features

- **Built-in Tools**: See [Built-in Tools](built-in-tools.md) for all available tools
- **Agent Configuration**: See [Agent Format](agent-format.md) for configuration details
