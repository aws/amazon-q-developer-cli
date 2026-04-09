---
doc_meta:
  validated: 2026-04-09
  commit: a1d370b5
  status: validated
  testable_headless: true
  category: settings-group
  title: Telemetry and Privacy Settings
  description: Settings for telemetry collection and privacy controls
  keywords: [settings, telemetry, privacy, analytics]
---

# Telemetry and Privacy Settings

Configure telemetry collection and privacy controls for Kiro CLI.

## telemetry.enabled

Enable or disable telemetry collection.

### Overview

Controls whether Kiro CLI collects anonymous usage data to improve the product. When enabled, sends usage statistics and error reports to help development team understand how the tool is used.

This is a **global-only** setting and cannot be overridden per-workspace.

### Usage

```bash
kiro-cli settings telemetry.enabled true
```

**Type**: Boolean  
**Default**: `true`  
**Scope**: Global only

### Environment Variables

Telemetry can also be disabled via environment variables, which take precedence over the setting:

- `KIRO_DISABLE_TELEMETRY` - Set to any value to disable telemetry
- `Q_DISABLE_TELEMETRY` - Set to any value to disable telemetry

```bash
# Disable telemetry via environment variable
export KIRO_DISABLE_TELEMETRY=1
```

### Examples

```bash
# Enable telemetry
kiro-cli settings telemetry.enabled true

# Disable telemetry
kiro-cli settings telemetry.enabled false

# Check status
kiro-cli settings telemetry.enabled
```

### Privacy

- No personal data collected
- Anonymous usage statistics only
- Can be disabled at any time

---

## telemetryClientId

Legacy client identifier for telemetry data.

### Overview

A legacy setting that stores a client identifier for telemetry collection. The actual client ID is now managed internally in the database state.

This is a **global-only** setting and cannot be overridden per-workspace.

### Usage

```bash
kiro-cli settings telemetryClientId "client-123"
```

**Type**: String  
**Default**: Auto-generated  
**Scope**: Global only

### Examples

```bash
# Set client ID
kiro-cli settings telemetryClientId "my-client-id"

# Check current ID
kiro-cli settings telemetryClientId

# Clear ID
kiro-cli settings --delete telemetryClientId
```
