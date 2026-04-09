---
doc_meta:
  validated: 2026-04-09
  commit: 4ae084db
  status: validated
  testable_headless: true
  category: settings-group
  title: API and Service Settings
  description: Settings for API timeouts and service endpoint configurations
  keywords: [settings, api, timeout, service, network, endpoint, codewhisperer, oidc]
  related: [settings, default-model]
---

# API and Service Settings

Configure API timeouts and service endpoint settings for Kiro CLI.

## api.timeout

API request timeout in milliseconds.

### Overview

Sets the timeout duration for API requests made by Kiro CLI. Affects requests to AI models, external services, and other network operations.

### Usage

```bash
kiro-cli settings api.timeout 300000
```

**Type**: Number  
**Default**: `300000` (5 minutes)  
**Unit**: Milliseconds  
**Scope**: Workspace-overridable

### Examples

```bash
# Set 5-minute timeout (default)
kiro-cli settings api.timeout 300000

# Set 2-minute timeout for faster failure detection
kiro-cli settings api.timeout 120000

# Set 10-minute timeout for slow connections
kiro-cli settings api.timeout 600000

# Check current timeout
kiro-cli settings api.timeout
```

### Timeout Guidelines

| Duration | Milliseconds | Use Case |
|----------|-------------|----------|
| 1 minute | 60000 | Fast networks, quick failure detection |
| 2 minutes | 120000 | Balanced for most connections |
| 5 minutes | 300000 | Default, handles complex requests |
| 10 minutes | 600000 | Very slow networks or large operations |

## api.codewhisperer.service

CodeWhisperer service endpoint URL.

### Overview

Overrides the default CodeWhisperer service endpoint. Used for custom deployments or testing.

### Usage

```bash
kiro-cli settings api.codewhisperer.service '{"endpoint": "https://custom.endpoint.com", "region": "us-east-1"}'
```

**Type**: Object (JSON with `endpoint` and `region` fields)  
**Default**: None (uses default AWS endpoint)  
**Scope**: Global only

## api.q.service

Q service endpoint URL.

### Overview

Overrides the default Q service endpoint. Used for custom deployments or testing.

### Usage

```bash
kiro-cli settings api.q.service '{"endpoint": "https://custom.q.endpoint.com", "region": "us-east-1"}'
```

**Type**: Object (JSON with `endpoint` and `region` fields)  
**Default**: None (uses default AWS endpoint)  
**Scope**: Global only

## api.oidc.scopePrefix

OIDC scope prefix for authentication.

### Overview

Configures the prefix used for OIDC authentication scopes. Used for custom identity provider configurations.

### Usage

```bash
kiro-cli settings api.oidc.scopePrefix "custom-prefix"
```

**Type**: String  
**Default**: None  
**Scope**: Global only

## api.kiroauth.service

Kiro authentication service endpoint.

### Overview

Overrides the default Kiro authentication service endpoint. Used for custom deployments.

### Usage

```bash
kiro-cli settings api.kiroauth.service "https://custom.auth.endpoint.com"
```

**Type**: String  
**Default**: None (uses default endpoint)  
**Scope**: Global only

## Troubleshooting

### Request Timeouts

If you experience frequent timeouts:

```bash
# Increase timeout for slow connections
kiro-cli settings api.timeout 600000
```

### Custom Endpoint Issues

For custom endpoint configurations, ensure:
- The endpoint URL is accessible from your network
- The region matches your deployment
- Authentication credentials are valid for the custom endpoint

## Related

- [Settings Command](../commands/settings.md) - Managing all CLI settings
- [Default Model Settings](default-model.md) - AI model configuration
