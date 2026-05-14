---
doc_meta:
  validated: 2026-05-11
  commit: 65011fb52
  status: validated
  testable_headless: false
  category: feature
  title: Research Surveys
  description: Optional in-session surveys to rate your experience with Kiro CLI, plan quality, and implementation quality
  keywords: [survey, feedback, rating, research, aperture]
  related: [session-management, planning-agent]
---

# Research Surveys

Optional in-session surveys to rate your experience with Kiro CLI, plan quality, and implementation quality.

## Overview

Kiro CLI may occasionally prompt you to provide feedback through short surveys. These surveys help improve the product by collecting ratings and optional comments about your experience. Surveys are:

- **Optional** — dismiss anytime by pressing ESC or continuing to type
- **Sampled** — only a percentage of users see each survey
- **Cooldown-protected** — you won't be prompted repeatedly

## Survey Types

### Session Feedback

Appears after 3 completed assistant turns. Asks about your overall experience with Kiro CLI.

**Questions:**
1. How would you rate your experience with Kiro CLI today? (rating)
2. Do you have additional feedback? (optional text)
3. Email to join research panel (optional)

### Plan Quality

Appears when the planning agent hands off to the execution agent. Asks about how well the plan captured your intent.

**Questions:**
1. How well did Kiro's plan capture what you are trying to accomplish? (rating)
2. Do you have additional feedback? (optional text)

### Implementation Quality

Appears after all plan tasks complete (only if you saw the plan quality survey). Asks about the quality of the implementation.

**Questions:**
1. How would you rate the quality of Kiro's implementation of the plan? (rating)
2. Do you have additional feedback? (optional text)
3. Email for follow-up (optional)

## Usage

### Accepting a Survey

When a survey prompt appears as a blue bar above the input:

```
How did Kiro do?                                    ctrl+y to rate
```

Press `Ctrl+Y` to open the survey panel.

### Answering Questions

- **Rating questions**: Use arrow keys to select, Enter to confirm
- **Text questions**: Type your response, Enter to submit
- **Optional questions**: Press Enter with empty input to skip

### Dismissing

- Press `ESC` to close the survey panel without submitting
- Type a message while the prompt is showing to dismiss it
- The prompt auto-dismisses if you start a new task

## When Surveys Appear

Surveys only appear when:

1. You're in the sampled population for that survey
2. The cooldown period has elapsed since your last survey
3. No other UI is blocking (pending approvals, errors, queued messages)
4. The assistant isn't currently processing

### Trigger Conditions

| Survey | Trigger |
|--------|---------|
| Session Feedback | After 3 completed assistant turns |
| Plan Quality | When planner hands off to executor |
| Implementation Quality | When all plan tasks complete |

## Examples

### Example 1: Session Feedback

```
> Fix the bug in auth.ts

[assistant fixes the bug]

> Add tests for it

[assistant adds tests]

> Run the tests

[assistant runs tests]

How did Kiro do?                                    ctrl+y to rate
```

Press `Ctrl+Y`:

```
Research question                              1 of 3 questions
────────────────────────────────────────────────────────────────
How would you rate your experience with Kiro CLI today?

> Very poor
  Poor
  Fair
  Good
  Excellent

────────────────────────────────────────────────────────────────
ENTER to select and proceed | ESC to cancel and close
```

### Example 2: Dismissing by Typing

```
How did Kiro do?                                    ctrl+y to rate

> Now deploy to staging
```

The survey prompt disappears and your message is sent normally.

### Example 3: Plan Quality After Handoff

```
> /plan Add user authentication

[planner creates plan and hands off]

How did the planning agent do?                      ctrl+y to rate
```

## Troubleshooting

### Survey never appears

- You may not be in the sampled population (surveys use random sampling)
- The cooldown period may not have elapsed
- Check if `KIRO_SURVEY_SAMPLE_RATE=1` forces 100% sampling for testing

### Survey appears too often

- Each survey type has its own cooldown (typically 30-90 days)
- Dismissing a survey starts the cooldown timer
- Completing a survey also starts the cooldown

### Feedback not submitted

- Submissions are fire-and-forget; network errors are silently ignored
- A "Thanks for your feedback" toast confirms successful submission
- Rate limiting may temporarily block submissions (100 requests/5 min per IP)

## Technical Details

### Sampling and Cooldown

| Survey | Sample Rate | Cooldown |
|--------|-------------|----------|
| Session Feedback | 5% | 90 days |
| Plan Quality | 10% | 30 days |
| Implementation Quality | 100%* | 0 days* |

*Implementation survey only appears if plan survey was shown in the same session.

### Environment Variables

For testing or internal use:

- `KIRO_SURVEY_SAMPLE_RATE` — Override sample rate for all surveys (0-1)
- `KIRO_SURVEY_COOLDOWN_DAYS` — Override cooldown for all surveys
- `KIRO_SURVEY_SAMPLE_RATE_SESSION_FEEDBACK` — Per-survey rate override
- `KIRO_SURVEY_COOLDOWN_DAYS_PLAN_QUALITY` — Per-survey cooldown override

### State Persistence

Survey state is stored in `~/.kiro/settings/survey_state.json`:

- Eligibility (result of sampling roll)
- Last shown timestamp
- Last completed timestamp
- Dismiss count

## Limitations

- Surveys require network connectivity to submit
- No offline queuing — if submission fails, feedback is lost
- Cannot review or edit submitted responses
- Survey definitions are built into the CLI (not configurable)

## Related

- [Session Management](session-management.md) — Session lifecycle
- [Planning Agent](planning-agent.md) — Plan creation workflow
