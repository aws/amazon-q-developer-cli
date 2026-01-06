---
doc_meta:
  validated: 2026-01-05
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

### Usage

```bash
kiro-cli settings telemetry.enabled true
```

**Type**: Boolean  
**Default**: `false`

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

Client identifier for telemetry data.

### Overview

Sets a unique client identifier used for telemetry collection. This helps correlate usage data while maintaining anonymity.

### Usage

```bash
kiro-cli settings telemetryClientId "client-123"
```

**Type**: String  
**Default**: Auto-generated

### Examples

```bash
# Set client ID
kiro-cli settings telemetryClientId "my-client-id"

# Check current ID
kiro-cli settings telemetryClientId

# Clear ID
kiro-cli settings --delete telemetryClientId
```