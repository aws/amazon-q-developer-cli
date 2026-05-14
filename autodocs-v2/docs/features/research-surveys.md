---
doc_meta:
  title: Research Surveys
  description: Optional in-session feedback surveys to help improve Kiro CLI
  category: feature
  keywords: [survey, feedback, rating, research, telemetry, ctrl+y]
  related: [telemetry-privacy-settings, planning-agent]
  validated: 2026-05-11
  commit: 65011fb52
  status: validated
  testable_headless: false
---

# Research Surveys

Optional in-session feedback surveys to help improve Kiro CLI.

## Overview

Kiro CLI may occasionally prompt you to rate your experience through short in-session surveys. These surveys help the team understand what's working well and what needs improvement. Participation is entirely optional—you can dismiss any survey prompt and continue working.

Surveys appear as a blue notification bar above the prompt. Press `Ctrl+Y` to open the survey panel, or simply continue typing to dismiss.

## Survey Types

### Session Feedback

Appears after completing several conversation turns. Asks about your overall experience with Kiro CLI.

**Trigger**: After 3 completed assistant turns  
**Questions**: Experience rating, optional feedback, optional email for follow-up  
**Sampling**: 5% of users  
**Cooldown**: 90 days between prompts

### Plan Quality

Appears after the planning agent hands off to execution. Asks how well the plan captured your intent.

**Trigger**: When switching from planner to execution agent  
**Questions**: Plan quality rating, optional feedback  
**Sampling**: 10% of users  
**Cooldown**: 30 days between prompts

### Implementation Quality

Appears after all plan tasks complete. Asks about the quality of the implementation.

**Trigger**: When all plan tasks are marked complete  
**Questions**: Implementation rating, optional feedback, optional email  
**Sampling**: Only shown if plan quality survey was shown in the same session

## Interacting with Surveys

### Accepting a Survey

When a survey prompt appears:

```
┌─────────────────────────────────────────────────────────────┐
│ How did Kiro do?                              ctrl+y to rate│
└─────────────────────────────────────────────────────────────┘
```

Press `Ctrl+Y` to open the survey panel.

### Answering Questions

Use arrow keys to navigate rating options, then press `Enter` to select:

```
How would you rate your experience with Kiro CLI today?
> Excellent
  Good
  Fair
  Poor
  Very poor
```

For text questions, type your response and press `Enter`:

```
Do you have additional feedback about this experience?
> The code suggestions were helpful
```

### Dismissing a Survey

To skip a survey without answering:
- Press `Esc` while the panel is open
- Simply type a message instead of pressing `Ctrl+Y`
- The prompt auto-dismisses when you start your next task

Dismissing counts toward the cooldown period—you won't be prompted again until the cooldown expires.

## Examples

### Example 1: Complete a Survey

```
# Survey prompt appears after your third conversation turn
How did Kiro do?                                ctrl+y to rate

# Press Ctrl+Y to open
# Use arrows to select "Good", press Enter
# Type feedback: "Fast responses, helpful suggestions"
# Press Enter to skip email, or enter your email for follow-up
# Survey submits, shows "Thanks for your feedback"
```

### Example 2: Dismiss and Continue Working

```
# Survey prompt appears
How did the planning agent do?                  ctrl+y to rate

# You're busy, so just type your next message:
> implement the login feature

# Survey prompt disappears, cooldown starts
```

### Example 3: Plan and Implementation Surveys

```
# After /plan completes and hands off to execution:
How did the planning agent do?                  ctrl+y to rate

# If you answer (or dismiss), and later all tasks complete:
How was the implementation?                     ctrl+y to rate
```

## Privacy

Survey responses are sent to AWS's internal feedback service (Aperture). The data collected includes:

- Your rating selections
- Any text feedback you provide
- An anonymous user ID (hash of hostname + username)
- Session ID for correlation
- Whether you're an internal AWS user

**Email is optional**: Only collected if you choose to share it for follow-up research.

**No code or conversation content** is included in survey submissions.

## Troubleshooting

### Issue: Survey Keeps Appearing

**Symptom**: Same survey prompt appears repeatedly  
**Cause**: Cooldown period hasn't elapsed  
**Solution**: This shouldn't happen—cooldown starts when the prompt is shown. If it persists, the state file may be corrupted. Delete `~/.kiro/settings/survey_state.json` to reset.

### Issue: Can't Dismiss Survey Prompt

**Symptom**: Blue bar won't go away  
**Cause**: Waiting for input  
**Solution**: Press `Esc`, type any message, or press `Ctrl+Y` then `Esc` to close the panel.

### Issue: Survey Panel Not Responding

**Symptom**: Arrow keys or Enter not working  
**Cause**: Focus may be elsewhere  
**Solution**: The survey panel captures all input when open. Press `Esc` to close and try again.

## Related

- [Telemetry & Privacy Settings](../settings/telemetry-privacy-settings.md) - Other privacy controls
- [Planning Agent](planning-agent.md) - Triggers plan quality survey

## Technical Details

**State Storage**: `~/.kiro/settings/survey_state.json`

**Eligibility**: Determined once per survey via random sampling at the configured rate. Result is cached—you're either in the sample or not for the lifetime of your installation.

**Cooldown**: Starts when the prompt is shown (not when answered). Dismissing or answering both trigger the cooldown.

**Rate Limiting**: The feedback service limits submissions to 100 requests per 5 minutes per IP. If rate-limited, you'll see a warning but no data is lost.

**Environment Overrides** (for testing):
- `KIRO_SURVEY_SAMPLE_RATE` - Override sampling rate (0-1)
- `KIRO_SURVEY_COOLDOWN_DAYS` - Override cooldown period
- `KIRO_APERTURE_URL` - Override submission endpoint
