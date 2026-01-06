---
doc_meta:
  validated: 2026-01-05
  commit: a1d370b5
  status: validated
  testable_headless: true
  category: settings-group
  title: API and Service Settings
  description: Settings for API timeouts and service configurations
  keywords: [settings, api, timeout, service, network]
---

# API and Service Settings

Configure API timeouts and service-related settings for Kiro CLI.

## api.timeout

API request timeout in seconds.

### Overview

Sets the timeout duration for API requests made by Kiro CLI. Affects requests to AI models, external services, and other network operations.

### Usage

```bash
kiro-cli settings api.timeout 30
```

**Type**: Number  
**Default**: `30`  
**Unit**: Seconds

### Examples

```bash
# Increase for slow connections
kiro-cli settings api.timeout 60

# Decrease for fast networks
kiro-cli settings api.timeout 15

# Check current timeout
kiro-cli settings api.timeout
```

### Timeout Guidelines

- **15s**: Fast networks, quick failure detection
- **30s**: Balanced (default)
- **60s**: Slow networks, complex requests
- **120s**: Very slow networks or large requests

### Use Cases

**Increase timeout for**:
- Slow internet connections
- Complex AI model requests
- Large file operations
- Unstable networks

**Decrease timeout for**:
- Fast, reliable connections
- Quick failure detection
- Interactive responsiveness