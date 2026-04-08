---
doc_meta:
  validated: 2025-12-19
  commit: 57090ffe
  status: validated
  testable_headless: false
  category: slash_command
  title: /usage
  description: Show billing and credits information for current session
  keywords: [usage, billing, credits, cost]
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
