---
doc_meta:
  validated: 2026-04-09
  commit: 4ae084db
  status: validated
  testable_headless: false
  category: slash_command
  title: /usage
  description: Show account-level usage limits and subscription plan information
  keywords: [usage, billing, credits, plan, limits, subscription]
  related: [model, settings]
---

# /usage

Show account-level usage limits and subscription plan information.

## Overview

Retrieves your account's usage limits from the API, including subscription plan details, usage breakdowns by resource type, and bonus credits with expiry information.

## Usage

```
/usage
```

## Output

Returns JSON data containing:
- `planName` - Your subscription plan title
- `overagesEnabled` - Whether overage charges are enabled
- `isEnterprise` - Whether this is an enterprise-managed plan
- `usageBreakdowns[]` - Array of usage by resource type:
  - `resourceType`, `displayName`
  - `used`, `limit`, `percentage`
  - `currentOverages`, `overageRate`, `overageCharges`, `currency`
- `bonusCredits[]` - Array of bonus credits:
  - `name`, `used`, `total`, `daysUntilExpiry`

## Limitations

- Enterprise users see "Your plan is managed by admin" message
- Requires valid API authentication

## Technical Details

Calls `get_usage_limits()` API to retrieve account-level subscription and usage data.

## Examples

### Example 1: View Usage

```
/usage
```

**Output**:
```
Plan: Q Developer Pro | 2 usage breakdowns
```

With JSON data:
```json
{
  "planName": "Q Developer Pro",
  "overagesEnabled": false,
  "isEnterprise": false,
  "usageBreakdowns": [
    {
      "resourceType": "AGENTIC_REQUESTS",
      "displayName": "Agentic requests",
      "used": 150.0,
      "limit": 1000.0,
      "percentage": 15,
      "currentOverages": 0.0,
      "overageRate": 0.0,
      "overageCharges": null,
      "currency": "USD"
    }
  ],
  "bonusCredits": [
    {
      "name": "Welcome bonus",
      "used": 10.0,
      "total": 50.0,
      "daysUntilExpiry": 25
    }
  ]
}
```

## Troubleshooting

### Issue: "Your plan is managed by admin"

**Symptom**: Message says plan is managed by admin  
**Cause**: Enterprise account with centrally managed billing  
**Solution**: Contact your organization's admin for usage details

### Issue: Failed to retrieve usage information

**Symptom**: Error message about retrieval failure  
**Cause**: API authentication or connectivity issue  
**Solution**: Check your login status with `/whoami` and re-authenticate if needed
