---
doc_meta:
  validated: 2026-04-30
  commit: be2c1347
  status: validated
  testable_headless: false
  category: slash_command
  title: /usage
  description: Show account-level usage limits and subscription plan information
  keywords: [usage, billing, credits, plan, limits, subscription, tokens]
  related: [model, settings]
---

# /usage

Show account-level usage limits and subscription plan information.

## Overview

Retrieves your account's usage limits from the API, including subscription plan details, usage breakdowns by resource type, and bonus credits with expiry information.

**Note**: This shows account-level usage, not per-session token counts. Token-level metrics for individual sessions are not currently available.

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

## FAQ

### How do I check token usage for my current session?

Per-session token counts are not currently available. `/usage` shows account-level usage only, which aggregates all your activity.

### What's the difference between cached and non-cached tokens?

Token caching is handled automatically by the service. The `/usage` command shows aggregate usage — it doesn't break down cached vs non-cached tokens.

### When does usage reset?

Usage resets based on your billing cycle. The exact reset date depends on your subscription plan. Check your account settings for billing cycle details.

### What counts as an "agentic request"?

An agentic request is a conversation turn where the agent uses tools. Simple Q&A without tool use may be counted differently depending on your plan.

## Limitations

- Shows account-level usage only (not per-session)
- Token-level breakdown not available
- Enterprise users see "Your plan is managed by admin" message
- Requires valid API authentication

## Technical Details

Calls `get_usage_limits()` API to retrieve account-level subscription and usage data.

## Troubleshooting

### Issue: "Your plan is managed by admin"

**Symptom**: Message says plan is managed by admin  
**Cause**: Enterprise account with centrally managed billing  
**Solution**: Contact your organization's admin for usage details

### Issue: Failed to retrieve usage information

**Symptom**: Error message about retrieval failure  
**Cause**: API authentication or connectivity issue  
**Solution**: Check your login status with `/whoami` and re-authenticate if needed

## Related

- [/model](model.md) - Switch models mid-session
- [chat.defaultModel](../settings/default-model.md) - Set default model
