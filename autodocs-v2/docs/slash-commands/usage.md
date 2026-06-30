---
doc_meta:
  title: /usage
  description: Show account-level usage limits, add-on credits, and subscription plan information
  category: slash_command
  keywords: [usage, billing, credits, plan, limits, subscription, add-on, overages, pooled]
  related: [model, settings]
  validated: 2026-06-29
  commit: e854ba465
  status: validated
  testable_headless: false
---

# /usage

Show account-level usage limits, add-on credits, and subscription plan information.

## Overview

Retrieves your account's usage limits from the API, including subscription plan details, usage breakdowns by resource type, purchased add-on credit packs, and bonus credits with expiry information.

**Note**: This shows account-level usage, not per-session token counts. Token-level metrics for individual sessions are not currently available.

## Usage

```
/usage
```

## Output

The display adapts based on your plan type and capabilities:

### Usage Breakdowns

Each resource type shows:
- **With a usage limit**: A progress bar with `(X of Y covered in plan)` and percentage
- **Without a usage limit** (e.g. pooled enterprise credits with no per-user cap): Only consumption is shown as `(X used)` with no progress bar

### Additional Credits

Shown only for users whose plan supports add-on credits (`overageCapable`). Displays:
- **Active pack**: The pack currently being drawn from (earliest-expiry with remaining credits, FIFO order), with usage and expiry date
- **Inactive packs**: Summarized as a single combined line showing total credits across all remaining packs
- **No packs purchased**: A note that add-on credits are available for purchase

Users on free plans (overage-incapable) do not see this section.

### Call to Action

- Users who can purchase add-on credits see: "To manage your plan or purchase add-on credits navigate to app.kiro.dev/account/usage"
- Other users see: "To manage your plan navigate to app.kiro.dev/account/usage"
- Enterprise users see: "Your plan is managed by admin"

### JSON Data Fields

- `planName` — Your subscription plan title
- `overagesEnabled` — Legacy field (retained from post-paid overages model)
- `overageCapable` — Whether you can use/purchase add-on credits
- `isEnterprise` — Whether this is an enterprise-managed plan
- `usageBreakdowns[]` — Array of usage by resource type:
  - `resourceType`, `displayName`
  - `used`, `limit`, `percentage`
  - `hasLimit` — Whether this dimension has a real usage cap (false hides progress bar)
  - `currentOverages`, `overageRate`, `overageCharges`, `currency` (legacy fields)
- `addOnCredits[]` — Array of purchased prepaid add-on credit packs:
  - `used` — Credits consumed from this pack
  - `total` — Total credits in this pack
  - `expiresAt` — Formatted expiry date (e.g. "Jun 18, 2027") or null
  - `isActive` — Whether this is the pack currently being consumed (FIFO)
- `bonusCredits[]` — Array of bonus credits:
  - `name`, `used`, `total`, `daysUntilExpiry`

## Examples

### Example 1: Pro User with Add-on Credits

```
/usage
```

**Output**:
```
Agentic requests (850.00 of 1000 covered in plan)
████████████████████████████████████████████████████████████████████░░░░░░░░░░ 85%

Additional credits
42.50 of 100 used, expires Jun 18, 2027
0.00 of 200 credits across 1 pack

To manage your plan or purchase add-on credits navigate to app.kiro.dev/account/usage
```

### Example 2: User Without a Usage Limit (Pooled Credits)

```
/usage
```

**Output**:
```
Agentic requests (320.00 used)

To manage your plan navigate to app.kiro.dev/account/usage
```

No progress bar is shown when there is no per-user cap.

### Example 3: Pro User with No Add-on Packs Purchased

```
/usage
```

**Output**:
```
Agentic requests (150.00 of 1000 covered in plan)
████████████████░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░ 15%

Additional credits
Add-on credits available for purchase

To manage your plan or purchase add-on credits navigate to app.kiro.dev/account/usage
```

### Example 4: Free User (No Add-on Section)

```
/usage
```

**Output**:
```
Agentic requests (50.00 of 100 covered in plan)
██████████████████████████████████████░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░ 50%

To manage your plan navigate to app.kiro.dev/account/usage
```

Free users do not see the "Additional credits" section or the purchase prompt.

## FAQ

### How do I check token usage for my current session?

Per-session token counts are not currently available. `/usage` shows account-level usage only, which aggregates all your activity.

### What are add-on credits?

Add-on credits are prepaid credit packs you can purchase to extend your usage beyond your plan's included allowance. They are consumed in FIFO order (earliest-expiring pack first). Only users on plans that support add-on credits will see this section.

### Why don't I see a progress bar?

If your account uses pooled credits (e.g. enterprise credit pooling) with no per-user limit, only your consumption is shown without a progress bar or upper bound.

### When does usage reset?

Usage resets based on your billing cycle. The exact reset date depends on your subscription plan. Check your account settings for billing cycle details.

### What counts as an "agentic request"?

An agentic request is a conversation turn where the agent uses tools. Simple Q&A without tool use may be counted differently depending on your plan.

## Limitations

- Shows account-level usage only (not per-session)
- Token-level breakdown not available
- Enterprise users see "Your plan is managed by admin" message
- Requires valid API authentication
- Add-on credit section only visible to overage-capable users

## Technical Details

Calls `get_usage_limits()` API to retrieve account-level subscription and usage data. The backend returns a sentinel value (999999) as the usage limit for users without a per-user cap — when detected, the progress bar is hidden and only consumption is displayed.

Add-on credit packs are collected from `overage_credits` on each usage breakdown item. The active pack is determined by FIFO ordering: the earliest-expiring pack with remaining credits is designated active, and consumption is drawn from it first.

## Troubleshooting

### Issue: "Your plan is managed by admin"

**Symptom**: Message says plan is managed by admin
**Cause**: Enterprise account with centrally managed billing
**Solution**: Contact your organization's admin for usage details

### Issue: Failed to retrieve usage information

**Symptom**: Error message about retrieval failure
**Cause**: API authentication or connectivity issue
**Solution**: Check your login status with `/whoami` and re-authenticate if needed

### Issue: No "Additional credits" section visible

**Symptom**: You don't see any add-on credit information
**Cause**: Your plan does not support add-on credits (e.g. free tier)
**Solution**: Upgrade your plan at app.kiro.dev/account/usage to access add-on credits

## Related

- [/model](model.md) - Switch models mid-session
- [chat.defaultModel](../settings/default-model.md) - Set default model
