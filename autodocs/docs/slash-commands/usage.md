---
doc_meta:
  validated: 2026-04-24
  commit: 22dc5f71
  status: validated
  testable_headless: false
  category: slash_command
  title: /usage
  description: Show billing and credits information for current plan
  keywords: [usage, billing, credits, cost, limit, overage, subscription, plan]
  related: [context]
---

# /usage

Show billing and credits information for current plan.

## Overview

Displays your plan's billing usage, including subscription tier, usage breakdowns per resource type, bonus credits, purchased add-on credit packs, and a link to manage your account. Also handles displaying the monthly limit reached message when your request quota is exhausted.

## Usage

```
/usage
```

No arguments or subcommands. Output is printed inline (no panel overlay).

## Output

The command prints directly to the terminal (stderr). Content adapts based on plan type:

### Header

```
Estimated Usage | resets on {date} | {plan_name}
```

### Bonus Credits (shown immediately after header)

Single bonus credit:
```
🎁 Bonus credits: {used:.2}/{total:.0} credits used, expires in {days} days
```

Multiple bonus credits:
```
🎁 Bonus Credits:
   {name} - {used:.2}/{total:.0} used ({days} days left)
```

### Usage Breakdowns

Each resource type shows:
- **With a usage limit**: A progress bar with `({used:.2} of {limit:.0} covered in plan)` and integer percentage
- **Without a usage limit** (e.g. credit-pooling with no per-user cap): Only `({used:.2} used)` with no progress bar

Progress bar uses `█` characters in brand color for filled portion and `█` in secondary color for the remainder. Width is capped at `min(terminal_width, 80)`.

### Additional Credits

Shown only for non-enterprise users whose plan supports add-on credits (`overage_capable`) or who already own packs. Displays:
- **Active pack**: The earliest-expiry pack with remaining credits (FIFO), showing `{used:.2} of {total:.0} used, expires {date}`
- **Inactive packs**: Summarized as `{used:.2} of {total:.0} credits across {N} pack(s)`
- **No packs purchased**: "Add-on credits available for purchase"

Enterprise users never see this section.

### Footer

- Non-enterprise users who can purchase add-on credits: "To manage your plan or purchase add-on credits navigate to app.kiro.dev/account/usage"
- Non-enterprise users without add-on capability: "To manage your plan navigate to app.kiro.dev/account/usage"
- Enterprise users: "Since your account is through your organization, for account management please contact your account administrator."

### Tip

After the output, a tip is shown:
```
Tip: to see context window usage, run /context
```

## Monthly Limit Reached

When you reach your monthly request limit, Kiro displays a warning based on your account type:

**Enterprise users**: "Contact your administrator for account management." with reset date.

**Legacy Q Developer Pro users**: Suggests cancelling the legacy subscription and purchasing a Kiro subscription for increased limits. Links to kiro.dev/pricing.

**Kiro users with overages enabled (can upgrade)**: "You've used your monthly included requests and are now using overages. Upgrade your plan to get more included requests and avoid overage charges." Links to kiro.dev/pricing.

**Kiro users with overages enabled (cannot upgrade)**: "You've used your monthly included requests and are now using overages." with reset date.

**Kiro users who can enable overages (can upgrade)**: "You can enable overages to continue making requests, or upgrade your plan for more included requests." Links to kiro.dev/pricing.

**Kiro users who can enable overages (cannot upgrade)**: "You can enable overages to continue making requests." with reset date.

**Kiro users who can upgrade (no overage support)**: "Upgrade your plan for increased limits." Links to kiro.dev/pricing.

**Kiro users on highest tier**: Shows only the reset date.

All messages indicate when your limit will reset (the first of the next month).

## Examples

### Example 1: Pro User with Bonus Credits and Add-on Pack

```
/usage
```

**Output**:
```
Estimated Usage | resets on June 1, 2026 | Pro

🎁 Bonus credits: 50.00/100 credits used, expires in 14 days

Requests (245.00 of 500 covered in plan)
█████████████████████████████████████████████████████████████████████████████████ 49%

Additional credits
12.00 of 50 used, expires Jun 18, 2026

To manage your plan or purchase add-on credits navigate to app.kiro.dev/account/usage

Tip: to see context window usage, run /context
```

### Example 2: Free User Near Limit

```
/usage
```

**Output**:
```
Estimated Usage | resets on June 1, 2026 | Free

Requests (95.00 of 100 covered in plan)
█████████████████████████████████████████████████████████████████████████████████ 95%

To manage your plan navigate to app.kiro.dev/account/usage

Tip: to see context window usage, run /context
```

### Example 3: Enterprise User

Enterprise users do not see the Additional Credits section:

```
/usage
```

**Output**:
```
Estimated Usage | resets on June 1, 2026 | Enterprise

Requests (1250.00 of 5000 covered in plan)
█████████████████████████████████████████████████████████████████████████████████ 25%

Since your account is through your organization, for account management please contact your account administrator.

Tip: to see context window usage, run /context
```

### Example 4: Pooled Credits (No Per-User Limit)

```
/usage
```

**Output**:
```
Estimated Usage | resets on June 1, 2026 | Enterprise

Requests (320.00 used)

Since your account is through your organization, for account management please contact your account administrator.

Tip: to see context window usage, run /context
```

No progress bar is shown when there is no per-user cap.

## Progress Bar Colors

- Brand color: Under 90% usage
- Warning color: 91-99% usage
- Error color: 100%+ usage (over limit)

## Error States

| State | Output |
|-------|--------|
| Feature not supported | "Plan: Q Developer Pro" + "Your plan is managed by admin" |
| Backend error | "⚠️ Warning: Could not retrieve usage information from backend" + error details |

## Troubleshooting

### Issue: No Usage Data

**Symptom**: Empty or error output
**Cause**: Backend request failed or feature not supported
**Solution**: Check your network connection and login status. Try `/whoami` to verify authentication.

### Issue: Monthly Limit Reached

**Symptom**: Warning message about reaching monthly limit
**Cause**: You've used all your included requests for the month
**Solution**: Options depend on your account type: enable overages, upgrade your plan at kiro.dev/pricing, or wait until limits reset on the first of next month.

### Issue: Plan Shows as Unknown

**Symptom**: Plan name not displayed in header
**Cause**: Plan information not available from backend
**Solution**: Your usage data is still valid. Contact support if this persists.

## Related Features

- [/context](context.md) - View context window usage

## Limitations

- Shows estimated account-level usage (final billing may differ slightly)
- Requires active backend connectivity
- Bonus credits only shown if you have any active promotions
- Per-session token counts are not available via this command

## Technical Details

**Data source**: `get_billing_usage_data()` via the REST billing API (not ACP).

**Progress bar width**: `min(terminal_width, 80)` characters.

**Add-on credit ordering**: FIFO by expiry timestamp. The single active pack is the earliest-expiring with remaining credits.
