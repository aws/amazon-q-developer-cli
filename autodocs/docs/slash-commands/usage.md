---
doc_meta:
  validated: 2026-04-24
  commit: 22dc5f71
  status: validated
  testable_headless: false
  category: slash_command
  title: /usage
  description: Show billing and credits information for current session
  keywords: [usage, billing, credits, cost, limit, overage, subscription]
  related: []
---

# /usage

Show billing and credits information for current session.

## Overview

Displays billing information including credits used and remaining for current conversation session.

## Usage

```
/usage
```

## Output

Shows:
- Credits used in session
- Remaining credits
- Cost breakdown

## Monthly Limit Reached

When you reach your monthly request limit, Kiro displays a message based on your account type:

**Enterprise users**: Contact your administrator for account management.

**Legacy Q Developer Pro users**: You can cancel your legacy subscription and purchase a Kiro subscription for increased limits.

**Kiro users with overages enabled**: You've used your monthly included requests and are now using overages. You can upgrade your plan to get more included requests.

**Kiro users who can enable overages**: You can enable overages to continue making requests, or upgrade your plan for more included requests.

**Kiro users who can upgrade**: Upgrade your plan for increased limits.

All messages include a link to the Kiro pricing page and indicate when your limit will reset (the first of the next month).

## Limitations

- Shows session data only
- Not available in all regions

## Technical Details

**Billing**: Based on token usage and model rates.

## Examples

### Example 1: View Usage

```
/usage
```

**Output**:
```
Session Usage:
  Input tokens: 1,234
  Output tokens: 5,678
  Total tokens: 6,912
  
  Estimated cost: $0.15
  Credits remaining: 9.85
```

## Troubleshooting

### Issue: No Usage Data

**Symptom**: Empty or zero usage  
**Cause**: New session or no API calls yet  
**Solution**: Usage tracked after first AI response

### Issue: Cost Seems Wrong

**Symptom**: Unexpected cost  
**Cause**: Different model rates  
**Solution**: Costs vary by model. Check model pricing.

### Issue: Monthly Limit Reached

**Symptom**: Error message about reaching monthly limit  
**Cause**: You've used all your included requests for the month  
**Solution**: Options depend on your account type - enable overages, upgrade your plan, or wait until the next month when limits reset.
