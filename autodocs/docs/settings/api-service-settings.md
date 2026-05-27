---
doc_meta:
  validated: 2026-02-25
  commit: d6d0a7b9
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

API request timeout in milliseconds.

### Overview

Sets the timeout duration for API requests made by Kiro CLI. Affects requests to AI models, external services, and other network operations.

### Usage

```bash
kiro-cli settings api.timeout 30000
```

**Type**: Number  
**Default**: `30000`  
**Unit**: Milliseconds

### Examples

```bash
# Increase for slow connections
kiro-cli settings api.timeout 60000

# Decrease for fast networks
kiro-cli settings api.timeout 15000

# Check current timeout
kiro-cli settings api.timeout
```

### Timeout Guidelines

- **15000**: Fast networks, quick failure detection (15s)
- **30000**: Balanced (default, 30s)
- **60000**: Slow networks, complex requests (60s)
- **120000**: Very slow networks or large requests (120s)

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